//! The single-policy-surface contract: the audit log record shape and its
//! append primitive, and which door an operation came through.
//!
//! Both the CLI door and the MCP door route through here; neither writes a
//! separate log (entities/aoided, concepts/Governance).

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::untrusted::escape_for_log;

/// Default audit-log path (mirrors `aoide.auditLog`, BUILD.md option table).
///
/// Resolution order: `$AOIDE_AUDIT_LOG` (set by the systemd unit) → derived
/// as `$AOIDE_ROOT/log` (L-C2, task #107; `$AOIDE_ROOT` default
/// `<home>/.aoide` — see `aoide_storage::fs::root`'s own doc for the full
/// path-model note). Duplicated rather than shared with that function
/// deliberately: this crate sits BELOW `aoide-storage` in the dependency
/// graph, so it cannot call it without a cycle; [`aoide_home`] is the one
/// piece already common to both.
pub fn default_audit_log() -> PathBuf {
    if let Ok(explicit) = std::env::var("AOIDE_AUDIT_LOG") {
        if !explicit.is_empty() {
            return PathBuf::from(explicit);
        }
    }
    if let Ok(dir) = std::env::var("AOIDE_ROOT") {
        if !dir.is_empty() {
            let p = PathBuf::from(&dir);
            if p.is_absolute() {
                return p.join("log");
            }
        }
    }
    aoide_home().join(".aoide").join("log")
}

/// The audit-log path in effect for one invocation (flag override →
/// `aoide.auditLog` default). Moved from the root package's `dispatch.rs`
/// (Phase 9 restructure, docs/architecture/PACKAGE-LAYOUT.md) — both inputs
/// (`Invocation.flags`, `default_audit_log`) are protocol types, so the
/// policy lives here and every crate's handlers consult it directly.
pub fn audit_log_path(inv: &crate::invocation::Invocation) -> PathBuf {
    if let Some(p) = inv.flags.get("audit-log") {
        return PathBuf::from(p);
    }
    default_audit_log()
}

/// The Aoide user's home — what [`default_audit_log`] and `aoide-storage`'s
/// root resolve against when neither is overridden.
///
/// **Unix**: `$AOIDE_USER` → that user's passwd home (`getpwnam_r`;
/// `/home/<user>` when the passwd database has no entry for the name), else
/// `$HOME` when it is set (an empty one is returned as written, and the storage
/// root's guard refuses the relative path it makes), else the passwd home of the
/// effective uid, else the temp dir. The last arm is the native Windows arm's
/// last resort: a process with neither input has no per-user directory to
/// name, and a literal path would be a guess about some other account's home.
///
/// **Native Windows**: `$USERPROFILE`, else `$HOMEDRIVE$HOMEPATH`, else the
/// temp dir — the same last-resort shape `aoide_storage::runtime_dir` takes.
/// Neither of the Unix chain's inputs is this host's answer: Windows never sets
/// `$HOME` (a shell that invents one is not a host fact) and `$AOIDE_USER` is a
/// Linux deployment variable, so honoring either would answer a path OFF THE
/// CURRENT DRIVE — `\home\khoa\.aoide` under `C:\` — which is what this chain
/// did before the arm existed (measured on ThinkChiyo: `aoide daemon` with
/// `$HOME` cleared wrote `C:\home\khoa\.aoide\log`). A fabricated path is not
/// the refusal convention's answer; the host's own per-user directory is.
#[cfg(unix)]
pub fn aoide_home() -> PathBuf {
    if let Ok(user) = std::env::var("AOIDE_USER") {
        if !user.is_empty() {
            return user_home(&user);
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(passwd_home)
        .unwrap_or_else(std::env::temp_dir)
}

/// The effective uid's home from the passwd database (`getpwuid_r`), or `None`
/// when that uid has no entry or an empty home.
#[cfg(unix)]
pub fn passwd_home() -> Option<PathBuf> {
    passwd_dir(|pwd, buf, found| {
        // SAFETY: the out-parameters are the ones `getpwuid_r` documents, and
        // `buf` is passed with its own length.
        unsafe { libc::getpwuid_r(libc::geteuid(), pwd, buf.as_mut_ptr(), buf.len(), found) }
    })
}

/// `user`'s home from the passwd database (`getpwnam_r`), or `/home/<user>`
/// when the name has no entry (or cannot be a C string).
#[cfg(unix)]
pub fn user_home(user: &str) -> PathBuf {
    let fallback = || Path::new("/home").join(user);
    let Ok(name) = std::ffi::CString::new(user) else {
        return fallback();
    };
    passwd_dir(|pwd, buf, found| {
        // SAFETY: `name` is a live NUL-terminated string, and the rest are the
        // out-parameters `getpwnam_r` documents, `buf` with its own length.
        unsafe { libc::getpwnam_r(name.as_ptr(), pwd, buf.as_mut_ptr(), buf.len(), found) }
    })
    .unwrap_or_else(fallback)
}

/// Runs one passwd lookup, growing its buffer on `ERANGE` up to 1 MiB, and
/// returns the entry's home directory.
#[cfg(unix)]
fn passwd_dir(
    lookup: impl Fn(*mut libc::passwd, &mut [libc::c_char], *mut *mut libc::passwd) -> libc::c_int,
) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf: Vec<libc::c_char> = vec![0; 1024];
    loop {
        // SAFETY: all-zero is a valid `passwd` (integers and raw pointers), and
        // the lookup fills it before anything reads it.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let ret = lookup(&mut pwd, &mut buf, &mut found);
        if ret == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if ret != 0 || found.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        // SAFETY: on success `pw_dir` is a NUL-terminated string inside `buf`,
        // which is still alive here.
        let dir = unsafe { std::ffi::CStr::from_ptr(pwd.pw_dir) }.to_bytes();
        return (!dir.is_empty()).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(dir)));
    }
}

