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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aoide_door::{check_runtime, runtime_from_env, token_path, Ask, Door, DoorError, Frame};
use aoide_protocol::{append_audit, default_audit_log, AuditRecord, EventClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{load_stage, sessions_path, SessionRecord, SessionsFile};

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
/// `session`, byte for byte.
pub fn bind_session(hello: &str, sessions: &[SessionRecord]) -> Option<String> {
    sessions.iter().find(|r| r.log_path.as_deref() == Some(hello)).map(|r| r.session_id.clone())
}

/// Applies one frame of a door's stream to the store and says whether it changed
/// the store. Only `hello`, `ask` and `ask-settled` change anything.
pub fn apply(file: &mut AsksFile, session_id: &str, port: u16, frame: &Frame, now: u64) -> bool {
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
            file.asks.push(Entry { session_id: session_id.to_string(), port, ask_id, ask: value, first_seen: now });
            true
        }
        Frame::AskSettled { ask_id, .. } => {
            file.asks.retain(|e| !(of_door(e) && e.ask_id == *ask_id));
            file.asks.len() != before
        }
        _ => false,
    }
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
        let mut events = door.events().map_err(|e| reason(&e))?;
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
            if let Frame::Hello { session: hello, .. } = &frame {
                let sessions = load_stage::<SessionsFile>(&self.sessions).map(|f| f.sessions).unwrap_or_default();
                let Some(id) = bind_session(hello, &sessions) else {
                    if let Ok(closer) = events.closer() {
                        let _ = closer.close();
                    }
                    return Err("unbound");
                };
                session = Some(id);
            }
            if let Some(id) = &session {
                self.update(|file| apply(file, id, port, &frame, now()));
            }
        }
        if let Some(id) = session {
            self.update(|file| drop_door(file, &id, port));
        }
        failure.map_or(Ok(()), Err)
    }

    /// Moves the store under the stage lock. A store that does not read is left
    /// as it is, and that is audited once.
    fn update(&self, change: impl FnOnce(&mut AsksFile) -> bool) {
        let Some(dir) = self.asks.parent() else { return };
        aoide_storage::fs::with_lock_in(dir, || {
            let mut file: AsksFile = match load_stage(&self.asks) {
                Ok(file) => file,
                Err(_) => return self.audit(None, "unreadable-store", "error"),
            };
            if !change(&mut file) {
                return;
            }
            file.written = now();
            let Ok(body) = serde_json::to_string_pretty(&file) else { return };
            let _ = aoide_storage::fs::atomic_write_private(&self.asks, body.as_bytes());
        });
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

/// aoided's puller: lists the doors every [`TICK`] until the process ends. With
/// no runtime directory there is no door to find, and it returns.
pub fn run() {
    let Ok(runtime) = runtime_from_env(std::env::var_os("XDG_RUNTIME_DIR").as_deref()) else { return };
    let puller = Puller::new(runtime, asks_path(), sessions_path(), default_audit_log());
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
        Entry { session_id: session_id.into(), port, ask_id, ask: Value::Null, first_seen: 1 }
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
        assert_eq!(bind_session("/w/a.eid", &sessions), Some("s0".to_string()));
        assert_eq!(bind_session("/w/A.eid", &sessions), Some("s1".to_string()));
        assert_eq!(bind_session("/w/a.eid/", &sessions), None, "no trailing-slash match");
        assert_eq!(bind_session("a.eid", &sessions), None, "no stem match");
    }

    #[test]
    fn an_ask_frame_adds_it_and_its_settle_removes_it_and_nothing_else() {
        let mut file = AsksFile::default();
        let ask = Frame::Ask(Ask::Question { ask_id: 7, prompt: "p".into(), options: vec![] });
        assert!(apply(&mut file, "s0", 1, &ask, 5));
        assert!(!apply(&mut file, "s0", 1, &ask, 9), "the same ask again changes nothing");
        assert_eq!(file.asks[0].first_seen, 5, "the first sight is kept");
        let other = Frame::Ask(Ask::Question { ask_id: 7, prompt: "q".into(), options: vec![] });
        assert!(apply(&mut file, "s0", 2, &other, 5), "another door's ask 7 is its own ask");
        let settle = Frame::AskSettled { ask_id: 7, how: "answered".into(), answer: None, note: None };
        assert!(apply(&mut file, "s0", 1, &settle, 6));
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
        assert!(apply(&mut file, "s0", 1, &hello_frame, 5));
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
}
