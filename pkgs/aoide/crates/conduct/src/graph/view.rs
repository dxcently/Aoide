//! `session watch <id>` — the READ-ONLY view of a managed task wrapper run
//! (`docs/Aoide-Wiki/concepts/orchestration/Managed-Task-Wrapper.md`):
//! the instructions the run was started with, its live output, the mail filed
//! to its task mailbox, and where it stands right now.
//!
//! **Live by default; `--snapshot` is the single bounded mode.** There is no
//! `--follow` flag: a person watching a run expects it to keep talking, so
//! following is the default and `--snapshot` exists for exactly one reason —
//! a door that must return (MCP/stdin) cannot hold a loop, so it asks for one
//! frame. Both modes call the same [`gather`], so a snapshot can never drift
//! from what the live view renders.
//!
//! **Read-only by construction.** The view opens files and prints them: no
//! `write_stage`, no stage lock held across a render, no `.ring.lock`, no
//! `mark`, no `ring`, no socket write — the same posture `session trace`
//! documents. Mail is listed from
//! [`aoide_storage::mail::read_base`] (every entry, no cursor), NEVER
//! `read_for`/`mark`, so watching a run consumes neither the child's nor the
//! parent's unread state, and a person may watch twice without losing mail.
//!
//! **Every untrusted fragment is rendered as data, never as control.** A
//! letter's subject, body, sender and a task's instructions are all sanitized
//! (control characters stripped, lines clipped), because a `\r` in a letter
//! body is an Enter at whoever pastes it and an escape sequence is a
//! terminal's to obey. Raw PTY bytes are printed only when stdout really is a
//! terminal — that is what the live terminal view means — and `--snapshot`,
//! `--json` and every non-TTY stdout get the sanitized form
//! (`trace.rs::stdout_is_terminal`'s own gate). Nothing read here is ever
//! interpreted as a command, and no letter is ever relayed into the run's
//! PTY: only an explicit `send` types into a session.
//!
//! **Exit is not success.** The footer reports `exited <code> at <endedAt>`,
//! `running`, or `stopped — not running`, and nothing else: no subject is
//! parsed for a verdict, and a missing `exitCode` is reported as missing, not
//! as `0`. Whether a run's work was any good is the operator's own reading of
//! the output and the mail.

use super::common::{self, require_args, stage_error};
use super::doc::is_conductable_now;
use super::model::{
    canonical_state, load_stage, resolved_parent, sessions_path, SessionRecord, SessionsFile,
};
use aoide_protocol::output::Outcome;
use aoide_protocol::Door;
use aoide_protocol::Invocation;
use aoide_storage::addr::{self, LocalCandidate, Resolution};
use aoide_storage::mail::Entry;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often the live view looks for new output/mail/state.
const WATCH_POLL: Duration = Duration::from_millis(500);
/// Default `--tail` window, in output lines.
const DEFAULT_TAIL: usize = 50;
/// How many letters of the task mailbox the rail shows (the newest ones).
const MAIL_RAIL: usize = 20;
/// How many lines of a sanitized block (instructions, a letter body) are
/// rendered — a bound on the render buffer, never on what is stored. The
/// per-line clip is [`common::LINE_MAX`], shared with every other graph
/// surface that prints text this process did not write.
const BLOCK_LINES_MAX: usize = 400;
/// The bounded window read from the tail of a run's PTY transcript: a runaway
/// log can never pull an unbounded amount of bytes into a render buffer.
const LOG_TAIL_BYTES: u64 = 64 * 1024;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_signum: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// One letter as the rail shows it: the untrusted fields already sanitized,
/// plus which RUN of this task sent it. Attribution only — never a verdict.
///
/// `pub` with [`Frame`]: an A2A caller reads the very same frame off
/// `tasks/get` (CONTRACTS.md §6), so this is the one shape on the wire as
/// well as on the terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MailLine {
    pub seq: u64,
    #[serde(rename = "receivedAt")]
    pub received_at: String,
    #[serde(rename = "from")]
    pub from: String,
    pub subject: String,
    /// `this run`, `earlier run (<petname|session id>)` or
    /// `not this run (<sender>)` — the mailbox name is shared across runs by
    /// design, so each letter says which run it belongs to.
    pub run: String,
    pub body: Vec<String>,
}

/// `skip_serializing_if` helper for a plain (non-`Option`) `bool` field whose
/// common case is `false` — the same one-liner `aoide_storage::records` uses,
/// for the same reason: the common case stays off the wire.
fn is_false(b: &bool) -> bool {
    !*b
}

/// One rendered frame of a run: everything the view shows, gathered without
/// writing anything. Both modes render THIS — and so does `tasks/get`: an
/// authorized A2A caller gets this same frame, minus the fields
/// [`Frame::for_wire`] strikes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub label: String,
    pub agent: String,
    pub task: Option<String>,
    /// The parent this run's completion report is addressed to
    /// (`parentSessionId`) — the same id the spawner enrolled as a reader of
    /// the task mailbox. `None` on a run with no parent at all, which the
    /// report line then says plainly instead of promising a wake.
    pub parent: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    pub state: String,
    /// `running` | `exited` | `signalled` | `timed out` | `stopped` — the
    /// process fact, never a verdict. The last four are the record's own
    /// discriminated `outcome`, so a `--timeout` kill and a signal death are
    /// never flattened into an ordinary stop.
    pub presence: String,
    /// The footer's own line: `exited <code> at <endedAt>`,
    /// `timed out — the run's own --timeout deadline fired (ended <endedAt>)`,
    /// `died by signal at <endedAt>`, `stopped — not running`, or `running`.
    pub status: String,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
    #[serde(rename = "endedAt")]
    pub ended_at: Option<String>,
    /// The run's DISCRIMINATED end, verbatim from the record
    /// (`exit`/`signal`/`timeout`/`stopped`, the closed set `conduct` stamps).
    /// `null` on an ordinary session and on a record written before the field
    /// existed — and a `signal`/`timeout`/`stopped` end never carries an
    /// `exitCode` beside it.
    pub outcome: Option<String>,
    pub socket: Option<String>,
    pub conductable: bool,
    /// The run's PTY transcript path — a path on the box that wrote it, so
    /// [`Frame::for_wire`] strikes it before the frame leaves this host.
    /// `None` only for a frame that has already been through `for_wire`.
    #[serde(rename = "logPath", default)]
    pub log_path: Option<String>,
    #[serde(rename = "instructionsPath")]
    pub instructions_path: Option<String>,
    pub instructions: Option<Vec<String>>,
    pub output: Vec<String>,
    pub mail: Vec<MailLine>,
    /// Whether this run's exit report is in the mailbox yet, from the report
    /// lane's own cursor — never inferred from some letter the child sent.
    pub report: String,
    /// What the wake did once the report was filed (`rang:0
    /// skipped:<id>(not-conductable)` — a Codex-desktop/never-conducted parent
    /// cannot be woken at all), absent until then.
    pub wake: Option<String>,
    /// The operator's own next step, PRINTED and never executed.
    pub suggested: Option<String>,
    /// Set by the A2A door alone, and only when a wire cap dropped something
    /// from this frame (`a2a.rs::frame_artifact`): the local view always
    /// shows every line of its own window, so a local frame never sets it —
    /// hence `skip_serializing_if`, which keeps `--json` byte-identical.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

impl Frame {
    /// The frame as another BOX may see it: the four fields that name or
    /// command something on THIS host are struck —
    /// * `logPath`/`instructionsPath`, paths in this box's filesystem
    ///   (`instructions` itself stays: the sidecar's sanitized text is what
    ///   the frame is FOR);
    /// * `socket`, the live control socket's own path;
    /// * `suggested`, an `aoide send --id …` line whose `id` is this box's
    ///   namespace — the reader rebuilds its own
    ///   (`aoide send --to <node>/<id> --submit -- …`).
    ///
    /// Nothing else changes: the same [`render`] draws both.
    pub fn for_wire(mut self) -> Frame {
        self.log_path = None;
        self.socket = None;
        self.instructions_path = None;
        self.suggested = None;
        self
    }
}