#[cfg(windows)]
pub fn aoide_home() -> PathBuf {
    if let Some(profile) = std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()) {
        return PathBuf::from(profile);
    }
    let drive = std::env::var("HOMEDRIVE").unwrap_or_default();
    let rest = std::env::var("HOMEPATH").unwrap_or_default();
    if !drive.is_empty() && !rest.is_empty() {
        return PathBuf::from(format!("{drive}{rest}"));
    }
    std::env::temp_dir()
}

/// Which door an operation came through — both share one gate and one log.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Door {
    Cli,
    Mcp,
    Daemon,
    /// The A2A (Agent2Agent) HTTP door (CONTRACTS.md §6) — `aoide a2a serve`.
    /// Every handled HTTP request is audited through this door, same as the
    /// other two.
    A2a,
    /// A far call a pair partner made, forwarded through its own link
    /// (gate-design.md §2.3).
    Pair,
}

impl Door {
    /// Whether this door may approve a gated send (the door half of the send
    /// gate). `Cli` (a person or a local agent at a terminal) and `Daemon`
    /// (aoided's own dispatch) keep every approval route: `--yes`, the box-wide
    /// autogate environment, the parent and sibling rules, `session pending
    /// approve`. `A2a` (the network door) cannot, and a send through it parks
    /// as pending whatever it asks for. `Mcp` (stdio, and strillix's) cannot
    /// approve either: it has no `--yes`, no autogate switch, no `session
    /// pending approve`, `session permit` or `mail ring`. What it can do is be
    /// attested like the CLI: a send whose MCP server runs under a sealed
    /// session earns that session's parent and sibling rules and delivers, and
    /// any other MCP send parks. Exhaustive on purpose, so a new door has to be
    /// decided here rather than inherit an answer. `Pair` (a far call a pair
    /// partner made) cannot either: the partner is not a decider at this box's
    /// terminal.
    pub fn can_approve(self) -> bool {
        match self {
            Door::Cli | Door::Daemon => true,
            Door::Mcp | Door::A2a | Door::Pair => false,
        }
    }
}

