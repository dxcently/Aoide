//! aoided's ask puller: the asks each `eidolon web` door holds, kept in
//! `state/stage/asks.json` for the verbs that list and answer them.
//!
//! The puller lists the token files `<runtime>/eidolon/web-<port>.token` and
//! reads them; it never deletes, renames or sweeps one. A door is dialled
//! through `aoide-door`, whose checks run before any byte is written. A door's
//! `hello` carries its session's path, and the door binds to the one session
//! record whose `logPath` is that path, byte for byte: a door no record holds is
//! skipped. Each frame of its stream then moves the store under the stage lock:
//! an ask lands, a settle removes it, a hello clears what that door held (a
//! restarted door never inherits its old asks), and the end of the stream clears
//! it too.
//!
//! [`run`] is the one thread aoided starts. It spawns a reader for each door
//! not already followed: a reader's stream is a blocking read of its own. A door
//! that fails waits out [`BACKOFF`], and each failure is audited once per (port,
//! reason). [`Puller`] takes its paths as parameters, so a test names its own.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aoide_door::{check_runtime, runtime_from_env, token_path, Ask, Door, DoorError, Frame};
use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::untrusted::{clean_line, visible, visible_json_line};
use aoide_protocol::{append_audit, default_audit_log, AuditRecord, EventClass, Invocation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::common::{refuse_unapproving_door, require_args, stage_error};
use super::model::{load_stage, sessions_path, SessionRecord, SessionsFile};
use super::send::DOOR_CANNOT_APPROVE;

/// How often [`run`] lists the doors.
const TICK: Duration = Duration::from_secs(2);
/// How long a door that failed waits before it is dialled again.
const BACKOFF: Duration = Duration::from_secs(30);
/// The command the audit lines of this puller name.
const AUDIT_COMMAND: &str = "ask.puller";

/// The store file the puller owns, in the conducting stage directory.
pub fn asks_path() -> PathBuf {
    aoide_storage::fs::conducting_stage_dir().join("asks.json")
}

/// One open ask: the door it came from, and the frame as the door gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub session_id: String,
    pub port: u16,
    pub ask_id: u64,
    pub ask: Value,
    /// Unix seconds the puller first saw the ask.
    pub first_seen: u64,
    /// The door's token digest, as the puller read it at the stream's open.
    #[serde(default)]
    pub token_digest: String,
}

/// `asks.json`: the open asks, and the unix second the file was last written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsksFile {
    #[serde(default)]
    pub written: u64,
    #[serde(default)]
    pub asks: Vec<Entry>,
}

/// The port a token file's name names: `web-<port>.token`, with the port written
/// the way Rust writes a `u16`, so no two names name one port. Anything else is
/// not a door's token file.
pub fn port_of_token_name(name: &str) -> Option<u16> {
    let digits = name.strip_prefix("web-")?.strip_suffix(".token")?;
    let port: u16 = digits.parse().ok()?;
    (port.to_string() == digits).then_some(port)
}

/// The session a door binds to: the one record whose `logPath` is the hello's
/// `session`, byte for byte. No record, or more than one, is refused.
pub fn bind_session(hello: &str, sessions: &[SessionRecord]) -> Result<String, &'static str> {
    let mut found = sessions.iter().filter(|r| r.log_path.as_deref() == Some(hello));
    match (found.next(), found.next()) {
        (Some(record), None) => Ok(record.session_id.clone()),
        (None, _) => Err("unbound"),
        (Some(_), Some(_)) => Err("ambiguous-session"),
    }
}

/// Closes a door's stream, from the puller's own thread.
fn close(events: &aoide_door::Events) {
    if let Ok(closer) = events.closer() {
        let _ = closer.close();
    }
}

/// Applies one frame of a door's stream to the store and says whether it changed
/// the store. Only `hello`, `ask` and `ask-settled` change anything.
pub fn apply(file: &mut AsksFile, session_id: &str, port: u16, digest: &str, frame: &Frame, now: u64) -> bool {
    let before = file.asks.len();
    let of_door = |e: &Entry| e.session_id == session_id && e.port == port;
    match frame {
        Frame::Hello { .. } => {
            file.asks.retain(|e| !of_door(e));
            file.asks.len() != before
        }
        Frame::Ask(ask) => {
            let Ok(value) = serde_json::to_value(ask) else { return false };
            let ask_id = ask_id_of(ask);
            if let Some(entry) = file.asks.iter_mut().find(|e| of_door(e) && e.ask_id == ask_id) {
                let changed = entry.ask != value;
                entry.ask = value;
                return changed;
            }
            file.asks.push(Entry {
                session_id: session_id.to_string(),
                port,
                ask_id,
                ask: value,
                first_seen: now,
                token_digest: digest.to_string(),
            });
            true
        }
        Frame::AskSettled { ask_id, .. } => {
            file.asks.retain(|e| !(of_door(e) && e.ask_id == *ask_id));
            file.asks.len() != before
        }
        _ => false,
    }
}

/// The digest a store entry keeps for its door: the sha256 of the token's bytes,
/// in hex. The store keeps no token.
pub fn token_digest(token: &[u8]) -> String {
    aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(token))
}

/// Removes every ask of one door. Returns whether any was removed.
pub fn drop_door(file: &mut AsksFile, session_id: &str, port: u16) -> bool {
    let before = file.asks.len();
    file.asks.retain(|e| !(e.session_id == session_id && e.port == port));
    file.asks.len() != before
}

fn ask_id_of(ask: &Ask) -> u64 {
    match ask {
        Ask::Approval { ask_id, .. } | Ask::Question { ask_id, .. } => *ask_id,
    }
}

/// The audit's word for why a door was skipped or its store write failed.
fn reason(e: &DoorError) -> &'static str {
    match e {
        DoorError::NotLoopback(_) => "not-loopback",
        DoorError::RuntimeUnset => "runtime-unset",
        DoorError::Refused { .. } => "refused",
        DoorError::Unowned { .. } => "unowned",
        DoorError::Connect { .. } => "connect",
        DoorError::Io(_) => "io",
        DoorError::Malformed(_) => "malformed",
        DoorError::Protocol { .. } => "protocol",
        DoorError::Unauthorized => "unauthorized",
        DoorError::HostRefused => "host-refused",
        DoorError::TooLarge => "too-large",
        DoorError::Unexpected { .. } => "unexpected",
    }
}

fn now() -> u64 {
    super::conduct::unix_ts()
}