/// `session watch <id> [--tail N] [--snapshot] [--json]` — see the module doc.
pub fn session_watch(inv: &Invocation) -> Outcome {
    let cmd = "session.watch";
    let args = match require_args(inv, &["id"]) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let target = args[0].trim().to_string();
    // A `sub:`-prefixed argument is a sub-agent card by SHAPE, and it is
    // refused as one before resolution: such a card shares its executor's
    // process rather than owning one, so the honest answer does not depend on
    // whether it is still on the roster — and an unknown `sub:` id must not
    // read as an ordinary typo. Same boundary `conduct/AGENTS.md` draws for
    // kill, and the same refusal `send` makes.
    if target.starts_with("sub:") {
        return Outcome::error(
            cmd,
            "that is a sub-agent card — it shares its executor's process and keeps no PTY of its \
             own; watch its executor session instead",
        )
        .with_data(json!({ "reason": "subagent", "sessionId": target }));
    }
    let tail = match parse_tail(inv) {
        Ok(n) => n,
        Err(o) => return o,
    };
    let snapshot = inv.flag_present("snapshot");
    let json_mode = inv.flag_present("json");

    // Live is the default — and live BLOCKS, so a door that has to return is
    // asked for the one bounded mode instead of being silently narrowed.
    if !snapshot && inv.door != Door::Cli {
        return Outcome::usage(
            cmd,
            "live viewing follows until the run ends (Ctrl-C stops it) — over this door ask for one frame with --snapshot",
        );
    }

    // `<id>` resolves exactly like `send --to`/`session trace`: the ONE
    // `aoide_storage::addr::resolve`, over this box's live roster plus its
    // registered node names, with the same ambiguity/not-found refusals.
    let file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    let host = aoide_storage::display::local_host_name();
    let ids: HashSet<&str> = file.sessions.iter().map(|s| s.session_id.as_str()).collect();
    let candidates: Vec<LocalCandidate<'_>> = file
        .sessions
        .iter()
        .map(|s| {
            let role = if resolved_parent(s, &ids).is_some() { "child" } else { "root" };
            LocalCandidate { session_id: &s.session_id, petname: s.petname.as_deref(), role }
        })
        .collect();
    let nodes = aoide_storage::node_store::load_nodes();
    let node_names: Vec<&str> = nodes.iter().map(|p| p.name.as_str()).collect();
    let id = match addr::resolve(&target, &host, &candidates, &node_names) {
        Resolution::Local(id) => id,
        Resolution::Remote { node, .. } => {
            return Outcome::error(
                cmd,
                format!(
                    "`{target}` resolves to a session on node `{node}` — its output and mail are \
                     files on the node that wrote them, so run `session watch` there"
                ),
            )
            .with_data(json!({ "reason": "remote-target", "node": node, "target": target }))
        }
        Resolution::Ambiguous(found) => {
            return Outcome::error(
                cmd,
                format!(
                    "`{target}` is ambiguous — {} local session(s) match: {}",
                    found.len(),
                    found.join(", ")
                ),
            )
            .with_data(json!({ "reason": "ambiguous", "target": target, "candidates": found }))
        }
        Resolution::NotFound => {
            let hint = if !target.contains('/') && node_names.contains(&target.as_str()) {
                format!(" (`{target}` names a known node, not a local session)")
            } else {
                String::new()
            };
            return Outcome::error(cmd, format!("no session matches `{target}`{hint}"))
                .with_data(json!({ "reason": "not-found", "target": target }));
        }
    };

    let frame = match gather(&id, tail, false, &file.sessions, &host, &ids) {
        Ok(f) => f,
        Err(o) => return o,
    };

    if snapshot {
        let mut message = format!("{} · {}", frame.label, frame.status);
        if !json_mode {
            message.push('\n');
            message.push_str(&render(&frame).join("\n"));
        }
        return Outcome::ok(cmd, message)
            .with_data(serde_json::to_value(&frame).unwrap_or_default());
    }

    // Live: the frame above is the opening frame (sanitized — the live
    // STREAM below is what prints raw bytes, and only onto a real terminal).
    let opening = match gather(&id, tail, !json_mode && stdout_is_terminal(), &file.sessions, &host, &ids) {
        Ok(f) => f,
        Err(_) => frame.clone(),
    };
    follow(cmd, &id, &opening, tail, json_mode, &host)
}

/// `--tail <n>`: a non-numeric or zero value is a usage error, never a
/// silently empty view — the same rule `session trace` holds.
fn parse_tail(inv: &Invocation) -> Result<usize, Outcome> {
    match inv.flags.get("tail") {
        None => Ok(DEFAULT_TAIL),
        Some(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if n > 0 => Ok(n),
            _ => Err(Outcome::usage(
                "session.watch",
                format!("--tail must be a positive whole number of lines (got `{raw}`)"),
            )
            .with_data(json!({ "reason": "bad-tail", "tail": raw }))),
        },
    }
}