/// Event classes for the neutral event stream. Subscriptions are default-deny
/// per class, so OSD noise never burns an agent run and forwarded notification
/// text never reaches an agent as a command (entities/aoided).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum EventClass {
    /// Audit records (every operation).
    Audit,
    /// User-gate proposals awaiting admission.
    Gate,
    /// Content-pipeline lifecycle.
    Content,
    /// Forwarded OS notifications — UNTRUSTED payload, wrapped as data.
    Notification,
    /// Secrets-broker resolve-attempt mirror (Workstream SECRETS, P-V2,
    /// `aoide-secrets`'s `broker` module): secret name, consumer,
    /// granted/denied, argv0 if the client sent one — NEVER a value.
    /// `untrusted_data` is FORBIDDEN on this class: a Secret event has no
    /// forwarded payload to carry (unlike `Notification`), and the value
    /// itself never reaches the audit path at all, by construction. See
    /// [`append_audit`] — the ban is enforced there, not only by this
    /// comment.
    Secret,
}

/// A single record in the neutral event stream / audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub ts: u64,
    pub door: Door,
    pub class: EventClass,
    pub command: String,
    pub status: String,
    pub message: String,
    /// Untrusted forwarded payload, always carried as opaque data, never code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untrusted_data: Option<String>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The audit log records that an operation happened, not what it printed.
/// Except for pairing ceremonies, whose dispatcher withholds the human
/// message, a command's `Outcome` message is also its audit payload, so a
/// command that renders content as its message — `aoide mail show`'s whole
/// letter, `mail read`'s every printed entry — would otherwise leave an
/// unbounded second copy sitting in `~/.aoide/log`, one the mailbase's own
/// `mail rm --older-than` can never reach. [`append_audit`] clamps to this
/// bound instead of trusting every future message-bearing command to stay
/// short on its own.
const STORED_MESSAGE_MAX_BYTES: usize = 512;

/// Appended once a stored message is cut short — never counted against
/// [`STORED_MESSAGE_MAX_BYTES`] itself, so a maximally clamped message is
/// that many content bytes plus this marker, not fewer.
const STORED_MESSAGE_TRUNCATION_MARKER: &str = " … (truncated)";

/// `Some(clamped)` when `msg` is over [`STORED_MESSAGE_MAX_BYTES`]; `None`
/// when it already fits and nothing about it changes. Cuts on the last
/// UTF-8 character boundary at or before the bound — never mid-codepoint —
/// so the result is always valid `str` before the marker is appended.
fn clamp_stored_message(msg: &str) -> Option<String> {
    if msg.len() <= STORED_MESSAGE_MAX_BYTES {
        return None;
    }
    let mut cut = STORED_MESSAGE_MAX_BYTES;
    while cut > 0 && !msg.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut clamped = msg[..cut].to_string();
    clamped.push_str(STORED_MESSAGE_TRUNCATION_MARKER);
    Some(clamped)
}