/// Applies one change to the store file under its lock, and writes the file only
/// when the change moved something. `Err` names what did not read or write.
pub fn update_store(path: &Path, change: impl FnOnce(&mut AsksFile) -> bool) -> Result<(), String> {
    let Some(dir) = path.parent() else { return Ok(()) };
    aoide_storage::fs::with_lock_in(dir, || {
        let mut file: AsksFile = load_stage(path)?;
        if !change(&mut file) {
            return Ok(());
        }
        file.written = now();
        let body = serde_json::to_string_pretty(&file).map_err(|e| format!("{}: {e}", path.display()))?;
        aoide_storage::fs::atomic_write_private(path, body.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    })
}

/// The ask as `ask list` shows it, and as its fingerprint covers it: every string
/// and every key in the visible form, so each character `visible` would spell is
/// written as its escape. Nothing is removed, so two asks that read alike but
/// differ in a hidden character are two asks.
pub fn visible_ask(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(visible(s)),
        Value::Array(items) => Value::Array(items.iter().map(visible_ask).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (visible(k), visible_ask(v))).collect()),
        other => other.clone(),
    }
}

/// The rows `ask list --json` shows: one per open ask, shown and fingerprinted.
fn rows_of(file: &AsksFile) -> Vec<Value> {
    file.asks
        .iter()
        .map(|entry| {
            let shown = visible_ask(&entry.ask);
            let fingerprint = fingerprint_of(&entry.session_id, entry.port, entry.ask_id, &shown);
            json!({
                "sessionId": entry.session_id,
                "port": entry.port,
                "askId": entry.ask_id,
                "ask": shown,
                "fingerprint": fingerprint,
                "firstSeen": entry.first_seen,
            })
        })
        .collect()
}

/// One text line for an open ask: what an approval would run, with its reason, or
/// what a question asks and the labels it offers. Every field is the visible form.
fn line_of(entry: &Entry, fingerprint: &str) -> String {
    let head = format!("{} port {} ask {}", visible(&entry.session_id), entry.port, entry.ask_id);
    let what = match entry.ask.get("kind").and_then(Value::as_str) {
        Some("approval") => format!(
            "approval for {} with {}; reason: {}",
            visible(entry.ask["tool"].as_str().unwrap_or_default()),
            visible_json_line(entry.ask.get("input").unwrap_or(&Value::Null)),
            entry.ask.get("reason").and_then(Value::as_str).map_or("none".to_string(), visible),
        ),
        Some("question") => format!(
            "question {}; offers: {}",
            visible(entry.ask["prompt"].as_str().unwrap_or_default()),
            offered(&entry.ask).iter().map(|label| visible(label)).collect::<Vec<_>>().join(", "),
        ),
        _ => "an ask this client does not read".to_string(),
    };
    format!("{head}: {what}; fingerprint {fingerprint}")
}

/// `ask list [--json]`: the open asks, each with the fingerprint an answer names,
/// and when the store was last written. Every field is in its visible form.
pub fn ask_list(_inv: &Invocation) -> Outcome {
    let cmd = "ask.list";
    let file: AsksFile = match load_stage(&asks_path()) {
        Ok(file) => file,
        Err(e) => return stage_error(cmd, e),
    };
    let rows = rows_of(&file);
    let lines: Vec<String> = file
        .asks
        .iter()
        .zip(&rows)
        .map(|(entry, row)| line_of(entry, row["fingerprint"].as_str().unwrap_or_default()))
        .collect();
    let n = rows.len();
    let body = if n == 0 { "no open asks".to_string() } else { lines.join("\n") };
    Outcome::ok(cmd, format!("{n} open ask(s), store written at {}\n{body}", file.written))
        .with_data(json!({ "asks": rows, "written": file.written }))
}

/// The fingerprint of one ask: the first 16 hex digits of the sha256 of the bytes
/// `ask list --json` shows for it, its session, port, ask id and visible ask.
pub fn fingerprint_of(session_id: &str, port: u16, ask_id: u64, shown: &Value) -> String {
    let body = json!({ "sessionId": session_id, "port": port, "askId": ask_id, "ask": shown });
    let bytes = serde_json::to_string(&body).unwrap_or_default();
    aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(bytes.as_bytes()))[..16].to_string()
}

/// The labels an ask offers the answer: an approval's two answers, or a
/// question's options. Read off the raw ask, since the door matches its own text.
fn offered(ask: &Value) -> Vec<String> {
    if let Some(answers) = ask.get("answers").and_then(Value::as_array) {
        return answers.iter().filter_map(Value::as_str).map(str::to_string).collect();
    }
    ask.get("options")
        .and_then(Value::as_array)
        .map(|options| options.iter().filter_map(|o| o.get("label")?.as_str()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// One audit line for a verb's outcome, with the door that called it. `text` is
/// the ask's visible JSON, carried as untrusted data; empty when there is no ask.
fn audit_ask(inv: &Invocation, command: &str, status: &str, message: &str, text: &str) {
    let log = inv.flags.get("audit-log").map(PathBuf::from).unwrap_or_else(default_audit_log);
    let _ = append_audit(
        &log,
        &AuditRecord {
            ts: now(),
            door: inv.door,
            class: EventClass::Audit,
            command: command.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            untrusted_data: (!text.is_empty()).then(|| text.to_string()),
        },
    );
}

/// Set when `ask watch` is interrupted: its loop ends and prints the closing line.
static WATCH_STOP: AtomicBool = AtomicBool::new(false);
const WATCH_POLL: Duration = Duration::from_millis(250);

extern "C" fn on_watch_sigint(_signum: libc::c_int) {
    WATCH_STOP.store(true, Ordering::SeqCst);
}

fn with_type(row: &Value, change: &str) -> Value {
    let mut row = row.clone();
    row["type"] = json!(change);
    row
}

/// The rows that left and the rows that came between two snapshots, each typed
/// `removed` or `added`.
fn changes(before: &[Value], after: &[Value]) -> Vec<Value> {
    let gone = before.iter().filter(|row| !after.contains(row)).map(|row| with_type(row, "removed"));
    let came = after.iter().filter(|row| !before.contains(row)).map(|row| with_type(row, "added"));
    gone.chain(came).collect()
}

fn change_text(row: &Value) -> String {
    format!(
        "{} {} port {} ask {}",
        row["type"].as_str().unwrap_or_default(),
        row["sessionId"].as_str().unwrap_or_default(),
        row["port"],
        row["askId"]
    )
}

/// Follows the store until `stop` is set, one line per change: an ask added or
/// removed. It starts from nothing, so the asks open at the start come out as
/// additions. The closing line is an `end` object, as the other watches end.
/// A store that cannot be read is skipped until it can.
pub fn watch_store(path: &Path, json_mode: bool, poll: Duration, stop: &AtomicBool, emit: &mut dyn FnMut(String)) {
    let mut seen: Vec<Value> = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        if let Ok(file) = load_stage::<AsksFile>(path) {
            let now = rows_of(&file);
            for row in changes(&seen, &now) {
                emit(if json_mode { row.to_string() } else { change_text(&row) });
            }
            seen = now;
        }
        thread::sleep(poll);
    }
    emit(if json_mode {
        json!({ "type": "end", "reason": "interrupted" }).to_string()
    } else {
        "watch ended: interrupted".to_string()
    });
}

/// `ask watch [--json]`: one line per change of the asks store, until Ctrl-C.
pub fn ask_watch(inv: &Invocation) -> Outcome {
    if !matches!(inv.door, aoide_protocol::Door::Cli) {
        return Outcome::refuse(
            "ask.watch",
            Kind::Refused,
            "ask watch runs from the terminal only; a daemon call would hold a loop".to_string(),
            "watch from the CLI",
            Fix::Run("aoide ask watch".to_string()),
        );
    }
    let json_mode = inv.flag_present("json");
    WATCH_STOP.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(libc::SIGINT, on_watch_sigint as *const () as libc::sighandler_t);
    }
    watch_store(&asks_path(), json_mode, WATCH_POLL, &WATCH_STOP, &mut |line| {
        use std::io::Write as _;
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    });
    Outcome::ok("ask.watch", "watched the asks store until interrupted").with_data(json!({ "reason": "interrupted" }))
}

/// What an answer's audit lines carry: the fingerprint of the ask it names (`none`
/// until an entry is found) and the ask's visible JSON, as untrusted data.
struct Trail<'a> {
    inv: &'a Invocation,
    fingerprint: String,
    ask: String,
}