/// Gather one frame — the whole read side of this command, shared by the
/// snapshot and by every live re-read so the two can never drift. Refusals
/// happen here, before a single line is printed: an unknown id, a `sub:`
/// card, and a record that keeps no conduct-owned PTY are all taught errors,
/// never an empty view.
fn gather(
    id: &str,
    tail: usize,
    raw: bool,
    roster: &[SessionRecord],
    host: &str,
    ids: &HashSet<&str>,
) -> Result<Frame, Outcome> {
    let cmd = "session.watch";
    let Some(rec) = roster.iter().find(|r| r.session_id == id) else {
        return Err(Outcome::error(cmd, format!("no session matches `{id}`"))
            .with_data(json!({ "reason": "not-found", "sessionId": id })));
    };
    if rec.session_id.starts_with("sub:") {
        return Err(Outcome::error(
            cmd,
            "that is a sub-agent card — it shares its executor's process and keeps no PTY of its own; \
             watch its executor session instead",
        )
        .with_data(json!({ "reason": "subagent", "sessionId": id })));
    }
    let Some(log_path) = rec.log_path.clone().filter(|p| !p.is_empty()) else {
        return Err(Outcome::error(
            cmd,
            format!(
                "`{id}` keeps no conduct-owned PTY — a run started by `conduct`/`spawn` is the one \
                 with output to watch (a hook-only agent session has none)"
            ),
        )
        .with_data(json!({ "reason": "no-log", "sessionId": id })));
    };

    let role = if resolved_parent(rec, ids).is_some() { "child" } else { "root" };
    let label = aoide_storage::display::session_label(rec, host, role);
    let done = canonical_state(&rec.state) == "done";
    let conductable = is_conductable_now(rec);
    let presence = if done {
        // The record's own DISCRIMINATED end decides, when it carries one: a
        // deadline kill and a signal death are not an ordinary stop, and
        // neither is an exit. A record written before `outcome` existed keeps
        // exactly the reading it always had.
        match rec.outcome.as_deref() {
            Some("exit") => "exited",
            Some("signal") => "signalled",
            Some("timeout") => "timed out",
            Some("stopped") => "stopped",
            _ => {
                if rec.exit_code.is_some() {
                    "exited"
                } else {
                    "stopped"
                }
            }
        }
    } else if conductable || rec.pid.is_some_and(aoide_storage::fs::pid_is_alive) {
        "running"
    } else {
        "stopped"
    };
    let ended_phrase = rec
        .ended_at
        .clone()
        .unwrap_or_else(|| "an unknown instant".to_string());
    let status = match presence {
        "exited" | "signalled" => format!(
            "{} at {}",
            end_words(rec.outcome.as_deref(), rec.exit_code),
            ended_phrase
        ),
        "timed out" => format!(
            "{} — the run's own --timeout deadline fired (ended {})",
            end_words(rec.outcome.as_deref(), rec.exit_code),
            ended_phrase
        ),
        "stopped" => match &rec.ended_at {
            Some(at) => format!("stopped — not running (ended {at})"),
            None => "stopped — not running".to_string(),
        },
        _ => "running".to_string(),
    };

    // Instructions: the operator's own bytes, read from the write-once
    // sidecar the record names. Sanitized — a sidecar is text to SHOW, never
    // a command to obey.
    let instructions = rec.instructions_path.as_deref().and_then(|p| {
        std::fs::read_to_string(p).ok().map(|text| clean_block(&text))
    });

    // Output: the tail of the PTY transcript, from a bounded window. `raw`
    // is the caller's terminal gate — a snapshot, a `--json` frame and any
    // non-TTY stdout always get the sanitized form.
    let (output, _) = read_log_tail(Path::new(&log_path), tail, raw);

    // Mail for this run's task mailbox, cursor-free, newest last. The report
    // line comes from the LANE'S OWN CURSOR, never from a letter the child
    // happened to send.
    let filed = super::taskreport::filed_report(id);
    let (mail, report) = match rec.task.as_deref().filter(|t| !t.is_empty()) {
        Some(slug) => {
            // A3: the rail is the CHILD's own UNREAD queue — reader-selected and
            // cursor-free (`unread_for` is a PEEK), so an observer sees exactly
            // what the subagent has not read and consumes nothing: not its own
            // mark, not the child's.
            let (_, entries) =
                aoide_storage::mail::unread_for(slug, Some(&rec.session_id)).unwrap_or_default();
            let lines = mail_lines(&entries, slug, rec, roster);
            (lines, report_line(filed.as_ref(), done))
        }
        None => (
            Vec::new(),
            "this run carries no task mailbox (`spawn --task`) — its record, output and the \
             session ledger are the whole record of it"
                .to_string(),
        ),
    };

    let suggested = if presence == "running" && conductable {
        Some(format!(
            "aoide send --id {id} --submit -- \"<text>\"   (prints nothing else; the gate is yours)"
        ))
    } else {
        None
    };

    Ok(Frame {
        session_id: id.to_string(),
        label,
        agent: rec.agent.clone(),
        task: rec.task.clone(),
        parent: rec.parent_session_id.clone(),
        started_at: rec.started_at.clone(),
        state: rec.state.clone(),
        presence: presence.to_string(),
        status,
        exit_code: rec.exit_code,
        ended_at: rec.ended_at.clone(),
        outcome: rec.outcome.clone(),
        socket: rec.socket.clone(),
        conductable,
        log_path: Some(log_path),
        instructions_path: rec.instructions_path.clone(),
        instructions,
        output,
        mail,
        report,
        wake: filed.and_then(|f| f.wake),
        suggested,
        truncated: false,
    })
}

/// The same frame `session watch --snapshot` prints, with RAW always off — the
/// public READ the A2A door serves (`tasks/get` with `metadata["aoide/frame"]`,
/// CONTRACTS.md §6). The door has no terminal of its own, so the gate
/// [`read_log_tail`] takes is never open on this path: every line is the
/// sanitized form, and the bytes do not depend on the caller's stdout.
///
/// It is the EXISTING [`gather`] with `raw = false`, never a second gather —
/// a frame served over the wire is the frame the local view renders, one
/// definition, and the refusals (an unknown id, a `sub:` card, a record that
/// keeps no conduct-owned PTY) are that function's own taught errors.
pub fn watch_frame(id: &str, tail: usize) -> Result<Frame, Outcome> {
    let file: SessionsFile = load_stage(&sessions_path())
        .map_err(|e| stage_error("session.watch", e))?;
    let host = aoide_storage::display::local_host_name();
    let ids: HashSet<&str> = file.sessions.iter().map(|s| s.session_id.as_str()).collect();
    gather(id, tail, false, &file.sessions, &host, &ids)
}

/// The task mailbox's letters, sanitized, each labelled with the run that
/// sent it. The mailbox NAME is the task slug and outlives a respawn, so the
/// run is identified by the sender's own session id — a letter whose sender
/// is not THIS run's child is shown (never hidden, never merged into this
/// run's report) and labelled as an earlier run.
fn mail_lines(entries: &[Entry], _slug: &str, rec: &SessionRecord, roster: &[SessionRecord]) -> Vec<MailLine> {
    // `entries` arrives ALREADY filtered to the child's own unread queue
    // (`unread_for(slug, Some(child))` — a peek, no cursor moved), so this only
    // trims to the rail's width. A letter the child has read is gone from here;
    // it is not hidden, it is read.
    let mut matched: Vec<&Entry> = entries.iter().collect();
    if matched.len() > MAIL_RAIL {
        matched.drain(..matched.len() - MAIL_RAIL);
    }
    matched
        .into_iter()
        .map(|e| mail_line(e, &rec.session_id, roster))
        .collect()
}

/// ONE letter's own decoded, sanitized shape — the rule both renderers share:
/// the frame's rail and the live delta both call this, so a letter can never
/// read one way in a snapshot and another way in the stream. The body is
/// already [`clean_block`]ed (control characters stripped, lines clipped, the
/// line count bounded), which is what makes the live delta bounded too.
fn mail_line(e: &Entry, this_run: &str, roster: &[SessionRecord]) -> MailLine {
    let from = common::clean_line(&e.envelope.header.from.name);
    let (subject, body) = match aoide_storage::letter::decode(&e.envelope.text) {
        Some(content) => (common::clean_line(&content.subject), clean_block(&content.body)),
        None => (String::new(), clean_block(&e.envelope.text)),
    };
    MailLine {
        seq: e.seq,
        received_at: common::clean_line(&e.received_at),
        run: run_label(&e.envelope.header.from.name, this_run, roster),
        from,
        subject,
        body,
    }
}

/// One letter as the lines it renders: the caller's own header line, then the
/// sanitized body indented beneath it — the SHARED rule, so the live delta
/// prints exactly the letter the rail holds, body included.
fn letter_lines(header: String, line: &MailLine) -> Vec<String> {
    let mut out = vec![header];
    out.extend(line.body.iter().map(|body| format!("    {body}")));
    out
}

/// The child's own unread queue: its mark and how many letters are pending —
/// a PEEK on every call (no cursor move), which is what makes the drain marker
/// honest about WHO read.
fn child_queue(id: &str, slug: Option<&str>) -> (u64, usize) {
    match slug {
        Some(slug) => aoide_storage::mail::unread_for(slug, Some(id))
            .map(|(mark, entries)| (mark, entries.len()))
            .unwrap_or((0, 0)),
        None => (0, 0),
    }
}

/// `hh:mm:ss` of the current UTC instant — the clock the drain marker prints.
fn hhmmss_utc() -> String {
    let iso = aoide_storage::time::now_iso_utc();
    iso.get(11..19).unwrap_or(&iso).to_string()
}