/// Append one JSON-lines record to the single audit log (real code path).
/// Creates the parent directory and the file if absent; idempotent per-call.
///
/// **`EventClass::Secret` may never carry `untrusted_data`** (Workstream
/// SECRETS's audit design, `aoide-secrets`'s `broker` module doc): a Secret
/// event mirrors a name-only resolve attempt (secret name, consumer,
/// granted/denied) — never a value, and `untrusted_data` is exactly the
/// field every other class uses to carry arbitrary forwarded text
/// verbatim (`Notification`'s whole reason for existing). Enforced HERE,
/// not only by convention at each call site: a `Secret`-classed record
/// that somehow arrives with `untrusted_data: Some(_)` has it forced back
/// to `None` before the write (with an `eprintln!` naming the mistake) —
/// never a panic. An audit call already runs behind `let _ = audit(...)`
/// at every call site in this workspace (a logging failure must never take
/// the caller down with it); this guard keeps that posture rather than
/// trading a data-shape mistake for a crashed process.
///
/// **`message` is clamped to [`STORED_MESSAGE_MAX_BYTES`]** the same way,
/// and for the same reason: the bound belongs here, once, so every door and
/// every future caller inherits it rather than each remembering to clamp
/// its own outcome message before logging it. Only the STORED copy is
/// bounded — the caller's own `Outcome` is never touched, so human and
/// JSON output keep printing in full.
///
/// **Every string field is escaped** by [`escape_for_log`] before the write:
/// a bidi override or C1 control in the command, status, message or forwarded
/// text is stored as visible `\u{hex}` text, and no caller can skip it. The
/// escape runs before the clamp, so the bound counts the text that is stored.
pub fn append_audit(log_path: &Path, record: &AuditRecord) -> std::io::Result<()> {
    let strip_secret = record.class == EventClass::Secret && record.untrusted_data.is_some();
    if strip_secret {
        eprintln!(
            "[aoide/protocol] BUG: an EventClass::Secret audit record carried untrusted_data — stripping it before writing (command: {})",
            record.command
        );
    }
    let message = escape_for_log(&record.message);
    let record = AuditRecord {
        command: escape_for_log(&record.command),
        status: escape_for_log(&record.status),
        message: clamp_stored_message(&message).unwrap_or(message),
        untrusted_data: if strip_secret { None } else { record.untrusted_data.as_deref().map(escape_for_log) },
        ..record.clone()
    };

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(&record)
        .unwrap_or_else(|e| format!("{{\"ts\":{},\"error\":\"{e}\"}}", record.ts));
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    f.write_all(line.as_bytes())
}