impl Trail<'_> {
    fn audit(&self, status: &str, message: &str) {
        audit_ask(
            self.inv,
            "ask.answer",
            status,
            &format!("{message}; fingerprint {}", self.fingerprint),
            &self.ask,
        );
    }

    /// A refusal, audited with its reason, and the outcome that names the fix.
    fn refuse(&self, what: String, why: &str, reason: &str) -> Outcome {
        self.audit("refused", &what);
        Outcome::refuse("ask.answer", Kind::Refused, what, why, Fix::Run("aoide ask list".to_string()))
            .with_fields(json!({ "reason": reason }))
    }
}

/// `ask answer <session> <ask_id> <label> --expect <fingerprint> [--note <text>]`:
/// answers one open ask through its door. Refused before any request unless the
/// ask is open, the fingerprint is the one `ask list` shows and the label is one
/// the ask offers, and the door answering now holds the token its puller read.
/// A door's 204 settles the ask, a 409 drops it, a 422 keeps it.
pub fn ask_answer(inv: &Invocation) -> Outcome {
    let cmd = "ask.answer";
    let args = match require_args(inv, &["session", "ask_id", "label"]) {
        Ok(args) => args,
        Err(o) => return o,
    };
    if let Some(refusal) = refuse_unapproving_door(
        inv,
        cmd,
        "this door cannot answer an Eidolon ask".to_string(),
        DOOR_CANNOT_APPROVE,
        Fix::Run("aoide ask list".to_string()),
    ) {
        return refusal;
    }
    let mut trail = Trail { inv, fingerprint: "none".to_string(), ask: String::new() };
    let (session, label) = (args[0].as_str(), args[2].as_str());
    let Ok(ask_id) = args[1].trim().parse::<u64>() else {
        let what = format!("`{}` is not an ask id", clean_line(&args[1]));
        trail.audit("refused", &what);
        return Outcome::usage(cmd, what);
    };
    let expect = match inv.flags.get("expect").map(|e| e.trim().to_string()) {
        None => {
            return trail.refuse(
                format!("ask {ask_id} not answered: answer needs --expect"),
                "a bare ask id is refused: list the asks and copy the fingerprint you read",
                "expect-required",
            )
        }
        Some(e) if e.is_empty() => {
            return trail.refuse(
                format!("ask {ask_id} not answered: --expect is empty"),
                "the fingerprint is empty; list the asks again and copy it",
                "expect-empty",
            )
        }
        Some(e) => e,
    };
    let file: AsksFile = match load_stage(&asks_path()) {
        Ok(file) => file,
        Err(e) => {
            trail.audit("error", &e);
            return stage_error(cmd, e);
        }
    };
    let matching: Vec<&Entry> = file.asks.iter().filter(|e| e.session_id == session && e.ask_id == ask_id).collect();
    if matching.is_empty() {
        return trail.refuse(
            format!("ask {ask_id} of `{}` is not open", clean_line(session)),
            "nothing was sent; list the asks to see what is open",
            "not-open",
        );
    }
    let Some(entry) = matching.into_iter().find(|e| {
        fingerprint_of(&e.session_id, e.port, e.ask_id, &visible_ask(&e.ask)) == expect
    }) else {
        return trail.refuse(
            format!("ask {ask_id} is not the one named by --expect {expect}; nothing was sent"),
            "the ask changed or the fingerprint is wrong; list the asks again",
            "fingerprint-mismatch",
        );
    };
    trail.fingerprint = expect.clone();
    trail.ask = visible_json_line(&entry.ask);
    if !offered(&entry.ask).iter().any(|offer| offer == label) {
        return trail.refuse(
            format!("ask {ask_id} does not offer `{}`; nothing was sent", clean_line(label)),
            "answer with one of the labels `ask list` shows for it",
            "not-offered",
        );
    }
    let Some(runtime) = runtime_from_env(std::env::var_os("XDG_RUNTIME_DIR").as_deref()).ok() else {
        return trail.refuse(
            "XDG_RUNTIME_DIR is not set; the door's token has no other place to live".to_string(),
            "nothing was sent",
            "runtime-unset",
        );
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], entry.port));
    let door = match Door::new(addr, &runtime, &token_path(&runtime, entry.port)) {
        Ok(door) => door,
        Err(e) => return trail.refuse(format!("door refused: {}", reason(&e)), "nothing was sent", reason(&e)),
    };
    let pinned = match door.pin() {
        Ok(pinned) => pinned,
        Err(e) => return trail.refuse(format!("door refused: {}", reason(&e)), "nothing was sent", reason(&e)),
    };
    if pinned.digest(token_digest) != entry.token_digest {
        return trail.refuse(
            format!("ask {ask_id}: the door on port {} is not the one that held it; nothing was sent", entry.port),
            "the door restarted or another door holds the port; list the asks again",
            "door-changed",
        );
    }
    let note = inv.flags.get("note").map(String::as_str);
    let (port, ask) = (entry.port, entry.ask_id);
    let settle = || {
        let _ = update_store(&asks_path(), |f| {
            let before = f.asks.len();
            f.asks.retain(|e| !(e.session_id == session && e.port == port && e.ask_id == ask));
            f.asks.len() != before
        });
    };
    match pinned.answer(ask_id, label, note) {
        Ok(aoide_door::AnswerOutcome::Answered) => {
            settle();
            trail.audit("answered", &format!("ask {ask_id} of {session}: {}", clean_line(label)));
            Outcome::ok(cmd, format!("ask {ask_id} answered: {}", clean_line(label)))
        }
        Ok(aoide_door::AnswerOutcome::NotPending) => {
            settle();
            let what = format!("ask {ask_id} is no longer pending on its door; dropped from the store");
            trail.audit("settled", &what);
            Outcome::refuse(cmd, Kind::Refused, what, "the door already settled it", Fix::Run("aoide ask list".to_string()))
                .with_fields(json!({ "reason": "not-pending", "askId": ask_id.to_string() }))
        }
        Ok(aoide_door::AnswerOutcome::Rejected { reason }) => {
            let what = format!("ask {ask_id} does not take that answer: {}", clean_line(&reason));
            trail.audit("rejected", &what);
            Outcome::refuse(cmd, Kind::Refused, what, "the ask stays open", Fix::Run("aoide ask list".to_string()))
                .with_fields(json!({ "reason": "not-offered", "askId": ask_id.to_string() }))
        }
        Err(e) => {
            let why = reason(&e);
            trail.audit("error", &format!("ask {ask_id} of {session}: door did not answer ({why})"));
            Outcome::error(cmd, format!("the door did not answer ask {ask_id}: {why}"))
                .with_data(json!({ "reason": why, "askId": ask_id.to_string() }))
        }
    }
}