/// Which RUN of the task a letter belongs to, as the rail labels it. The
/// mailbox NAME is shared across runs by design; the run is the sender's
/// session id. A finished run normally leaves the roster once its report is
/// filed, so the display name falls back to the durable ledger — the petname
/// `doc.rs::ledger_session_exit` projected there — before the raw id. A sender
/// that is neither a live nor a ledgered session is not a run at all (a plain
/// mailbox name), and is labelled as such rather than folded in with earlier
/// runs.
fn run_label(from: &str, this_run: &str, roster: &[SessionRecord]) -> String {
    if from == this_run {
        return "this run".to_string();
    }
    // Every id/label this returns is printed, so the sender is sanitized HERE
    // too: a live line once rendered a raw `sender\u{1b}[0m` because the
    // display form and the lookup key were the same string.
    let shown = common::clean_line(from);
    let roster_name = roster
        .iter()
        .find(|s| s.session_id == from)
        .and_then(|s| s.petname.clone());
    let ledger_name = || {
        aoide_storage::ledger::read_ledger().ok().and_then(|entries| {
            entries
                .into_iter()
                .rev()
                .find(|e| e.session_id == from)
                .and_then(|e| e.petname)
        })
    };
    match roster_name.or_else(ledger_name) {
        Some(name) => format!("earlier run ({})", common::clean_line(&name)),
        None if roster.iter().any(|s| s.session_id == from) => format!("earlier run ({shown})"),
        None => format!("not this run ({shown})"),
    }
}

/// A run's DISCRIMINATED end in words, from the record's own `outcome` (the
/// closed set `conduct` stamps: `exit`/`signal`/`timeout`/`stopped`) beside the
/// real `exitCode`. This is the ONE wording a watched run's footer, its report
/// line and the letter the report lane files all share, so a `--timeout` kill
/// can never read as an ordinary stop and a run that died by a signal never
/// claims a code. Never a fabricated `0`: an absent code is reported as absent,
/// and `signal`/`timeout`/`stopped` claim no code at all. A record that
/// predates `outcome` falls back to its `exitCode` exactly as it always did.
fn end_words(outcome: Option<&str>, exit_code: Option<i32>) -> String {
    match outcome {
        Some("exit") => match exit_code {
            Some(code) => format!("exited {code}"),
            None => "exited (no code recorded)".to_string(),
        },
        Some("signal") => "died by signal".to_string(),
        Some("timeout") => "timed out".to_string(),
        Some("stopped") => "stopped (no status)".to_string(),
        Some(other) => common::clean_line(other),
        None => match exit_code {
            Some(code) => format!("exited {code}"),
            None => "stopped (no exit status recorded)".to_string(),
        },
    }
}

/// Whether THIS run's exit report has been filed — read off the report lane's
/// own cursor (`state/stage/taskreport.json`, read-only, no lock, no cursor
/// advance), never inferred from the presence of some letter from the child: a
/// run that mails its own mailbox during its life must not read as reported.
/// The mail rail is corroboration, never the evidence. The line carries the
/// cursor's own discriminated end, so a deadline kill is visible here too.
fn report_line(filed: Option<&super::taskreport::FiledReport>, done: bool) -> String {
    if let Some(f) = filed {
        let msgid = f
            .msgid
            .as_deref()
            .map(|m| m.chars().take(12).collect::<String>())
            .unwrap_or_else(|| "(no msgid read back)".to_string());
        let mut parts = vec![format!(
            "filed — msgid {msgid}, run ended {}, {}",
            f.ended_at.clone().unwrap_or_else(|| "unknown".to_string()),
            end_words(f.outcome.as_deref(), f.exit_code)
        )];
        if let Some(wake) = &f.wake {
            parts.push(format!("wake {wake}"));
        }
        parts.join(", ")
    } else if done {
        "not filed yet — the daemon tick files it, and the run's record is retained until it \
         does (a daemon that was down reports on its next pass)"
            .to_string()
    } else {
        "none yet — the report is filed when the run ends".to_string()
    }
}

/// The whole rendered frame, one line per element — the human door's body.
/// The first block is never raw PTY bytes: `--snapshot`, `--json` and a
/// redirected stdout all get this sanitized form.
fn render(frame: &Frame) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.push(format!(
        "{} · {} · {} · started {}",
        frame.label, frame.agent, frame.status, frame.started_at
    ));
    out.push(match &frame.log_path {
        Some(path) => format!("session {} · log {path}", frame.session_id),
        None => format!("session {}", frame.session_id),
    });
    match (&frame.task, &frame.socket, frame.conductable) {
        (Some(task), _, _) => out.push(format!("task {task} · mailbox self/{task}")),
        (None, _, _) => {}
    }
    match (&frame.socket, frame.conductable) {
        (Some(socket), conductable) => out.push(format!(
            "socket {socket} · conductable {}",
            if conductable { "yes" } else { "no" }
        )),
        (None, _) => out.push("conductable no (no live control socket)".to_string()),
    }
    match (&frame.instructions, &frame.instructions_path) {
        (Some(lines), Some(path)) => {
            out.push(String::new());
            out.push(format!("── instructions ({path}) ──"));
            out.extend(lines.iter().cloned());
        }
        (None, Some(path)) => {
            out.push(String::new());
            out.push(format!(
                "── instructions ({path}) — NOT READABLE (missing, or a permission edge case) ──"
            ));
        }
        _ => {}
    }
    out.push(String::new());
    out.push(format!("── output (last {} lines) ──", frame.output.len()));
    if frame.output.is_empty() {
        out.push("(nothing yet)".to_string());
    } else {
        out.extend(frame.output.iter().cloned());
    }
    if !frame.mail.is_empty() || frame.task.is_some() {
        out.push(String::new());
        out.push(format!(
            "── {} unread for {} (the child's own queue; the watcher consumes nothing) ──",
            frame.mail.len(),
            frame.task.clone().unwrap_or_default()
        ));
        if frame.mail.is_empty() {
            out.push("(nothing unread)".to_string());
        }
        for line in &frame.mail {
            out.extend(letter_lines(
                format!(
                    "#{} {} {} {} [{}]",
                    line.seq,
                    line.received_at,
                    line.from,
                    if line.subject.is_empty() { "(no subject)" } else { &line.subject },
                    line.run
                ),
                line,
            ));
        }
    }
    out.push(String::new());
    out.push(format!("report: {}", frame.report));
    if let Some(suggested) = &frame.suggested {
        out.push(format!("suggested: {suggested}"));
    }
    out
}

/// The live rail's cursor-free delta reader: every letter for `slug` that has
/// not been rendered yet, in `seq` order.
///
/// `follower` is `None` while the mailbase does not exist — no letter has ever
/// been filed on this box — because [`aoide_protocol::feed::Follower`]'s own
/// constructor opens the file, and a missing base would otherwise leave the
/// live view deaf for the rest of the watch. On the first tick that finds one,
/// the follower attaches at that file's END and the base is ALSO read directly
/// ([`aoide_storage::mail::read_base`], a PEEK: no cursor moves, for this
/// viewer or for the child). The attach goes FIRST and the direct read second,
/// deliberately: the attach's offset is then the earlier write frontier, so a
/// letter that lands between the two is caught by the read rather than skipped
/// by the attach — attaching at the end without that catch-up is exactly how
/// the FIRST letter of a run went unrendered.
///
/// `rendered_seq` is the highest `seq` already on stdout (the opening frame's
/// own rail seeds it). It is what makes the two readers exactly-once between
/// them: a letter the direct read caught is filtered out when the follower's
/// own poll returns the same line. It advances past the newest returned entry.
fn live_letters(
    follower: &mut Option<aoide_protocol::feed::Follower>,
    rendered_seq: &mut u64,
    slug: &str,
) -> Vec<Entry> {
    let mut fresh: Vec<Entry> = Vec::new();
    if follower.is_none() {
        *follower = aoide_protocol::feed::Follower::open_at_end(&aoide_storage::mail::base_path()).ok();
        fresh = aoide_storage::mail::read_base().unwrap_or_default();
    } else if let Some(f) = follower.as_mut() {
        if let Ok(lines) = f.poll() {
            fresh.extend(lines.into_iter().filter_map(|l| serde_json::from_str::<Entry>(&l).ok()));
        }
    }
    let mut out: Vec<Entry> = fresh
        .into_iter()
        .filter(|e| e.envelope.header.to.name == slug && e.seq > *rendered_seq)
        .collect();
    out.sort_by_key(|e| e.seq);
    if let Some(newest) = out.last() {
        *rendered_seq = newest.seq;
    }
    out
}