/// Convenience: log an operation to the single audit log.
pub fn audit(
    log_path: &Path,
    door: Door,
    class: EventClass,
    command: &str,
    status: &str,
    message: &str,
) -> std::io::Result<()> {
    append_audit(
        log_path,
        &AuditRecord {
            ts: now_secs(),
            door,
            class,
            command: command.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            untrusted_data: None,
        },
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_local_doors_can_approve() {
        use super::Door;
        assert!(Door::Cli.can_approve());
        assert!(Door::Daemon.can_approve());
        assert!(!Door::Mcp.can_approve());
        assert!(!Door::A2a.can_approve());
    }

    use super::*;

    fn tmp_log(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aoide-protocol-audit-test-{tag}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        dir.join("log")
    }

    fn read_lines(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// The pair door approves nothing, and a call through it is audited
    /// as `pair`.
    #[test]
    fn a_pair_call_approves_nothing_and_is_audited_as_pair() {
        assert!(!Door::Pair.can_approve());
        let log = tmp_log("pair-door");
        audit(&log, Door::Pair, EventClass::Audit, "pair.call", "ok", "forwarded").unwrap();
        let lines = read_lines(&log);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["door"], "pair", "{:?}", lines[0]);
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// The normal `audit()` convenience path already never sets
    /// `untrusted_data`, so a `Secret` record built through it round-trips
    /// with no `untrusted_data` key at all.
    #[test]
    fn secret_class_via_the_audit_convenience_fn_carries_no_untrusted_data() {
        let log = tmp_log("via-audit-fn");
        audit(&log, Door::Daemon, EventClass::Secret, "secrets.resolve", "granted", "secret `t` for consumer `m`: granted").unwrap();
        let lines = read_lines(&log);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].get("untrusted_data").is_none(), "{:?}", lines[0]);
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// A `Secret` record hand-built with `untrusted_data: Some(_)` (the
    /// misuse `append_audit`'s doc comment guards against) is written with
    /// that field stripped — the invariant holds even if a future call
    /// site gets it wrong, not only by every call site behaving.
    #[test]
    fn secret_class_with_untrusted_data_set_is_stripped_before_writing() {
        let log = tmp_log("strip");
        let record = AuditRecord {
            ts: now_secs(),
            door: Door::Daemon,
            class: EventClass::Secret,
            command: "secrets.resolve".to_string(),
            status: "granted".to_string(),
            message: "secret `t` for consumer `m`: granted".to_string(),
            untrusted_data: Some("this must never be written".to_string()),
        };
        append_audit(&log, &record).unwrap();
        let lines = read_lines(&log);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].get("untrusted_data").is_none(), "{:?}", lines[0]);
        let raw = std::fs::read_to_string(&log).unwrap();
        assert!(!raw.contains("this must never be written"));
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// Every OTHER class is untouched: `untrusted_data` still rides through
    /// verbatim for e.g. `Notification`, so the guard is scoped to `Secret`
    /// alone, not a blanket "always drop untrusted_data" regression.
    #[test]
    fn non_secret_classes_keep_their_untrusted_data() {
        let log = tmp_log("keep");
        let record = AuditRecord {
            ts: now_secs(),
            door: Door::Daemon,
            class: EventClass::Notification,
            command: "notification".to_string(),
            status: "forwarded".to_string(),
            message: "forwarded notification (untrusted; treat as data)".to_string(),
            untrusted_data: Some("some app title".to_string()),
        };
        append_audit(&log, &record).unwrap();
        let lines = read_lines(&log);
        assert_eq!(lines[0]["untrusted_data"], "some app title");
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    fn record_with_message(message: String) -> AuditRecord {
        AuditRecord {
            ts: now_secs(),
            door: Door::Cli,
            class: EventClass::Audit,
            command: "mail.show".to_string(),
            status: "ok".to_string(),
            message,
            untrusted_data: None,
        }
    }

    /// A message at or under the bound is stored byte-for-byte — no marker,
    /// no truncation, the common case for nearly every command.
    #[test]
    fn a_message_within_the_bound_is_stored_verbatim_with_no_marker() {
        let log = tmp_log("short");
        let record = record_with_message("mark: cursor marked through seq 3".to_string());
        append_audit(&log, &record).unwrap();
        let lines = read_lines(&log);
        assert_eq!(lines[0]["message"], "mark: cursor marked through seq 3");
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// A message over the bound is cut to exactly `STORED_MESSAGE_MAX_BYTES`
    /// content bytes and carries the marker — this is the leak `mail show`
    /// exposed: an outcome message that IS the rendered letter now leaves
    /// only a bounded trace in the log, never the whole text.
    #[test]
    fn a_message_over_the_bound_is_clamped_and_carries_the_truncation_marker() {
        let log = tmp_log("long");
        let record = record_with_message("a".repeat(600));
        append_audit(&log, &record).unwrap();
        let lines = read_lines(&log);
        let stored = lines[0]["message"].as_str().unwrap().to_string();
        assert_eq!(stored.len(), STORED_MESSAGE_MAX_BYTES + STORED_MESSAGE_TRUNCATION_MARKER.len());
        assert!(stored.starts_with(&"a".repeat(STORED_MESSAGE_MAX_BYTES)));
        assert!(stored.ends_with(STORED_MESSAGE_TRUNCATION_MARKER));
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// A message whose byte 512 falls inside a multi-byte character clamps
    /// on the boundary BELOW it, never mid-codepoint, and the record still
    /// round-trips as valid JSON — the whole point of cutting on a boundary
    /// rather than a raw byte offset.
    #[test]
    fn clamping_a_split_codepoint_lands_on_the_boundary_below_it_and_still_parses() {
        let log = tmp_log("boundary");
        let mut message = "a".repeat(511);
        message.push('€'); // 3-byte UTF-8 char starting at byte 511: occupies 511..514
        message.push_str(&"b".repeat(100));
        assert!(!message.is_char_boundary(STORED_MESSAGE_MAX_BYTES), "the fixture must actually straddle byte 512");
        let record = record_with_message(message);
        append_audit(&log, &record).unwrap();

        let raw = std::fs::read_to_string(&log).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(raw.lines().next().unwrap()).expect("clamped record still parses as JSON");
        let stored = parsed["message"].as_str().unwrap();
        assert_eq!(stored, format!("{}{}", "a".repeat(511), STORED_MESSAGE_TRUNCATION_MARKER));
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }

    /// The native home is THIS host's per-user directory, never the Unix
    /// chain's answer — `$HOME` (which Windows does not set; a shell that
    /// invents one is not a host fact) or `$AOIDE_USER` (a Linux deployment
    /// variable). Both are set to decoys here and BOTH must lose: before this
    /// arm existed the answer was `\home\khoa`, a path off the current drive,
    /// which is the fabricated path this test refuses. The decoy for `$HOME`
    /// is deliberately a path that does not exist, so a chain that consulted it
    /// cannot pass by coincidence.
    #[cfg(windows)]
    #[test]
    fn the_native_home_is_the_hosts_own_per_user_directory_never_a_unix_shape() {
        let saved_profile = std::env::var("USERPROFILE").ok();
        let saved_home = std::env::var("HOME").ok();
        let saved_user = std::env::var("AOIDE_USER").ok();

        let profile = std::env::temp_dir().join(format!("aoide-home-probe-{}", std::process::id()));
        std::env::set_var("USERPROFILE", &profile);
        std::env::set_var("HOME", r"C:\home\khoa");
        std::env::set_var("AOIDE_USER", "khoa");
        let answer = aoide_home();
        assert_eq!(answer, profile, "the host's own per-user directory is the home");
        assert!(!answer.to_string_lossy().contains("home\\khoa"), "never the Unix chain's shape: {answer:?}");
        assert_eq!(default_audit_log(), profile.join(".aoide").join("log"), "and the root's default log rides it");

        match saved_profile {
            Some(v) => std::env::set_var("USERPROFILE", v),
            None => std::env::remove_var("USERPROFILE"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match saved_user {
            Some(v) => std::env::set_var("AOIDE_USER", v),
            None => std::env::remove_var("AOIDE_USER"),
        }
    }

    /// Serialises the tests below that set `HOME`/`AOIDE_USER`. This crate has
    /// no shared env lock (the storage crate's [`env_lock`] is its own binary's),
    /// so the one this module needs is local to it.
    #[cfg(unix)]
    static HOME_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sets `HOME` and `AOIDE_USER` to `home`/`user` (`None` unsets), runs `f`,
    /// and puts both back.
    #[cfg(unix)]
    fn with_home_env(home: Option<&str>, user: Option<&str>, f: impl FnOnce()) {
        let _guard = HOME_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let saved = (std::env::var_os("HOME"), std::env::var_os("AOIDE_USER"));
        for (var, value) in [("HOME", home), ("AOIDE_USER", user)] {
            match value {
                Some(v) => std::env::set_var(var, v),
                None => std::env::remove_var(var),
            }
        }
        f();
        match saved.0 {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match saved.1 {
            Some(v) => std::env::set_var("AOIDE_USER", v),
            None => std::env::remove_var("AOIDE_USER"),
        }
    }

    /// The name of the effective uid in the passwd database, or `None`.
    #[cfg(unix)]
    fn euid_name() -> Option<String> {
        let mut buf = vec![0 as libc::c_char; 16 * 1024];
        // SAFETY: all-zero is a valid `passwd`; `getpwuid_r` fills it.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: out-parameters as `getpwuid_r` documents, `buf` with its length.
        let ret = unsafe { libc::getpwuid_r(libc::geteuid(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found) };
        (ret == 0 && !found.is_null()).then(|| {
            // SAFETY: on success `pw_name` is NUL-terminated inside `buf`.
            unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) }.to_string_lossy().into_owned()
        })
    }

    /// With `HOME` unset (and `AOIDE_USER` too) the home is the euid's passwd
    /// home. The old chain returned the literal `/home/khoa` here. On this box
    /// the euid is khoa, whose passwd home is `/home/khoa`, so the literal passed
    /// by coincidence: the literal is only refuted on another account.
    #[cfg(unix)]
    #[test]
    fn with_no_home_the_home_is_the_euid_passwd_home() {
        let expected = passwd_home().unwrap_or_else(std::env::temp_dir);
        with_home_env(None, None, || assert_eq!(aoide_home(), expected));
        assert!(expected.is_absolute(), "the answer is a real directory, not a fragment: {expected:?}");
    }

    /// `AOIDE_USER=root` names root's passwd home, not `/home/root`. The old
    /// `Path::new("/home").join(user)` answered `/home/root` here. The expected
    /// value is the passwd entry itself, not a literal: a nix build sandbox
    /// gives root the home `/build`.
    #[cfg(unix)]
    #[test]
    fn aoide_user_resolves_through_the_passwd_database() {
        let root = user_home("root");
        assert!(root.is_absolute(), "root has a passwd home: {root:?}");
        assert_ne!(root, std::path::Path::new("/home/root"), "no /home/root guess");
        with_home_env(Some("/home/ignored"), Some("root"), || {
            assert_eq!(aoide_home(), root, "AOIDE_USER beats HOME");
        });
    }

    /// The euid's own name resolves to the euid's passwd home, through the
    /// name lookup and the uid lookup alike.
    #[cfg(unix)]
    #[test]
    fn aoide_user_of_the_current_user_is_its_passwd_home() {
        let name = euid_name().expect("the test uid has a passwd entry");
        let home = passwd_home().expect("the test uid has a passwd home");
        assert_eq!(user_home(&name), home);
        with_home_env(None, Some(&name), || assert_eq!(aoide_home(), home));
    }

    /// A name with no passwd entry keeps the `/home/<name>` answer.
    #[cfg(unix)]
    #[test]
    fn an_unknown_aoide_user_keeps_the_home_name_answer() {
        let name = format!("aoide-no-such-user-{}", std::process::id());
        let expected = std::path::Path::new("/home").join(&name);
        assert_eq!(user_home(&name), expected);
        with_home_env(Some("/home/ignored"), Some(&name), || assert_eq!(aoide_home(), expected));
    }

    /// A bidi override (U+202E) and a C1 control (U+009B) in any string field
    /// reach the log as escapes, not raw. The JSON writer leaves both raw, so
    /// the writer escapes them for every caller.
    #[test]
    fn append_audit_escapes_a_bidi_override_and_a_c1_control_in_every_string_field() {
        let log = tmp_log("escape-every-field");
        let record = AuditRecord {
            ts: 1,
            door: Door::Cli,
            class: EventClass::Notification,
            command: "cmd\u{202e}".to_string(),
            status: "ok\u{9b}".to_string(),
            message: "msg\u{202e}\u{9b}".to_string(),
            untrusted_data: Some("data\u{9b}".to_string()),
        };
        append_audit(&log, &record).unwrap();
        let text = std::fs::read_to_string(&log).unwrap();
        for raw in ['\u{202e}', '\u{9b}'] {
            assert!(!text.contains(raw), "a raw {raw:?} reached the log: {text}");
        }
        assert!(text.contains(r"\u{202e}") && text.contains(r"\u{9b}"), "the escapes are written: {text}");
    }

    /// A typed `\u{202e}` and a real U+202E in one message reach the log as
    /// two different escapes, each once, and no raw bidi character reaches it.
    #[test]
    fn a_typed_escape_and_a_bidi_char_in_one_message_reach_the_log_apart() {
        let log = tmp_log("typed-and-char");
        let message = format!("{} then {}", r"\u{202e}", '\u{202e}');
        append_audit(&log, &record_with_message(message)).unwrap();
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(!text.contains('\u{202e}'), "raw U+202E in the log: {text}");
        let lines = read_lines(&log);
        assert_eq!(lines[0]["message"], r"\\u{202e} then \u{202e}", "{text}");
        std::fs::remove_dir_all(log.parent().unwrap()).ok();
    }
}