/// The puller's paths, and its books: the doors it follows, the doors in backoff,
/// and the audit lines it has written.
pub struct Puller {
    runtime: PathBuf,
    asks: PathBuf,
    sessions: PathBuf,
    audit_log: PathBuf,
    followed: Mutex<HashSet<u16>>,
    retry_at: Mutex<HashMap<u16, Instant>>,
    audited: Mutex<HashSet<(Option<u16>, &'static str)>>,
}

impl Puller {
    pub fn new(runtime: PathBuf, asks: PathBuf, sessions: PathBuf, audit_log: PathBuf) -> Arc<Puller> {
        Arc::new(Puller {
            runtime,
            asks,
            sessions,
            audit_log,
            followed: Mutex::new(HashSet::new()),
            retry_at: Mutex::new(HashMap::new()),
            audited: Mutex::new(HashSet::new()),
        })
    }

    /// Starts a reader for each token file whose door is not followed and not in
    /// backoff. Lists the runtime's `eidolon` directory only when the runtime
    /// passes its checks and that directory is a real directory.
    pub fn tick(self: &Arc<Self>) {
        if check_runtime(&self.runtime).is_err() {
            return;
        }
        let dir = self.runtime.join("eidolon");
        if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
            return;
        }
        let Ok(entries) = fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let Some(port) = entry.file_name().to_str().and_then(port_of_token_name) else { continue };
            if !entry.file_type().is_ok_and(|t| t.is_file()) || !self.claim(port) {
                continue;
            }
            let me = Arc::clone(self);
            thread::spawn(move || me.follow_and_release(port));
        }
    }

    fn claim(&self, port: u16) -> bool {
        if self.retry_at.lock().unwrap().get(&port).is_some_and(|at| Instant::now() < *at) {
            return false;
        }
        self.followed.lock().unwrap().insert(port)
    }

    fn follow_and_release(self: Arc<Self>, port: u16) {
        if let Err(why) = self.follow(port) {
            self.retry_at.lock().unwrap().insert(port, Instant::now() + BACKOFF);
            self.audit(Some(port), why, "skipped");
        }
        self.followed.lock().unwrap().remove(&port);
    }

    /// Reads one door's stream until it ends. `Ok` is a stream that ended, and
    /// `Err` the reason the door was skipped or failed.
    fn follow(&self, port: u16) -> Result<(), &'static str> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let door = Door::new(addr, &self.runtime, &token_path(&self.runtime, port)).map_err(|e| reason(&e))?;
        let pinned = door.pin().map_err(|e| reason(&e))?;
        let digest = pinned.digest(token_digest);
        let mut events = pinned.events().map_err(|e| reason(&e))?;
        let mut session: Option<String> = None;
        let mut failure = None;
        while let Some(next) = events.next() {
            let frame = match next {
                Ok(frame) => frame,
                Err(e) => {
                    failure = Some(reason(&e));
                    break;
                }
            };
            match &frame {
                Frame::Hello { session: hello, .. } => {
                    let sessions = load_stage::<SessionsFile>(&self.sessions).map(|f| f.sessions).unwrap_or_default();
                    match bind_session(hello, &sessions) {
                        Ok(id) => session = Some(id),
                        Err(why) => {
                            close(&events);
                            return Err(why);
                        }
                    }
                }
                Frame::Lagged { .. } => {
                    // The door dropped frames: this stream cannot be trusted to hold every
                    // ask, so it ends here and the next tick dials the door again.
                    close(&events);
                    break;
                }
                _ => {}
            }
            if let Some(id) = &session {
                self.update(|file| apply(file, id, port, &digest, &frame, now()));
            }
        }
        if let Some(id) = session {
            self.update(|file| drop_door(file, &id, port));
        }
        failure.map_or(Ok(()), Err)
    }

    /// Moves the store under the stage lock. A store that does not read or write
    /// is left as it is, and that is audited once.
    fn update(&self, change: impl FnOnce(&mut AsksFile) -> bool) {
        if update_store(&self.asks, change).is_err() {
            self.audit(None, "store-error", "error");
        }
    }

    /// One audit line per (door, reason), so a door that keeps failing writes one.
    fn audit(&self, port: Option<u16>, why: &'static str, status: &str) {
        if !self.audited.lock().unwrap().insert((port, why)) {
            return;
        }
        let message = match port {
            Some(port) => format!("eidolon web door on port {port}: {why}"),
            None => format!("asks store: {why}"),
        };
        let _ = append_audit(
            &self.audit_log,
            &AuditRecord {
                ts: now(),
                door: aoide_protocol::Door::Daemon,
                class: EventClass::Audit,
                command: AUDIT_COMMAND.to_string(),
                status: status.to_string(),
                message,
                untrusted_data: None,
            },
        );
    }
}