/// The live view: print the frame just gathered, then re-read output, mail
/// and the record every [`WATCH_POLL`] until the run ends or Ctrl-C. Ends on
/// the RUN's own end (never on a guess) and keeps every completed-run artifact
/// — the transcript, the sidecar, the letters — in place for inspection.
fn follow(
    cmd: &'static str,
    id: &str,
    first: &Frame,
    tail: usize,
    json_mode: bool,
    host: &str,
) -> Outcome {
    use std::io::Write as _;
    unsafe {
        libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
    }
    let mut printed_lines = 0usize;
    let emit_frame = |frame: &Frame, printed: &mut usize| {
        if json_mode {
            if let Ok(value) = serde_json::to_string(frame) {
                println!("{value}");
            }
        } else {
            for line in render(frame) {
                println!("{line}");
                *printed += 1;
            }
        }
        let _ = std::io::stdout().flush();
    };
    emit_frame(first, &mut printed_lines);

    // Raw PTY bytes are printed only onto a real terminal; anything else
    // (a pipe, a file, a captured door) gets the sanitized form. The output
    // tail is the live stream, so its offset is taken NOW: history was
    // already rendered above.
    let raw_output = stdout_is_terminal() && !json_mode;
    // The transcript to follow: the opening frame's own `logPath`, which
    // `gather` above already guaranteed (a record that keeps no conduct-owned
    // PTY is refused there). A frame whose `logPath` was struck —
    // `Frame::for_wire`, run by another box — leaves this empty, and an empty
    // path reads as no growth: the same degradation this loop already
    // tolerates for a transcript that went away mid-watch.
    let path = PathBuf::from(first.log_path.clone().unwrap_or_default());
    let mut offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let mut child_mark = child_queue(id, first.task.as_deref()).0;
    // Catch up after attaching, including letters filed since the opening
    // frame; the child's cursor and printed rail exclude old letters.
    let mut follower = None;
    let mut rendered_seq = first.mail.iter().map(|l| l.seq).max().unwrap_or(0).max(child_mark);
    let mut new_mail = 0usize;

    while !INTERRUPTED.load(Ordering::SeqCst) {
        std::thread::sleep(WATCH_POLL);
        if INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }
        // Output: whatever grew since the last look, from the same bounded
        // reader; a truncated/replaced transcript restarts from its new head.
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < offset {
            offset = 0;
        }
        if len > offset {
            if let Some(chunk) = read_log_from(&path, offset) {
                offset += chunk.len() as u64;
                let text = String::from_utf8_lossy(&chunk);
                for line in text.split('\n') {
                    if line.is_empty() {
                        continue;
                    }
                    let line = if raw_output { line.to_string() } else { common::clean_line(line) };
                    println!("{line}");
                    printed_lines += 1;
                }
                let _ = std::io::stdout().flush();
            }
        }
        // Mail: every letter the rail has not rendered yet, sanitized and on
        // its own line, with its sanitized BODY beneath that line — through
        // the same [`mail_line`]/[`letter_lines`] pair the frame's rail uses,
        // so a letter read live says exactly what the same letter shows in a
        // snapshot. `live_letters` is read-only: no cursor moves here, for
        // this viewer or for the child.
        if let Some(slug) = first.task.as_deref() {
            for entry in live_letters(&mut follower, &mut rendered_seq, slug) {
                // The SAME decoding, sanitizing and three-way attribution the
                // frame uses — one rule, two renderers (a live line must never
                // disagree with the rail it is appended to).
                let roster_now: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
                    .map(|f| f.sessions)
                    .unwrap_or_default();
                let letter = mail_line(&entry, id, &roster_now);
                for out in letter_lines(
                    format!(
                        "mail #{} {} {} [{}]",
                        letter.seq,
                        letter.from,
                        if letter.subject.is_empty() {
                            "(no subject)"
                        } else {
                            &letter.subject
                        },
                        letter.run
                    ),
                    &letter,
                ) {
                    println!("{out}");
                    printed_lines += 1;
                }
                new_mail += 1;
            }
        }
        // A3: when the CHILD's own mark advances, the queue above drained
        // because the CHILD read — never because this observer did. One marker
        // per advance, so a person watching sees the queue shrink for the right
        // reason.
        if let Some(slug) = first.task.as_deref() {
            let (mark, pending) = child_queue(id, Some(slug));
            if mark > child_mark {
                println!(
                    "child read through seq {mark} at {} ({pending} still unread)",
                    hhmmss_utc()
                );
                printed_lines += 1;
                child_mark = mark;
            }
        }
        // The run's own state decides when this ends: `done` (its real exit
        // path) or a record that left the roster (pruned/reaped). A mere
        // silence never ends the view.
        let roster = load_stage::<SessionsFile>(&sessions_path())
            .map(|f| f.sessions)
            .unwrap_or_default();
        let ids: HashSet<&str> = roster.iter().map(|s| s.session_id.as_str()).collect();
        match roster.iter().find(|r| r.session_id == id) {
            Some(rec) if canonical_state(&rec.state) != "done" => continue,
            _ => {}
        }
        let last = gather(id, tail, true, &roster, host, &ids).ok();
        if let Some(frame) = &last {
            emit_frame(frame, &mut printed_lines);
        } else {
            println!("stopped — not running (the record has left the roster)");
            println!("the transcript, the instructions sidecar and the mail above remain on disk");
            let _ = std::io::stdout().flush();
        }
        return Outcome::ok(cmd, format!("watched {id} to its end"))
            .with_data(json!({
                "sessionId": id,
                "followed": true,
                "printed": printed_lines,
                "newMail": new_mail,
            }));
    }

    // Ctrl-C: report where things stood, gathered once more, and change
    // nothing — the run keeps running without this viewer.
    let roster = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let ids: HashSet<&str> = roster.iter().map(|s| s.session_id.as_str()).collect();
    if let Ok(frame) = gather(id, tail, true, &roster, host, &ids) {
        println!("{}", frame.status);
        println!("report: {}", frame.report);
    }
    let _ = std::io::stdout().flush();
    Outcome::ok(cmd, format!("stopped watching {id}"))
        .with_data(json!({ "sessionId": id, "followed": true, "printed": printed_lines }))
}

/// The last `lines` lines of the byte stream at `path`, read from a bounded
/// trailing window. `raw` is the terminal gate: a real terminal gets the run's
/// own bytes (ANSI included — that is what a live terminal view is), every
/// other sink gets one sanitized line per line.
fn read_log_tail(path: &Path, lines: usize, raw: bool) -> (Vec<String>, u64) {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let Some(chunk) = read_log_from(path, len.saturating_sub(LOG_TAIL_BYTES)) else {
        return (Vec::new(), len);
    };
    let text = String::from_utf8_lossy(&chunk);
    let all: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    let window: Vec<String> = all
        .iter()
        .rev()
        .take(lines)
        .rev()
        .map(|l| if raw { (*l).to_string() } else { common::clean_line(l) })
        .collect();
    (window, len)
}

/// Read the trailing bytes of `path` from `offset` on — one bounded syscall
/// window, never the whole file.
fn read_log_from(path: &Path, offset: u64) -> Option<Vec<u8>> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    let mut bounded = (&file).take(LOG_TAIL_BYTES);
    bounded.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// A block of untrusted/operator text kept multi-line: `\n` is kept, every
/// other character [`common::is_unsafe`] refuses (a `\r`, an escape sequence,
/// a bell, a zero-width or bidi `Cf` mark — none of them reach a terminal),
/// each line is clipped, and the block is bounded to [`BLOCK_LINES_MAX`]
/// lines.
fn clean_block(s: &str) -> Vec<String> {
    s.chars()
        .filter(|c| *c == '\n' || !common::is_unsafe(*c))
        .collect::<String>()
        .split('\n')
        .take(BLOCK_LINES_MAX)
        .map(|l| common::clip_flat(l.trim_end(), common::LINE_MAX))
        .collect()
}

/// Is stdout a terminal? Raw PTY bytes are printed only here; every other
/// sink gets the sanitized form (the same gate `trace.rs` uses for dimming).
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::model::{write_stage, HooksFile};
    use crate::graph::testutil::{unique_stage, EnvVars};
    use aoide_storage::mail;
    use std::collections::BTreeMap;

    /// A fresh isolated state/stage/runtime triple, held by the caller's
    /// `env_lock` — every test here reads and writes only its own tree.
    fn isolated(tag: &str) -> (PathBuf, EnvVars) {
        let guard = EnvVars::save(&["AOIDE_STATE_DIR", "AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);
        let root = unique_stage(tag);
        for sub in ["stage", "state", "state/mail", "state/sessions", "run"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
        (root, guard)
    }

    /// One finished managed task run on the roster, with a real transcript and
    /// a real instruction sidecar on disk.
    fn finished_run(root: &Path, id: &str, slug: &str, exit_code: Option<i32>) -> SessionRecord {
        let log = root.join("state/sessions").join(format!("{id}.log"));
        std::fs::write(&log, "line one\nline two\n").unwrap();
        let sidecar = root.join("state/sessions").join(format!("{id}.instructions.md"));
        std::fs::write(&sidecar, "BRIEF: do the thing\nsecond line\n").unwrap();
        let mut rec = crate::graph::testutil::session(id, &root.to_string_lossy(), "done", "2026-09-21T05:00:00Z", None);
        rec.task = Some(slug.to_string());
        rec.log_path = Some(log.to_string_lossy().into_owned());
        rec.instructions_path = Some(sidecar.to_string_lossy().into_owned());
        rec.exit_code = exit_code;
        rec.ended_at = Some("2026-09-21T05:01:00Z".to_string());
        rec
    }

    fn write_roster(recs: Vec<SessionRecord>) {
        write_stage(
            &sessions_path(),
            &SessionsFile { sessions: recs, ..Default::default() },
        )
        .unwrap();
        write_stage(
            &crate::graph::model::hooks_path(),
            &HooksFile::default(),
        )
        .unwrap();
    }

    fn snapshot_inv(id: &str) -> Invocation {
        Invocation {
            path: vec!["session".to_string(), "watch".to_string()],
            args: vec![id.to_string()],
            flags: BTreeMap::from([("snapshot".to_string(), "true".to_string())]),
            door: Door::Cli,
        }
    }

    /// `for_wire` is the ONE place a frame sheds what belongs to this box: the
    /// two paths, the live socket, and the `aoide send` line whose id is a
    /// local namespace. Everything the frame exists to SHOW stays — and the
    /// struck fields are `null` on the wire, never absent, so a reader can
    /// tell "this box kept its paths" from "there is no path".
    #[test]
    fn for_wire_strikes_the_four_fields_that_name_something_on_this_box() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-for-wire");
        let sock = root.join("run/run-1.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        let mut rec = crate::graph::testutil::session(
            "run-1",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T05:00:00Z",
            None,
        );
        let log = root.join("state/sessions/run-1.log");
        std::fs::write(&log, "line one\nline two\n").unwrap();
        let sidecar = root.join("state/sessions/run-1.instructions.md");
        std::fs::write(&sidecar, "BRIEF: do the thing\n").unwrap();
        rec.task = Some("fix-flaky".to_string());
        rec.log_path = Some(log.to_string_lossy().into_owned());
        rec.instructions_path = Some(sidecar.to_string_lossy().into_owned());
        rec.socket = Some(sock.to_string_lossy().into_owned());
        rec.conductable = Some(true);
        write_roster(vec![rec]);

        let frame = watch_frame("run-1", 5).unwrap();
        // The local frame carries all four — otherwise the assertions below
        // would pass on a frame that never had them.
        assert!(frame.log_path.is_some(), "local frame: {frame:?}");
        assert!(frame.socket.is_some(), "local frame: {frame:?}");
        assert_eq!(
            frame.suggested.as_deref(),
            Some("aoide send --id run-1 --submit -- \"<text>\"   (prints nothing else; the gate is yours)"),
        );

        let wire = frame.clone().for_wire();
        assert_eq!(wire.log_path, None);
        assert_eq!(wire.socket, None);
        assert_eq!(wire.instructions_path, None);
        assert_eq!(wire.suggested, None);
        // Everything the frame is FOR survives.
        assert_eq!(wire.output, frame.output);
        assert_eq!(wire.mail, frame.mail);
        assert_eq!(wire.label, frame.label);
        assert_eq!(wire.status, frame.status);
        assert_eq!(wire.instructions, frame.instructions);
        let v = serde_json::to_value(&wire).unwrap();
        assert!(v["logPath"].is_null() && v["socket"].is_null(), "serialized: {v}");
        assert!(v["instructionsPath"].is_null() && v["suggested"].is_null(), "serialized: {v}");
        // The struck frame is still a frame: the same renderer draws it.
        assert!(render(&wire).iter().any(|l| l.contains("run-1")), "rendered: {:?}", render(&wire));
    }

    /// The frame a wire reader gets round-trips back through `Deserialize`
    /// into the same value — the client-side half of `for_wire` (the same
    /// `render` on both sides, one shape).
    #[test]
    fn a_wire_frame_deserializes_back_into_the_frame_that_was_sent() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-for-wire-round-trip");
        let rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        write_roster(vec![rec]);

        let wire = watch_frame("run-1", 5).unwrap().for_wire();
        let json = serde_json::to_value(&wire).unwrap();
        let back: Frame = serde_json::from_value(json).unwrap();
        assert_eq!(back, wire);
    }

    /// A `--json` frame is the LOCAL view's own bytes: `truncated` — a wire
    /// cap's flag, set by the door alone — is absent until something sets it.
    #[test]
    fn a_local_frame_serializes_without_the_wire_only_flag() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-truncated-absent");
        let rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        write_roster(vec![rec]);

        let frame = watch_frame("run-1", 5).unwrap();
        assert!(!frame.truncated);
        let v = serde_json::to_value(&frame).unwrap();
        assert!(v.get("truncated").is_none(), "serialized: {v}");
        let mut cut = frame;
        cut.truncated = true;
        assert_eq!(serde_json::to_value(&cut).unwrap()["truncated"], true);
    }

    /// `clean_block` keeps `\n` and drops every other character the one
    /// sanitizer refuses — a zero-width or bidi `Cf` mark is an invisible
    /// instruction inside an instructions sidecar or a letter body too.
    #[test]
    fn clean_block_strips_format_characters_and_keeps_its_newlines() {
        assert_eq!(
            clean_block("one\u{200b}\ntwo\u{202e}three\u{feff}\n"),
            vec!["one".to_string(), "twothree".to_string(), String::new()],
        );
        assert_eq!(clean_block("a\rb"), vec!["ab".to_string()]);
    }

    /// The live rail attaches to the mailbase on the first tick that finds one,
    /// even when NO letter has ever been filed when the watch began — and it
    /// renders the FIRST letter, because a base that did not exist at watch
    /// start holds only letters that arrived after it. Reading nothing while
    /// the base was absent (the old shape: one failed `open_at_end`, `None`
    /// forever) is what left the first letter to the child alone; attaching at
    /// the end with no catch-up read would have skipped it instead. Neither
    /// reader moves a cursor.
    #[test]
    fn the_live_rail_attaches_to_an_absent_base_and_renders_the_first_letter() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-live-attach");
        // A record, but NO mailbase at all: `mail::base_path()` does not exist.
        let rec = finished_run(&root, "run-1", "fix-flaky", None);
        write_roster(vec![rec.clone()]);
        let roster = vec![rec.clone()];
        assert!(
            !mail::base_path().exists(),
            "the fixture must start with no mailbase at all"
        );
        let cursors_before = std::fs::read(mail::cursors_path()).unwrap_or_default();

        let mut follower = None;
        let mut rendered_seq = 0u64;
        assert!(
            live_letters(&mut follower, &mut rendered_seq, "fix-flaky").is_empty(),
            "no base, no letters — and no panic"
        );
        assert!(follower.is_none(), "there is nothing to attach to yet");

        // The FIRST letter of the run is filed while the watch is already up.
        let first = mail::file_letter("outside", "fix-flaky", "FIRST-LETTER-20260921\nbody line").unwrap().seq;
        let rendered: Vec<u64> = live_letters(&mut follower, &mut rendered_seq, "fix-flaky")
            .iter()
            .map(|e| e.seq)
            .collect();
        assert_eq!(rendered, vec![first], "the first letter is rendered, never skipped");
        assert!(follower.is_some(), "and the rail is attached now");

        // Later letters arrive on the incremental path, exactly once each.
        let second = mail::file_letter("outside", "fix-flaky", "SECOND-LETTER").unwrap().seq;
        assert_eq!(
            live_letters(&mut follower, &mut rendered_seq, "fix-flaky")
                .iter()
                .map(|e| e.seq)
                .collect::<Vec<_>>(),
            vec![second]
        );
        assert!(
            live_letters(&mut follower, &mut rendered_seq, "fix-flaky").is_empty(),
            "a letter is rendered once, never on every poll"
        );

        // The live renderer prints that first letter's body, and nothing here
        // consumed any mail: both cursor files are byte-identical.
        let entry = mail::read_base()
            .unwrap()
            .into_iter()
            .find(|e| e.seq == first)
            .expect("the first letter is in the base");
        let letter = mail_line(&entry, &rec.session_id, &roster);
        let lines = letter_lines(format!("mail #{}", letter.seq), &letter);
        assert!(lines.iter().any(|l| l == "    FIRST-LETTER-20260921"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "    body line"), "{lines:?}");
        assert_eq!(
            std::fs::read(mail::cursors_path()).unwrap_or_default(),
            cursors_before,
            "the live rail advances no cursor"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn live_attachment_catches_letters_after_the_frame_without_replaying_history() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-live-startup");
        let consumed = mail::file_letter("outside", "fix-flaky", "already read").unwrap().seq;
        let shown = mail::file_letter("outside", "fix-flaky", "opening rail").unwrap().seq;
        let mut rendered_seq = shown.max(consumed);
        let arriving = mail::file_letter("outside", "fix-flaky", "between frame and attach").unwrap().seq;
        let mut follower = None;
        assert_eq!(
            live_letters(&mut follower, &mut rendered_seq, "fix-flaky")
                .iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![arriving]
        );
        assert!(live_letters(&mut follower, &mut rendered_seq, "fix-flaky").is_empty());
        // An empty opening rail still starts after the child's read cursor.
        let mut follower = None;
        let mut rendered_seq = arriving;
        assert!(live_letters(&mut follower, &mut rendered_seq, "fix-flaky").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A `--timeout` kill is not an ordinary stop. The record carries
    /// `outcome:"timeout"` and NO exit code; the frame must say exactly that —
    /// the discriminated `outcome`, a `timed out` presence and footer, and a
    /// `null` `exitCode` it never fabricates a `0` for — and the report line
    /// must say it too, from the cursor's own copy of the same fact.
    #[test]
    fn a_timeout_run_reads_as_a_timeout_and_never_claims_an_exit_code() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-timeout");
        let mut rec = finished_run(&root, "run-timeout", "wrapper-timeout-probe", None);
        rec.outcome = Some("timeout".to_string());
        rec.ended_at = Some("2026-09-21T11:48:18Z".to_string());
        write_roster(vec![rec]);

        let out = session_watch(&snapshot_inv("run-timeout"));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let data = out.data.clone().unwrap();
        assert_eq!(data["outcome"], json!("timeout"));
        assert_eq!(data["presence"], json!("timed out"));
        assert_eq!(
            data["exitCode"],
            json!(null),
            "a deadline kill claims no code, and never a fabricated 0"
        );
        assert!(
            out.message.contains("timed out — the run's own --timeout deadline fired"),
            "{}",
            out.message
        );
        assert!(
            !out.message.contains("stopped — not running"),
            "a timeout must never read as an ordinary stop: {}",
            out.message
        );

        // The report line reads the lane's cursor, which carries the same
        // discriminated fact.
        let filed = super::super::taskreport::FiledReport {
            ended_at: Some("2026-09-21T11:48:18Z".to_string()),
            exit_code: None,
            outcome: Some("timeout".to_string()),
            mailbox: Some("aoide-ops".to_string()),
            msgid: Some("c697dbf54e0352f6".to_string()),
            wake: None,
        };
        let line = report_line(Some(&filed), true);
        assert!(line.contains("timed out"), "{line}");
        assert!(!line.contains("stopped"), "{line}");
        // And an ordinary exit still reads exactly as it always did.
        assert_eq!(end_words(Some("exit"), Some(7)), "exited 7");
        assert_eq!(end_words(Some("signal"), None), "died by signal");
        assert_eq!(end_words(Some("timeout"), None), "timed out");
        assert_eq!(
            end_words(None, None),
            "stopped (no exit status recorded)",
            "a record predating `outcome` keeps the old wording, never a code"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The live delta and the snapshot rail are ONE rule: a letter that
    /// arrives while watching is decoded, sanitized and rendered through the
    /// same [`mail_line`]/[`letter_lines`] pair the rail uses, so its BODY is
    /// printed live exactly as the rail prints it — and the observer consumes
    /// nothing (no cursor is touched by rendering it).
    #[test]
    fn the_live_delta_renders_the_letter_body_the_rail_holds() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-live-body");
        let rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        let roster = vec![rec.clone()];
        write_roster(roster.clone());
        mail::file_letter(
            "outside",
            "fix-flaky",
            "BODY-LIVE-20260921\u{1b}[31m\r\nsecond live line",
        )
        .unwrap();
        let cursors_before = std::fs::read(mail::cursors_path()).unwrap_or_default();

        let entry = mail::read_base().unwrap().into_iter().next().expect("one letter");
        let letter = mail_line(&entry, &rec.session_id, &roster);
        let live = letter_lines(
            format!(
                "mail #{} {} {} [{}]",
                letter.seq,
                letter.from,
                if letter.subject.is_empty() { "(no subject)" } else { &letter.subject },
                letter.run
            ),
            &letter,
        );
        assert!(
            live.len() > 1,
            "the delta carries the body, not only the header: {live:?}"
        );
        assert!(
            live.iter().any(|l| l == "    BODY-LIVE-20260921[31m"),
            "the sanitized body prints under the header: {live:?}"
        );
        assert!(
            live.iter().any(|l| l == "    second live line"),
            "every line of the body, the same as the rail: {live:?}"
        );
        for line in &live {
            assert!(
                !line.chars().any(char::is_control),
                "no control character may reach the terminal: {line:?}"
            );
        }
        // The rail prints the very same body lines for the same letter.
        let snapshot = session_watch(&snapshot_inv("run-1"));
        assert!(snapshot.message.contains("    BODY-LIVE-20260921[31m"), "{}", snapshot.message);
        assert!(snapshot.message.contains("    second live line"), "{}", snapshot.message);
        assert_eq!(
            std::fs::read(mail::cursors_path()).unwrap_or_default(),
            cursors_before,
            "rendering a live letter moves no cursor"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The four files the view reads, byte-for-byte, plus whether a ring lock
    /// ever appeared — the read-only/cursor-free contract (`view.rs`'s module
    /// doc): watching must change NOTHING and must never consume mail.
    #[test]
    fn the_view_writes_nothing_and_advances_no_cursor() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-read-only");
        let rec = finished_run(&root, "run-1", "fix-flaky", Some(7));
        write_roster(vec![rec]);
        mail::file_letter("outside", "fix-flaky", "body of the letter").unwrap();
        mail::enrol_reader("fix-flaky", "reader-a").unwrap();
        // reader-a has NOT read: the mailbox is unread for it.
        assert!(mail::names_with_unread(Some("reader-a")).unwrap().contains(&"fix-flaky".to_string()));

        let watched = [
            sessions_path(),
            crate::graph::model::hooks_path(),
            mail::base_path(),
            mail::cursors_path(),
        ]
        .map(|p| std::fs::read(&p).unwrap_or_default());
        let mail_dir_before: Vec<_> = std::fs::read_dir(mail::mail_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();

        let out = session_watch(&snapshot_inv("run-1"));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert!(out.message.contains("BRIEF: do the thing"), "instructions shown: {}", out.message);
        assert!(out.message.contains("body of the letter"), "mail shown: {}", out.message);
        assert!(out.message.contains("exited 7 at 2026-09-21T05:01:00Z"), "{}", out.message);

        for (path, before) in [
            sessions_path(),
            crate::graph::model::hooks_path(),
            mail::base_path(),
            mail::cursors_path(),
        ]
        .into_iter()
        .zip(watched)
        {
            assert_eq!(
                std::fs::read(&path).unwrap_or_default(),
                before,
                "the view must leave {} byte-identical",
                path.display()
            );
        }
        // No lock of its own: the ring lock belongs to `ring`, which this
        // read-only view never takes. (`.stage.lock` is the shared lock every
        // reader of the stage already takes — `read_base`/`load_stage` — and is
        // not a file this command invents.)
        assert!(!mail::mail_dir().join(".ring.lock").exists(), "no ring lock");
        let mail_dir_after: Vec<_> = std::fs::read_dir(mail::mail_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(mail_dir_before, mail_dir_after, "the mail dir gains no file");
        // Cursor-free: the enrolled reader's unread state is untouched, and
        // so is the pseudo-reader's.
        assert!(mail::names_with_unread(Some("reader-a")).unwrap().contains(&"fix-flaky".to_string()));
        assert!(mail::names_with_unread(None).unwrap().contains(&"fix-flaky".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// House rule 4: forwarded text is untrusted DATA. A letter carrying an
    /// escape sequence, a `\r`, a bell and a 300-character subject renders with
    /// no control character anywhere and one clipped line — and never as a
    /// command (nothing is executed, nothing is written to a session).
    #[test]
    fn untrusted_letter_text_is_sanitized_into_data() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-sanitize");
        let rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        write_roster(vec![rec]);
        let hostile = format!(
            "\u{1b}[31mred\u{7}\r\nsecond\rline\n{}",
            "x".repeat(300)
        );
        mail::file_letter("sender\u{1b}[0m", "fix-flaky", &hostile).unwrap();

        let out = session_watch(&snapshot_inv("run-1"));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        // A rendered BLOCK is multi-line by design, so the rule is per LINE:
        // no line may carry a control character (`\n` is the block's own
        // separator, never a fragment of forwarded text).
        for line in out.message.lines() {
            assert!(
                !line.chars().any(char::is_control),
                "no control character may reach the terminal: {line:?}"
            );
        }
        for byte in ['\u{1b}', '\u{7}', '\r'] {
            assert!(!out.message.contains(byte), "no {byte:?} anywhere in the frame");
        }
        let data = out.data.clone().unwrap();
        let mail = data["mail"].as_array().expect("mail rail");
        assert_eq!(mail.len(), 1);
        // A plain letter carries no structured subject; its hostile 300-char
        // LINE is what must be clipped, one line at a time.
        assert_eq!(mail[0]["subject"].as_str().unwrap(), "", "plain letter, no subject");
        let body = mail[0]["body"].as_array().unwrap();
        let long = body.last().unwrap().as_str().unwrap();
        assert!(long.ends_with('…'), "clipped with an ellipsis: {long}");
        assert_eq!(long.chars().count(), common::LINE_MAX);
        for line in body {
            let line = line.as_str().unwrap();
            assert!(!line.chars().any(char::is_control), "body line: {line:?}");
        }
        let instructions = data["instructions"].as_array().unwrap();
        assert!(instructions.iter().all(|l| !l.as_str().unwrap().chars().any(char::is_control)));
        // Attribution, never a verdict: the sender is shown, no success word.
        assert!(!out.message.to_lowercase().contains("succe"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Both modes call the same `gather`, so a snapshot can never drift from
    /// what the live view would render at the same point.
    #[test]
    fn a_snapshot_frame_equals_a_live_frame_gathered_at_the_same_point() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-snapshot-eq");
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", Some(2))]);
        let file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: HashSet<&str> = file.sessions.iter().map(|s| s.session_id.as_str()).collect();
        let snapshot = gather("run-1", 5, false, &file.sessions, "host", &ids).unwrap();
        let live = gather("run-1", 5, false, &file.sessions, "host", &ids).unwrap();
        assert_eq!(snapshot, live);
        assert_eq!(snapshot.presence, "exited");
        assert_eq!(snapshot.exit_code, Some(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A killed/reaped run reports NO exit code — and never a fabricated 0.
    #[test]
    fn a_stopped_run_reports_stopped_and_an_absent_exit_code() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-stopped");
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", None)]);
        let out = session_watch(&snapshot_inv("run-1"));
        let data = out.data.clone().unwrap();
        assert_eq!(data["exitCode"], serde_json::Value::Null, "no invented 0");
        assert_eq!(data["presence"], "stopped");
        assert!(out.message.contains("stopped — not running"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Refusals are taught, never an empty view: an unknown id, a `sub:` card
    /// (by SHAPE, before resolution), and a record that keeps no PTY.
    #[test]
    fn refusals_are_taught_errors() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-refusals");
        let mut hook_only = crate::graph::testutil::session("hooked", "/x", "idle", "2026-09-21T05:00:00Z", None);
        hook_only.log_path = None;
        write_roster(vec![hook_only]);

        for (target, reason) in [("sub:t1", "subagent"), ("nobody", "not-found"), ("hooked", "no-log")] {
            let out = session_watch(&snapshot_inv(target));
            assert_ne!(out.status, aoide_protocol::output::Status::Ok, "{target}: {}", out.message);
            assert_eq!(
                out.data.as_ref().and_then(|d| d.get("reason")).and_then(|r| r.as_str()),
                Some(reason),
                "{target}: {}",
                out.message
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The live mode is refused over a door that must return, and told what to
    /// ask for — never silently narrowed.
    #[test]
    fn live_mode_over_a_non_cli_door_asks_for_a_snapshot() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("view-door");
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", Some(0))]);
        let mut inv = snapshot_inv("run-1");
        inv.flags.clear();
        inv.door = Door::Daemon;
        let out = session_watch(&inv);
        assert_eq!(out.status, aoide_protocol::output::Status::Usage, "{}", out.message);
        assert!(out.message.contains("--snapshot"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }
}