/// aoided's puller: lists the doors every [`TICK`] until the process ends, and
/// audits to `log_path`, the log aoided was given. With no runtime directory
/// there is no door to find, and it returns.
pub fn run(log_path: PathBuf) {
    let Ok(runtime) = runtime_from_env(std::env::var_os("XDG_RUNTIME_DIR").as_deref()) else { return };
    let puller = Puller::new(runtime, asks_path(), sessions_path(), log_path);
    loop {
        puller.tick();
        thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_door::fake::FakeDoor;
    use std::os::unix::fs::PermissionsExt;

    use super::super::model::write_stage;

    const LOG: &str = "/w/session-a.eid";

    fn hello(log: &str) -> String {
        format!(r#"{{"type":"hello","session":"{log}","cwd":"/w","model":"m","yolo":false,"pending":0,"protocol":1}}"#)
    }

    const ASK_7: &str = r#"{"type":"ask","kind":"question","ask_id":7,"prompt":"pick one","options":[{"label":"a"},{"label":"b"}]}"#;
    const ASK_8: &str = r#"{"type":"ask","kind":"question","ask_id":8,"prompt":"pick two","options":[{"label":"c"}]}"#;
    const SETTLE_7: &str = r#"{"type":"ask-settled","ask_id":7,"how":"answered","answer":"a"}"#;

    fn entry(session_id: &str, port: u16, ask_id: u64) -> Entry {
        Entry { session_id: session_id.into(), port, ask_id, ask: Value::Null, first_seen: 1, token_digest: String::new() }
    }

    /// A stage directory and a runtime directory of this test's own.
    struct Rig {
        runtime: PathBuf,
        asks: PathBuf,
        sessions: PathBuf,
        audit: PathBuf,
    }

    fn rig(tag: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("aoide-asks-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let runtime = root.join("run");
        fs::create_dir_all(&runtime).unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
        let stage = root.join("stage");
        fs::create_dir_all(&stage).unwrap();
        Rig {
            asks: stage.join("asks.json"),
            sessions: stage.join("sessions.json"),
            audit: root.join("audit.log"),
            runtime,
        }
    }

    impl Rig {
        fn puller(&self) -> Arc<Puller> {
            Puller::new(self.runtime.clone(), self.asks.clone(), self.sessions.clone(), self.audit.clone())
        }

        /// A `sessions.json` with one record per log path, each named `s<index>`.
        fn sessions(&self, logs: &[&str]) {
            let sessions = logs
                .iter()
                .enumerate()
                .map(|(i, log)| SessionRecord {
                    session_id: format!("s{i}"),
                    log_path: Some(log.to_string()),
                    ..Default::default()
                })
                .collect();
            write_stage(&self.sessions, &SessionsFile { schema_version: "0".into(), sessions }).unwrap();
        }
    }

    /// Polls the store until `want` holds of it, or fails the test.
    fn wait_for(path: &Path, want: impl Fn(&AsksFile) -> bool) -> AsksFile {
        for _ in 0..400 {
            if let Ok(file) = load_stage::<AsksFile>(path) {
                if want(&file) {
                    return file;
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("the asks store never reached the state the test waits for: {}", path.display());
    }

    /// Polls the audit log until a line holds `needle`, or fails the test.
    fn wait_for_audit(path: &Path, needle: &str) -> String {
        for _ in 0..400 {
            if let Ok(text) = fs::read_to_string(path) {
                if text.contains(needle) {
                    return text;
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("no audit line holds `{needle}`");
    }

    fn ask_ids(file: &AsksFile) -> Vec<u64> {
        file.asks.iter().map(|e| e.ask_id).collect()
    }

    #[test]
    fn a_token_name_names_a_port_only_when_the_port_is_canonical_decimal() {
        assert_eq!(port_of_token_name("web-8080.token"), Some(8080));
        assert_eq!(port_of_token_name("web-0.token"), Some(0));
        assert_eq!(port_of_token_name("web-80.token"), Some(80));
        assert_eq!(port_of_token_name("web-0080.token"), None, "a leading zero is another spelling");
        assert_eq!(port_of_token_name("web-+80.token"), None);
        assert_eq!(port_of_token_name("web-65536.token"), None, "past u16");
        assert_eq!(port_of_token_name("web-abc.token"), None);
        assert_eq!(port_of_token_name("web-80.token.bak"), None);
        assert_eq!(port_of_token_name("mail-80.token"), None);
    }

    #[test]
    fn a_door_binds_to_the_record_whose_log_path_is_its_hello_session_byte_for_byte() {
        let sessions = vec![
            SessionRecord { session_id: "s0".into(), log_path: Some("/w/a.eid".into()), ..Default::default() },
            SessionRecord { session_id: "s1".into(), log_path: Some("/w/A.eid".into()), ..Default::default() },
        ];
        assert_eq!(bind_session("/w/a.eid", &sessions), Ok("s0".to_string()));
        assert_eq!(bind_session("/w/A.eid", &sessions), Ok("s1".to_string()));
        assert_eq!(bind_session("/w/a.eid/", &sessions), Err("unbound"), "no trailing-slash match");
        assert_eq!(bind_session("a.eid", &sessions), Err("unbound"), "no stem match");
        let twins = vec![
            SessionRecord { session_id: "s0".into(), log_path: Some("/w/a.eid".into()), ..Default::default() },
            SessionRecord { session_id: "s2".into(), log_path: Some("/w/a.eid".into()), ..Default::default() },
        ];
        assert_eq!(bind_session("/w/a.eid", &twins), Err("ambiguous-session"));
    }

    #[test]
    fn an_ask_frame_adds_it_and_its_settle_removes_it_and_nothing_else() {
        let mut file = AsksFile::default();
        let ask = Frame::Ask(Ask::Question { ask_id: 7, prompt: "p".into(), options: vec![] });
        assert!(apply(&mut file, "s0", 1, "d", &ask, 5));
        assert!(!apply(&mut file, "s0", 1, "d", &ask, 9), "the same ask again changes nothing");
        assert_eq!(file.asks[0].first_seen, 5, "the first sight is kept");
        let other = Frame::Ask(Ask::Question { ask_id: 7, prompt: "q".into(), options: vec![] });
        assert!(apply(&mut file, "s0", 2, "d", &other, 5), "another door's ask 7 is its own ask");
        let settle = Frame::AskSettled { ask_id: 7, how: "answered".into(), answer: None, note: None };
        assert!(apply(&mut file, "s0", 1, "d", &settle, 6));
        assert_eq!(file.asks.len(), 1);
        assert_eq!((file.asks[0].session_id.as_str(), file.asks[0].port), ("s0", 2));
    }

    #[test]
    fn a_hello_clears_only_the_asks_of_its_own_door() {
        let mut file = AsksFile { written: 0, asks: vec![entry("s0", 1, 7), entry("s0", 2, 7), entry("s1", 1, 9)] };
        let hello_frame = Frame::Hello {
            session: LOG.into(),
            cwd: "/w".into(),
            model: "m".into(),
            yolo: false,
            pending: 0,
            protocol: 1,
        };
        assert!(apply(&mut file, "s0", 1, "d", &hello_frame, 5));
        assert_eq!(file.asks, vec![entry("s0", 2, 7), entry("s1", 1, 9)]);
        assert!(!drop_door(&mut file, "s9", 9), "a door with no asks removes none");
    }

    #[test]
    fn an_ask_frame_lands_in_asks_json_with_mode_0600() {
        let rig = rig("lands");
        rig.sessions(&[LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG), ASK_7]);
        rig.puller().tick();
        let file = wait_for(&rig.asks, |f| f.asks.len() == 1);
        let entry = &file.asks[0];
        assert_eq!(entry.session_id, "s0");
        assert_eq!(entry.port, fake.addr().port());
        assert_eq!(entry.ask_id, 7);
        assert_eq!(entry.ask["prompt"], "pick one");
        assert!(file.written > 0, "the write names its second");
        let mode = fs::metadata(&rig.asks).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "asks.json is owner-only");
    }

    #[test]
    fn a_settle_frame_removes_the_ask() {
        let rig = rig("settles");
        rig.sessions(&[LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG), ASK_7]);
        rig.puller().tick();
        wait_for(&rig.asks, |f| f.asks.len() == 1);
        fake.push(SETTLE_7);
        let file = wait_for(&rig.asks, |f| f.asks.is_empty());
        assert!(ask_ids(&file).is_empty());
    }

    #[test]
    fn a_restarted_door_keeps_none_of_its_old_asks() {
        let rig = rig("restart");
        rig.sessions(&[LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG), ASK_7]);
        let puller = rig.puller();
        puller.tick();
        wait_for(&rig.asks, |f| ask_ids(f) == vec![7]);
        fake.end();
        wait_for(&rig.asks, |f| f.asks.is_empty());
        fake.set_head(&[&hello(LOG), ASK_8]);
        for _ in 0..400 {
            puller.tick();
            if fake.requests().len() >= 2 {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let file = wait_for(&rig.asks, |f| ask_ids(f) == vec![8]);
        assert_eq!(ask_ids(&file), vec![8], "the new process's ask 8, not the old ask 7");
    }

    #[test]
    fn a_door_no_record_holds_is_skipped_and_audited_once() {
        let rig = rig("unbound");
        rig.sessions(&["/w/other.eid"]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG)]);
        let puller = rig.puller();
        puller.tick();
        let text = wait_for_audit(&rig.audit, "unbound");
        assert_eq!(text.lines().count(), 1, "one audit line for the skip");
        assert!(!rig.asks.exists(), "no ask is kept for a door no record holds");
        puller.tick();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(fs::read_to_string(&rig.audit).unwrap().lines().count(), 1, "backoff: not dialled again");
        assert_eq!(fake.requests().len(), 1, "the door was dialled once");
    }

    #[test]
    fn a_token_name_that_is_not_web_port_token_is_skipped_without_a_word() {
        let rig = rig("names");
        let eidolon = rig.runtime.join("eidolon");
        fs::create_dir_all(&eidolon).unwrap();
        fs::set_permissions(&eidolon, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["web-99999.token", "web-abc.token", "web-0080.token", "web-80.token.bak"] {
            fs::write(eidolon.join(name), "tok").unwrap();
        }
        rig.puller().tick();
        thread::sleep(Duration::from_millis(300));
        assert!(!rig.audit.exists(), "a name that is no door's is not dialled, so nothing is audited");
        assert!(!rig.asks.exists(), "and no store is written");
    }

    #[test]
    fn a_token_file_failing_the_doors_checks_is_skipped_before_any_byte_and_audited_once() {
        let rig = rig("token");
        rig.sessions(&[LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG)]);
        fs::set_permissions(fake.token_file(), fs::Permissions::from_mode(0o644)).unwrap();
        let puller = rig.puller();
        puller.tick();
        let text = wait_for_audit(&rig.audit, "refused");
        assert_eq!(text.lines().count(), 1, "one audit line for the refusal");
        assert!(fake.requests().is_empty(), "no request reached the door");
    }

    #[test]
    fn a_door_two_records_hold_is_skipped_and_audited_once() {
        let rig = rig("ambiguous");
        rig.sessions(&[LOG, LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG), ASK_7]);
        rig.puller().tick();
        let text = wait_for_audit(&rig.audit, "ambiguous");
        assert_eq!(text.lines().count(), 1, "one audit line for the skip");
        assert!(!rig.asks.exists(), "no record is chosen, so no ask is kept");
    }

    #[test]
    fn a_lagged_frame_closes_the_stream_and_the_door_is_dialled_again() {
        let rig = rig("lagged");
        rig.sessions(&[LOG]);
        let fake = FakeDoor::start(&rig.runtime, "tok");
        fake.set_head(&[&hello(LOG)]);
        let puller = rig.puller();
        puller.tick();
        for _ in 0..400 {
            if fake.streaming() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        thread::sleep(Duration::from_millis(100));
        fake.push(r#"{"type":"lagged","dropped":3}"#);
        for _ in 0..400 {
            puller.tick();
            if fake.requests().iter().filter(|r| r.path == "/api/events").count() >= 2 {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("a lagged stream was not closed and the door was not dialled again");
    }
}

#[cfg(test)]
mod verb_tests {
    use super::*;
    use aoide_door::fake::FakeDoor;
    use aoide_protocol::Door as Caller;
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    const TOKEN: &str = "tok";
    const SESSION: &str = "s0";

    /// The env a verb reads: the stage dir, the runtime dir the fake door writes
    /// its token into, and an audit log of this test's own.
    struct Env {
        asks: PathBuf,
        audit: PathBuf,
        runtime: PathBuf,
    }

    fn env(tag: &str) -> Env {
        let root = crate::graph::testutil::unique_stage(tag);
        let stage = root.join("stage");
        let runtime = root.join("run");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &runtime);
        Env { asks: stage.join("asks.json"), audit: root.join("audit.log"), runtime }
    }

    fn invocation(args: &[&str], flags: &[(&str, &str)], door: Caller, audit: &Path) -> Invocation {
        let mut flags: BTreeMap<String, String> = flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        flags.insert("audit-log".to_string(), audit.display().to_string());
        Invocation {
            path: vec!["ask".to_string(), "answer".to_string()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags,
            door,
        }
    }

    /// The digest the puller stores for a door whose token is `token`.
    fn digest(token: &str) -> String {
        aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(token.as_bytes()))
    }

    /// An open ask of this session, as the puller stores it for a door on `port`
    /// whose token is `token`.
    fn entry_at(port: u16, ask_id: u64, ask: Value, token: &str) -> Entry {
        Entry { session_id: SESSION.into(), port, ask_id, ask, first_seen: 1, token_digest: digest(token) }
    }

    fn open_entries(env: &Env, entries: Vec<Entry>) {
        let file = AsksFile { written: 1, asks: entries };
        std::fs::write(&env.asks, serde_json::to_string(&file).unwrap()).unwrap();
    }

    /// A store with one question open on `port`, the ask as the door's frame gives it.
    fn open(env: &Env, port: u16, ask: Value) {
        open_entries(env, vec![entry_at(port, 7, ask, TOKEN)]);
    }

    fn question(prompt: &str) -> Value {
        json!({"kind": "question", "ask_id": 7, "prompt": prompt, "options": [{"label": "a"}, {"label": "b"}]})
    }

    fn approval(command: &str) -> Value {
        json!({
            "kind": "approval", "ask_id": 7, "call_id": "c1", "tool": "Bash",
            "input": {"command": command}, "prompt": "run it?", "reason": null,
            "structural": false, "judged": null, "yolo": false, "answers": ["yes", "no"]
        })
    }

    /// The `reason` a refusal or an error carries in its data.
    fn reason_of(out: &Outcome) -> String {
        out.data.as_ref().and_then(|d| d["reason"].as_str()).unwrap_or_default().to_string()
    }

    fn list(env: &Env) -> Outcome {
        ask_list(&invocation(&["ask", "list"], &[], Caller::Cli, &env.audit))
    }

    /// The fingerprint `ask list` shows for the open ask at row 0.
    fn listed_fingerprint(env: &Env) -> String {
        list(env).data.as_ref().expect("list data")["asks"][0]["fingerprint"].as_str().unwrap().to_string()
    }

    fn fingerprint(port: u16, ask: &Value) -> String {
        fingerprint_of(SESSION, port, 7, &visible_ask(ask))
    }

    fn open_asks(env: &Env) -> usize {
        load_stage::<AsksFile>(&env.asks).map(|f| f.asks.len()).unwrap_or(0)
    }

    fn audit_lines(env: &Env) -> usize {
        std::fs::read_to_string(&env.audit).map(|t| t.lines().count()).unwrap_or(0)
    }

    #[test]
    fn answer_without_expect_is_refused_before_any_request() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-no-expect");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        open(&env, fake.addr().port(), question("pick"));
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert!(fake.requests().is_empty(), "no request reached the door");
        assert_eq!(audit_lines(&env), 1, "the refusal is audited");
    }

    #[test]
    fn answer_with_an_empty_or_wrong_expect_is_refused_before_any_request() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-wrong-expect");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        open(&env, fake.addr().port(), question("pick"));
        for expect in ["", "0000000000000000"] {
            let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", expect)], Caller::Cli, &env.audit));
            assert_ne!(out.status, aoide_protocol::output::Status::Ok, "expect `{expect}` must refuse");
        }
        assert!(fake.requests().is_empty(), "no request reached the door");
        assert_eq!(audit_lines(&env), 2, "each refusal is audited");
    }

    #[test]
    fn answer_with_an_unoffered_label_is_refused_and_no_request_reaches_the_door() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-label");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        let out = ask_answer(&invocation(&[SESSION, "7", "zz"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok);
        assert!(fake.requests().is_empty(), "an unoffered label sends nothing");
    }

    #[test]
    fn answer_from_the_mcp_and_a2a_doors_is_refused() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-doors");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        for door in [Caller::Mcp, Caller::A2a] {
            let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], door, &env.audit));
            assert_ne!(out.status, aoide_protocol::output::Status::Ok, "{door:?} must be refused");
        }
        assert!(fake.requests().is_empty(), "no request reached the door from a refusing door");
    }

    #[test]
    fn a_good_answer_posts_the_label_and_note_with_the_bearer_and_settles_the_ask() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-good");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        fake.reply("/api/answer", 204, "");
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        let out = ask_answer(&invocation(
            &[SESSION, "7", "a"],
            &[("expect", &fp), ("note", "because")],
            Caller::Cli,
            &env.audit,
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let requests = fake.requests();
        assert_eq!(requests.len(), 1, "one request");
        let req = &requests[0];
        assert_eq!((req.method.as_str(), req.path.as_str()), ("POST", "/api/answer"));
        assert_eq!(req.header("authorization"), Some("Bearer tok"), "the bearer travels in the header");
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body, json!({"ask_id": 7, "answer": "a", "note": "because"}));
        assert_eq!(open_asks(&env), 0, "an answered ask leaves the store");
    }

    #[test]
    fn a_409_drops_the_entry_and_a_422_keeps_it() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-409-422");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        fake.reply("/api/answer", 422, r#"{"error":"not an option"}"#);
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok);
        assert_eq!(open_asks(&env), 1, "a 422 keeps the ask open");
        fake.reply("/api/answer", 409, "");
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok);
        assert_eq!(open_asks(&env), 0, "a 409 drops it");
    }

    #[test]
    fn hostile_ask_text_is_stripped_in_list_and_in_the_fingerprint_input() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-hostile");
        let raw = question("\u{1b}[31mpick\u{202e} one\u{7}\r");
        open(&env, 9, raw.clone());
        let out = ask_list(&invocation(&["ask", "list"], &[], Caller::Cli, &env.audit));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert!(!out.message.contains('\u{1b}') && !out.message.contains('\u{202e}') && !out.message.contains('\u{7}'));
        let shown = &out.data.as_ref().expect("list data")["asks"][0];
        assert!(!shown["ask"]["prompt"].as_str().unwrap().contains(|c: char| c.is_control() || c == '\u{202e}'));
        assert_eq!(
            shown["fingerprint"].as_str().unwrap(),
            fingerprint_of(SESSION, 9, 7, &visible_ask(&raw)),
            "the fingerprint is taken over the stripped ask"
        );
    }

    fn wait_for_lines(lines: &Mutex<Vec<String>>, n: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while lines.lock().unwrap().len() < n {
            assert!(std::time::Instant::now() < deadline, "watch printed too few lines");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_added_and_a_removed_ask_each_give_one_line_and_the_stop_closes_the_stream() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-watch");
        let stop = Arc::new(AtomicBool::new(false));
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let (path, stop_in, lines_in) = (env.asks.clone(), stop.clone(), lines.clone());
        let watcher = std::thread::spawn(move || {
            watch_store(&path, true, Duration::from_millis(5), &stop_in, &mut |line| {
                lines_in.lock().unwrap().push(line)
            });
        });
        open(&env, 9, question("pick one"));
        wait_for_lines(&lines, 1);
        std::fs::write(&env.asks, serde_json::to_string(&AsksFile::default()).unwrap()).unwrap();
        wait_for_lines(&lines, 2);
        stop.store(true, Ordering::SeqCst);
        watcher.join().unwrap();
        let got: Vec<Value> = lines.lock().unwrap().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
        let kinds: Vec<&str> = got.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["added", "removed", "end"]);
        assert_eq!(got[0]["askId"], 7);
        assert_eq!(got[2]["reason"], "interrupted");
    }

    #[test]
    fn ask_watch_takes_the_cli_door_only() {
        let mut r = aoide_protocol::registry::Registry::new();
        crate::commands::graph::register(&mut r);
        let watch = r.get(&["ask".to_string(), "watch".to_string()]).expect("ask watch is registered");
        assert!(watch.opens(Caller::Cli));
        assert!(!watch.opens(Caller::Daemon) && !watch.opens(Caller::Mcp) && !watch.opens(Caller::A2a));
    }

    /// The handler refuses a daemon call on its own, and returns: a live loop never
    /// runs inside aoided. The call runs on a thread so a loop fails the test, not hangs it.
    #[test]
    fn ask_watch_refuses_a_daemon_call_and_returns() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-watch-daemon");
        let audit = env.audit.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let out = ask_watch(&invocation(&["ask", "watch"], &[], Caller::Daemon, &audit));
            let _ = tx.send(out.status);
        });
        let status = rx.recv_timeout(Duration::from_secs(5)).expect("ask watch returned for the daemon door");
        assert_ne!(status, aoide_protocol::output::Status::Ok);
    }

    /// The approval an approver reads is the bytes that run: a newline is shown
    /// escaped, never removed.
    #[test]
    fn a_newline_in_an_approval_is_shown_escaped_never_removed() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-escaped");
        open(&env, 9, approval("ls #\nrm -rf ~"));
        let out = list(&env);
        assert!(out.message.contains(r"ls #\nrm -rf ~"), "{}", out.message);
        let shown = &out.data.as_ref().expect("list data")["asks"][0]["ask"]["input"]["command"];
        assert_eq!(shown, r"ls #\nrm -rf ~");
    }

    #[test]
    fn asks_differing_only_by_a_hidden_character_get_different_fingerprints() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-hidden-fp");
        open(&env, 9, approval("rm -rf x"));
        let plain = listed_fingerprint(&env);
        open(&env, 9, approval("rm\u{200b} -rf x"));
        let hidden = listed_fingerprint(&env);
        assert_ne!(plain, hidden, "the hidden character is part of what is approved");
    }

    #[test]
    fn a_key_differing_only_by_a_hidden_character_is_shown_beside_its_twin() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-keys");
        let ask = json!({
            "kind": "approval", "ask_id": 7, "call_id": "c1", "tool": "Bash",
            "input": {"cmd": "one", "cmd\u{200b}": "two"}, "prompt": "p", "reason": null,
            "structural": false, "judged": null, "yolo": false, "answers": ["yes", "no"]
        });
        open(&env, 9, ask);
        let out = list(&env);
        let input = &out.data.as_ref().expect("list data")["asks"][0]["ask"]["input"];
        assert_eq!(input.as_object().map(|m| m.len()), Some(2), "{input}");
    }

    #[test]
    fn the_text_list_names_an_approvals_tool_input_and_reason() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-text-approval");
        let mut ask = approval("ls");
        ask["reason"] = json!("because x");
        open(&env, 9, ask);
        let out = list(&env);
        assert!(out.message.contains("Bash"), "{}", out.message);
        assert!(out.message.contains(r#"{"command":"ls"}"#), "{}", out.message);
        assert!(out.message.contains("because x"), "{}", out.message);
    }

    #[test]
    fn the_text_list_names_a_questions_offered_labels() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-text-question");
        open(&env, 9, question("pick one"));
        let out = list(&env);
        assert!(out.message.contains("offers: a, b"), "{}", out.message);
    }

    /// A door restarted on the same port holds a new token; the stored ask is the
    /// old process's. The answer is refused before any post reaches the new door.
    #[test]
    fn a_restarted_door_on_the_same_port_is_refused_before_any_post() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-restart");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        fake.reply("/api/answer", 204, "");
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        fake.set_token_file("restarted-token");
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_eq!(reason_of(&out), "door-changed", "{}", out.message);
        assert!(fake.requests().iter().all(|r| r.method != "POST"), "a post reached the restarted door");
        assert_eq!(open_asks(&env), 1, "nothing was answered, so the entry stays");
    }

    /// Another session's door holds the port now: its token is not the one the
    /// stored ask was read from, so it is refused the same way.
    #[test]
    fn another_sessions_door_on_the_same_port_is_refused_before_any_post() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-other-session");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        fake.reply("/api/answer", 204, "");
        let ask = question("pick");
        open_entries(&env, vec![entry_at(fake.addr().port(), 7, ask.clone(), "session-a-token")]);
        let fp = fingerprint(fake.addr().port(), &ask);
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_eq!(reason_of(&out), "door-changed", "{}", out.message);
        assert!(fake.requests().iter().all(|r| r.method != "POST"), "a post reached the other session's door");
    }

    /// Two doors hold ask 7 of one session: the answer names the entry its
    /// fingerprint matches, and posts to that door only.
    #[test]
    fn answer_picks_the_entry_its_fingerprint_names_among_two_doors() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-two-doors");
        let first = FakeDoor::start(&env.runtime, "tok-a");
        let second = FakeDoor::start(&env.runtime, "tok-b");
        second.reply("/api/answer", 204, "");
        let (ask_a, ask_b) = (question("pick a"), question("pick b"));
        open_entries(
            &env,
            vec![
                entry_at(first.addr().port(), 7, ask_a, "tok-a"),
                entry_at(second.addr().port(), 7, ask_b.clone(), "tok-b"),
            ],
        );
        let fp = fingerprint(second.addr().port(), &ask_b);
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert_eq!(second.requests().iter().filter(|r| r.method == "POST").count(), 1, "the named door got the answer");
        assert!(first.requests().is_empty(), "the other door got nothing");
    }

    #[test]
    fn a_store_that_does_not_read_is_refused_and_audited() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-unreadable");
        std::fs::write(&env.asks, "not json").unwrap();
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", "0000000000000000")], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok);
        assert_eq!(audit_lines(&env), 1, "the store-read failure is audited");
    }

    #[test]
    fn an_ask_id_that_is_not_a_number_is_audited() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-id-usage");
        let out = ask_answer(&invocation(&[SESSION, "seven", "a"], &[("expect", "0000000000000000")], Caller::Cli, &env.audit));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok);
        assert_eq!(audit_lines(&env), 1, "the usage error is audited");
    }

    /// The answered line names the fingerprint the approver read, and carries the
    /// ask it answered as untrusted data, as pending.rs's lines do.
    #[test]
    fn an_answer_is_audited_with_its_fingerprint_and_the_ask_it_answered() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = env("ask-audit-answer");
        let fake = FakeDoor::start(&env.runtime, TOKEN);
        fake.reply("/api/answer", 204, "");
        let ask = question("pick");
        open(&env, fake.addr().port(), ask.clone());
        let fp = fingerprint(fake.addr().port(), &ask);
        let out = ask_answer(&invocation(&[SESSION, "7", "a"], &[("expect", &fp)], Caller::Cli, &env.audit));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let text = std::fs::read_to_string(&env.audit).unwrap();
        let line: Value = serde_json::from_str(text.lines().last().expect("an audit line")).unwrap();
        assert_eq!(line["status"], "answered");
        assert!(line["message"].as_str().unwrap_or_default().contains(&fp), "{line}");
        assert!(line["untrusted_data"].as_str().unwrap_or_default().contains("pick"), "{line}");
    }
}
