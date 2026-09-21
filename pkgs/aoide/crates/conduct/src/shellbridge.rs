//! shellbridge's socket loop — the session/hook state bridge
//! (concepts/shellbridge).
//!
//! Registers agent sessions + window addresses and records Claude Code hook
//! phases, publishing them to `state/stage/` for Quickshell to read. Writes
//! are atomic (write-temp-then-rename) so a hot-reload never sees a torn file
//! (CONTRACTS.md §4 discipline).
//!
//! The process seeds the stage files (atomic writer), then binds its unix
//! socket and serves newline-delimited JSON commands: a widget click sends
//! `{ "cmd": "focuswindow", "address": "0x…" }` and shellbridge dispatches the
//! Hyprland focus (the Terminal-Commander session-jump flow). The accept loop
//! is robust: a malformed line, an unknown command, or a dropped connection is
//! logged and skipped — nothing ever kills the service.
//!
//! Moved here from root `src/shellbridge.rs` (Phase 3b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) — ONLY the socket-loop half
//! (`socket_path`, `BridgeCommand`, `parse_command`, `run`); the stage-file FS
//! substrate (`stage_dir`, `atomic_write`, `with_stage_lock`, …) already
//! moved to `aoide-storage` in Phase 3a and stays there. Root re-exports both
//! halves at their old `crate::shellbridge::*` path so no caller changes.

use aoide_protocol as daemon;
use aoide_storage::fs::{conducting_stage_dir, seed_if_absent};
use aoide_storage::mode::{load_mode_marker, RiceMode};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// The shellbridge socket path — **contract**: never computed independently.
/// `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`.
pub fn socket_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
    PathBuf::from(runtime)
        .join("aoide")
        .join("shellbridge.sock")
}

/// Records one `sessiontrace` answer carries when the wire names no `lines` —
/// the last few turns, not a session's history.
const TRACE_LINES_DEFAULT: usize = 12;
/// Records a `sessiontrace` answer may carry, whatever the wire asks for: the
/// clamp that keeps one answer small no matter what cadence a caller keeps.
const TRACE_LINES_MAX: usize = 40;
/// How long one `sessiontrace` answer may take. Deliberately longer than the
/// producer pull's own five-second wall clock
/// (`docs/architecture/EIDOLON-TRACE.md`), so this bound never fires on a
/// legitimately slow export — only on a wedged one.
const TRACE_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// How often the bounded runner re-checks a child that has not exited.
const CHILD_POLL: std::time::Duration = std::time::Duration::from_millis(20);
/// How long a child's pipes are given to reach EOF after the child itself
/// exited. Past it the read is not the whole answer and is DISCARDED rather
/// than inferred from — which is what keeps the wall clock bounded when a
/// DESCENDANT of the child inherited a pipe and holds it open (the same
/// discipline, and the same reason, as the producer pull's own
/// `PULL_DRAIN_GRACE` in `protocol/src/agents.rs`).
const CHILD_DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);
/// Chunks in flight between a reader thread and the wait loop (4 × 64 KiB) —
/// the backpressure that stops a fast child from queueing a whole answer in
/// memory while the loop is asleep.
const READ_CHUNKS: usize = 4;
/// Bytes the bounded runner will COLLECT from a child's own streams before
/// abandoning the read: a last-resort ceiling for the ANSWER, above anything
/// the CLI's own bounds (`--tail` window × clip) can produce.
const MAX_ANSWER_BYTES: u64 = 4 * 1024 * 1024;

/// One parsed inbound command from the socket wire (newline-delimited JSON).
/// The wire shape is defined by ShellBridge.qml; the only command today is the
/// session-jump `focuswindow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeCommand {
    /// `{ "cmd": "focuswindow", "address": "0x…" }` — jump to a window.
    Focus { address: String },
    /// `{ "cmd": "focussession", "sessionId": "…" }` — jump to a SESSION by id.
    /// The daemon resolves id → `windowAddress` (focus the exact window), or
    /// falls back to id → `workspace` (switch to it) when the address isn't
    /// resolved yet. This is the source-of-truth jump: QML sends only the
    /// sessionId a roster row already holds, never a stale/empty address.
    FocusSession { session_id: String },
    /// `{ "cmd": "power", "action": "lock|logout|suspend|hibernate|reboot|shutdown" }`
    /// — a system action from the Exodos powermenu (AoideExodos.qml). QML never
    /// shells out; this command is the gate through which the six endings reach
    /// hyprlock / hyprctl / systemctl.
    Power { action: PowerAction },
    /// `{ "cmd": "ricemode" }` — a click on the bar's rice-mode cell
    /// (bar.qml's `modeText`). No payload: the daemon reads
    /// `stage/mode.json` itself and decides the target — a two-way toggle
    /// (`staging ⇄ declarative`), never a picker QML would need to supply
    /// state for. See [`dispatch_rice_mode_toggle`].
    ToggleRiceMode,
    /// `{ "cmd": "refreshusage" }` — a click on the CLAUDE ledger gadget's ❋
    /// spark (UsageGadget.qml). No payload: the daemon re-runs `aoide usage`
    /// itself, which atomic-writes `state/usage.json`, and the gadget's own
    /// FileView watch picks the new file up and re-renders — the manual
    /// analogue of the poller timer's periodic write. See
    /// [`dispatch_usage_refresh`].
    RefreshUsage,
    /// `{ "cmd": "rechecksessions" }` — a click on the Terminals/Conductor
    /// header recheck control. No payload: the daemon re-execs `aoide graph
    /// reap` itself — the liveness/rehook sweep (reap dead sessions, decay
    /// `stopped` → `idle`, prune orphaned hook records) the ~12s
    /// `aoide-graph-reap.timer` runs periodically — so a resumed/exited session
    /// is re-evaluated NOW instead of waiting up to a full timer period. The
    /// gadgets refresh off the resulting `sessions.json`/`hooks.json`/
    /// `graph.json` writes through their own FileView watches. See
    /// [`dispatch_recheck_sessions`].
    RecheckSessions,
    /// Acknowledged session-menu action, routed through the core CLI.
    SessionAction { session_id: String, action: String, fields: Value },
    /// `{ "cmd": "projectaction", "action": "create|edit|removehost", "name":
    /// "…", "paths": […], "hosts": […] }` — zero-session project creation,
    /// editing, or host-removal from the song-side project picker (P-14 M1
    /// §2d). Unlike [`Self::SessionAction`] this carries no session id
    /// anywhere: the reply's identity key is `"name"`. `fields` is the
    /// ENTIRE parsed wire object — `paths`/`hosts` sit at the top level, not
    /// nested under a `fields` key, matching the wire shape exactly.
    ProjectAction { name: String, action: String, fields: Value },
    /// `{ "cmd": "heraldpush", "notification": { … } }` — file one notification
    /// into `stage/herald.json`. Sent by `aoide herald push` (dunst's `script`
    /// hook) and by `graph permit` for a summons; NOT by QML, which only reads
    /// the ledger. The daemon is the single writer, which is the whole point:
    /// dunst runs its scripts asynchronously, so two notifications arriving
    /// together would otherwise race and one would be lost. See
    /// [`dispatch_herald_push`].
    HeraldPush { notification: Box<Value> },
    /// `{ "cmd": "heraldverdict", "id": "…", "verdict": "approve|deny" }` — the
    /// human clicked a summons' approve or deny button in the QML herald. The
    /// daemon types the verdict into the waiting session through
    /// `graph send`, the one gated injection door. This is the button that
    /// dunst physically could not draw: it had no per-region hit testing, so a
    /// drawn deny chip fired the window-wide left-click binding and APPROVED.
    HeraldVerdict { id: String, verdict: String },
    /// `{ "cmd": "heralddismiss", "id": "…" }` — drop one card from the ledger.
    /// The QML herald owns the dismiss clock (a notification dunst never
    /// displays is never expired by dunst either), so this is how a timeout or
    /// a click closes a card. `"*"` clears the desk.
    HeraldDismiss { id: String },
    /// `{ "cmd": "sessiontrace", "sessionId": "…", "lines": N, "clip":
    /// "line|detail" }` — the conductor card's own READ-ONLY trace query: what
    /// this one session has emitted (thinking, text, tool calls, tool results,
    /// settled turns), projected by `session trace --json`'s `data.steps`.
    ///
    /// It re-execs the existing CLI — `aoide session trace <id> --tail N
    /// --clip <clip> --json` — through the same `core_bin()` path
    /// [`Self::RefreshUsage`]/[`Self::RecheckSessions`] take, and answers on
    /// this connection the way [`Self::SessionAction`] does. Nothing is
    /// written, nothing is resolved here: the CLI's own resolver, capability
    /// refusal and projection are the whole answer, carried verbatim.
    ///
    /// Bounded by construction, whatever the caller's cadence: `lines` is
    /// clamped to [`TRACE_LINES_MAX`] (absent → [`TRACE_LINES_DEFAULT`], a
    /// non-number or `0` refused), `clip` defaults to `line` and an unknown
    /// word is REFUSED rather than silently widened, and the child is killed
    /// and reaped on [`TRACE_QUERY_TIMEOUT`]. `sessionId` is non-blank and
    /// carries no whitespace/control characters, exactly like
    /// [`Self::FocusSession`]'s — it becomes one argv element, never a shell
    /// word.
    SessionTrace { session_id: String, lines: usize, clip: String },
}

/// The six system actions the powermenu can request. A closed set — an unknown
/// action string parses to `None` at the wire, never to a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl PowerAction {
    /// Parse the wire's `action` string. Case-sensitive lowercase by contract
    /// (ShellBridge.qml sends exactly these), anything else is `None`.
    fn from_wire(s: &str) -> Option<Self> {
        match s {
            "lock" => Some(Self::Lock),
            "logout" => Some(Self::Logout),
            "suspend" => Some(Self::Suspend),
            "hibernate" => Some(Self::Hibernate),
            "reboot" => Some(Self::Reboot),
            "shutdown" => Some(Self::Shutdown),
            _ => None,
        }
    }

    /// The program + args this action spawns. `lock` matches the existing
    /// `lock` shell alias (hyprlock); `logout` exits the compositor; the rest
    /// are systemd commands.
    fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Lock => ("hyprlock", &[]),
            Self::Logout => ("hyprctl", &["dispatch", "exit"]),
            Self::Suspend => ("systemctl", &["suspend"]),
            Self::Hibernate => ("systemctl", &["hibernate"]),
            Self::Reboot => ("systemctl", &["reboot"]),
            Self::Shutdown => ("systemctl", &["poweroff"]),
        }
    }

    /// The wire name back, for audit lines.
    fn as_str(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Logout => "logout",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::Reboot => "reboot",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Parse ONE wire line into a [`BridgeCommand`]. Pure and total: malformed
/// JSON, a missing/unknown `cmd`, or a `focuswindow` with an empty/absent
/// `address` all yield `None` (the loop logs and ignores them) — never a panic.
pub fn parse_command(line: &str) -> Option<BridgeCommand> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("cmd").and_then(Value::as_str)? {
        "sessionaction" => {
            let session_id = v.get("sessionId")?.as_str()?.trim().to_string();
            let action = v.get("action")?.as_str()?.to_string();
            let fields = v.get("fields").cloned().unwrap_or_else(|| json!({}));
            session_action_args(&session_id, &action, &fields)?;
            Some(BridgeCommand::SessionAction { session_id, action, fields })
        }
        "projectaction" => {
            let name = v.get("name")?.as_str()?.trim().to_string();
            let action = v.get("action")?.as_str()?.to_string();
            // `fields` here IS the whole wire object — `paths`/`hosts` live
            // at the top level (§2d's wire shape), never nested.
            project_action_args(&name, &action, &v)?;
            Some(BridgeCommand::ProjectAction { name, action, fields: v })
        }
        "focuswindow" => {
            let address = v
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if address.is_empty() {
                return None;
            }
            Some(BridgeCommand::Focus { address })
        }
        "focussession" => {
            let session_id = v
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if session_id.is_empty() {
                return None;
            }
            Some(BridgeCommand::FocusSession { session_id })
        }
        "power" => {
            let action = v
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            PowerAction::from_wire(&action).map(|action| BridgeCommand::Power { action })
        }
        "ricemode" => Some(BridgeCommand::ToggleRiceMode),
        "refreshusage" => Some(BridgeCommand::RefreshUsage),
        "rechecksessions" => Some(BridgeCommand::RecheckSessions),
        "heraldpush" => {
            let notification = v.get("notification")?.clone();
            // A record with no id is unfilable — it could neither replace its
            // predecessor nor be dismissed later.
            let id = notification.get("id").and_then(Value::as_str)?;
            if id.trim().is_empty() {
                return None;
            }
            Some(BridgeCommand::HeraldPush {
                notification: Box::new(notification),
            })
        }
        "heraldverdict" => {
            let id = v.get("id").and_then(Value::as_str)?.trim().to_string();
            let verdict = v.get("verdict").and_then(Value::as_str)?.trim().to_string();
            // A closed set: only the two real answers reach the injection door.
            // Anything else — a typo, a truncated wire line — is dropped here
            // rather than resolved to a default, because both defaults are
            // wrong on a permission gate.
            if id.is_empty() || !matches!(verdict.as_str(), "approve" | "deny") {
                return None;
            }
            Some(BridgeCommand::HeraldVerdict { id, verdict })
        }
        "heralddismiss" => {
            let id = v.get("id").and_then(Value::as_str)?.trim().to_string();
            if id.is_empty() {
                return None;
            }
            Some(BridgeCommand::HeraldDismiss { id })
        }
        "sessiontrace" => {
            let session_id = v
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            // A blank or non-id-shaped session names nothing to read: refused,
            // like `focussession`'s own blank id, never defaulted to "all".
            if !safe_session_id(&session_id) {
                return None;
            }
            // `lines` is clamped from ABOVE (a caller asking for more gets the
            // bound, not a refusal); a nonsense value — a string, a float, a
            // zero-length window — is refused, never quietly replaced by the
            // default, because a caller that asked for something else must not
            // be handed a window it did not ask for.
            let lines = match v.get("lines") {
                None | Some(Value::Null) => TRACE_LINES_DEFAULT,
                Some(n) => match n.as_u64() {
                    Some(0) | None => return None,
                    Some(n) => n.min(TRACE_LINES_MAX as u64) as usize,
                },
            };
            // An unknown clip is REFUSED rather than silently widened to
            // `detail`: a caller that meant the wider window and mistyped it
            // must not be handed the narrow one as though that were its ask.
            let clip = match v.get("clip") {
                None | Some(Value::Null) => "line",
                Some(Value::String(s)) => match s.trim() {
                    "line" => "line",
                    "detail" => "detail",
                    _ => return None,
                },
                Some(_) => return None,
            };
            Some(BridgeCommand::SessionTrace {
                session_id,
                lines,
                clip: clip.to_string(),
            })
        }
        _ => None,
    }
}

/// Dispatch ONE power action: spawn the mapped command and return. NEVER waits
/// for exit — several of these actions kill or freeze the very process that
/// would be waiting (logout tears the session down, suspend stops the clock) —
/// a detached reaper thread collects the child's status so it never lingers as
/// a zombie. A spawn failure is an `Err` for the caller to log; nothing here
/// can take down the accept loop.
fn dispatch_power(action: PowerAction) -> std::io::Result<()> {
    let (prog, args) = action.command();
    let mut child = std::process::Command::new(prog).args(args).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Pure decision: which `rice mode <word>` this toggle targets, given the
/// CURRENT mode. A two-way toggle, not a three-way cycle — `Staging` locks
/// to `declarative`; `Declarative` OR `Draft` both unlock back to `stage`
/// (exiting a draft session to plain staging this way is deliberate:
/// `rice mode stage` already tears down the draft's routing symlink on its
/// own, `commands/mode.rs`'s `handle_mode_stage`). There is no generic "next
/// draft" a bare click could cycle into without a name, so draft is only
/// ever reachable via `rice mode draft <name>`, never this toggle.
///
/// This decides ONLY the target word, not which song it acts on — see
/// [`dispatch_rice_mode_toggle`] for the asymmetric song-arg resolution
/// (`declarative` explicitly re-pins to `AOIDE_DEFAULT_SONG`; `stage` stays
/// bare).
fn rice_mode_toggle_target(current: RiceMode) -> &'static str {
    match current {
        RiceMode::Staging => "declarative",
        RiceMode::Declarative | RiceMode::Draft => "stage",
    }
}

/// Dispatch ONE rice-mode toggle. Unlike [`dispatch_power`], this WAITS for
/// the child (`.output()`, not spawn-and-detach): a mode switch never kills
/// or freezes this process the way logout/suspend do, so it's safe — and
/// necessary — to know synchronously whether the switch actually succeeded
/// before deciding whether to fire a notification.
///
/// Execs the SIBLING `lyra` binary (`daemon::bin::rice_bin()`, protocol's
/// sibling resolver — never a bare `"aoide"`, and never `current_exe()`:
/// `rice mode` lives in lyra, not this running binary, since P-A5 moved it
/// out of core) as `rice mode <target> --json`. This call is NOT inside
/// `with_stage_lock` — see `protocol::bin`'s module doc for why that would
/// matter if it ever were.
///
/// The two toggle directions are deliberately asymmetric about which song
/// they act on. `stage` (declarative/draft → staging) passes no song arg —
/// `handle_mode_stage` resolves via its own `current_staged_song()`, and
/// staying on whatever's currently being edited is the reasonable default
/// there. `declarative` (staging → declarative) is different: that
/// direction is supposed to mean "matches nix," not "frozen wherever I
/// happened to be," so it explicitly appends the nix-declared baseline song
/// (`AOIDE_DEFAULT_SONG`, baked into shellbridge.service by
/// modules/nucleus/shellbridge.nix — the same env-var precedent as
/// AOIDE_WALLPAPER) as the CLI arg, overriding `handle_mode_declarative`'s
/// bare-call fallback to whatever song is currently staged
/// (`commands/mode.rs`). If the env var is absent or empty (outside the
/// systemd service, or before a rebuild lands it) this falls back to the
/// existing bare no-arg call rather than erroring — a missing env var must
/// never turn a working toggle into a broken one. This asymmetry is scoped
/// to THIS dispatch path only: a bare `aoide rice mode declarative` typed
/// directly in a terminal is untouched and keeps resolving via
/// `current_staged_song()`.
///
/// On success (exit 0), returns the CLI's own `message` string verbatim —
/// reusing that exact copy rather than inventing new wording — and fires a detached
/// `notify-send "Aoide" <message>` (same reaper-thread idiom as
/// `dispatch_power`'s spawned child, so a slow/hung `notify-send` can never
/// block the accept loop; a `notify-send` spawn failure is a soft, eprintln
/// -only failure — the mode DID switch, so it must not be reported as a
/// toggle failure). On failure (non-zero exit, a spawn error, or unparsable
/// JSON on an exit-0 that shouldn't happen) returns `Err` for the caller to
/// audit-log — no notification fires for a failed toggle.
/// Pure decision: does the declarative-direction toggle have an explicit
/// baseline song to pass, given the toggle's target word and the CURRENT
/// `AOIDE_DEFAULT_SONG` env value (read by the caller, passed in untouched —
/// kept pure and out of `std::env` here so this is unit-testable without
/// mutating process-wide env state, which races under parallel tests). The
/// `stage` direction never gets one (see [`dispatch_rice_mode_toggle`]'s doc
/// comment for why); an absent or blank/whitespace-only value also yields
/// `None` — never pass an empty arg, and never let a missing env var
/// (outside the systemd service, or before a rebuild lands it) turn the
/// toggle into anything but the existing bare call.
fn rice_mode_toggle_default_song(target: &str, env_value: Option<&str>) -> Option<String> {
    if target != "declarative" {
        return None;
    }
    let trimmed = env_value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn dispatch_rice_mode_toggle() -> Result<String, String> {
    let current = load_mode_marker().mode;
    let target = rice_mode_toggle_target(current);
    let default_song =
        rice_mode_toggle_default_song(target, std::env::var("AOIDE_DEFAULT_SONG").ok().as_deref());

    let mut command = std::process::Command::new(daemon::bin::rice_bin());
    command.args(["rice", "mode", target]);
    if let Some(song) = &default_song {
        command.arg(song);
    }
    command.arg("--json");
    let output = command
        .output()
        .map_err(|e| format!("spawning `lyra rice mode {target}`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`lyra rice mode {target}` exited {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let message = serde_json::from_slice::<Value>(&output.stdout)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| format!("rice mode: {target}"));

    match std::process::Command::new("notify-send")
        .arg("Aoide")
        .arg(&message)
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => {
            // The mode switch itself already succeeded above — a dead/missing
            // notify-send must not turn a successful toggle into a reported
            // failure, so this is logged, not returned as `Err`.
            eprintln!("[aoide/shellbridge] notify-send failed (mode switch itself succeeded): {e}");
        }
    }

    Ok(message)
}

/// Pure decision: did one finished `aoide usage --json` actually refresh the
/// state file? Judged on the CLI's own JSON envelope `status`, NOT the exit
/// code alone — the same "success is the real output, not the exit status"
/// rule `song/src/ipc.rs`'s `classify_call` is built on. `aoide usage` prints
/// `{"status":"ok",…,"message":"…"}` and exits 0 on a real `state/usage.json`
/// write, and an error envelope (or a non-zero exit) when the atomic write
/// itself fails.
///
/// A DEGRADED live block is NOT a failure: a `live:{ok:false}` payload (no
/// credentials, the Claude-Code-only OAuth rejection, a transport error) still
/// yields status `ok` and a written file — exactly the graceful-degrade the
/// gadget is built to render — so the refresh SUCCEEDED. Only a failed write,
/// a non-zero exit, or unparseable output is an `Err`. Returns the CLI's own
/// `message` on success so the audit line reuses that exact wording rather than
/// inventing new copy (same discipline as [`dispatch_rice_mode_toggle`]).
fn classify_usage_refresh(exited_ok: bool, stdout: &str, stderr: &str) -> Result<String, String> {
    if !exited_ok {
        // `--json` still prints the structured envelope on failure; prefer a
        // spoken stderr, fall back to stdout, never a blank reason.
        let said = {
            let e = stderr.trim();
            if e.is_empty() {
                stdout.trim()
            } else {
                e
            }
        };
        return Err(if said.is_empty() {
            "`aoide usage` exited nonzero with no message".to_string()
        } else {
            said.to_string()
        });
    }
    let v: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| "`aoide usage --json` printed no parseable envelope".to_string())?;
    let message = v
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("usage refreshed")
        .to_string();
    match v.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(message),
        Some(other) => Err(format!("`aoide usage` reported status {other}: {message}")),
        None => Err("`aoide usage --json` output had no status field".to_string()),
    }
}

/// Dispatch ONE usage refresh: exec the core `aoide` binary
/// (`daemon::bin::core_bin()`, protocol's sibling resolver — never a bare
/// `"aoide"` relying on PATH alone) as `aoide usage --json` and audit the
/// outcome, judged on its real output via [`classify_usage_refresh`]. This
/// call is NOT inside `with_stage_lock` — see `protocol::bin`'s module doc
/// for why that would matter if it ever were.
///
/// Runs on a DETACHED thread. Unlike the rice-mode toggle (a fast local switch,
/// safe to `.output()` inline), `aoide usage`'s live fetch is `curl --max-time
/// 15`, so waiting for it inline would tie up this connection's own thread for
/// up to 15 s for no reason (§3b's thread-per-connection accept loop keeps
/// that from starving anyone else, but there is still no reason to hold it).
/// So this SPAWNS and returns immediately — the same no-block posture
/// [`dispatch_power`] takes — and the thread collects the child and audits the
/// result so it neither lingers nor blocks. The gadget updates itself off the
/// resulting `state/usage.json` write through its own FileView watch regardless
/// of what this logs; the audit line exists for the operator, not to push data.
/// Best-effort throughout: a spawn failure or a classify error is audited,
/// never panicked or propagated.
fn dispatch_usage_refresh() {
    std::thread::spawn(|| {
        let result = match std::process::Command::new(daemon::bin::core_bin())
            .args(["usage", "--json"])
            .output()
        {
            Ok(out) => classify_usage_refresh(
                out.status.success(),
                &String::from_utf8_lossy(&out.stdout),
                &String::from_utf8_lossy(&out.stderr),
            ),
            Err(e) => Err(format!("spawning `aoide usage`: {e}")),
        };
        let (event, detail) = match result {
            Ok(msg) => ("usage-refresh", msg),
            Err(e) => ("usage-refresh-failed", e),
        };
        let _ = daemon::audit(
            &daemon::default_audit_log(),
            daemon::Door::Daemon,
            daemon::EventClass::Audit,
            "shellbridge",
            event,
            &detail,
        );
    });
}

/// Pure decision: did one finished `aoide session reap --json` actually run the
/// sweep? Judged on the CLI's own JSON envelope `status`, NOT the exit code
/// alone — the same "success is the real output, not the exit status" rule
/// [`classify_usage_refresh`] follows. `aoide session reap` prints
/// `{"status":"ok",…,"message":"…"}` and exits 0 on every real pass, INCLUDING
/// a no-op "nothing to reap (all sessions live)" one — a quiet sweep is a
/// successful sweep, not a failure — and an error envelope (or a non-zero exit)
/// only when a stage read/write actually failed. Returns the CLI's own
/// `message` on success so the audit line reuses that exact wording rather than
/// inventing new copy.
fn classify_recheck(exited_ok: bool, stdout: &str, stderr: &str) -> Result<String, String> {
    if !exited_ok {
        let said = {
            let e = stderr.trim();
            if e.is_empty() {
                stdout.trim()
            } else {
                e
            }
        };
        return Err(if said.is_empty() {
            "`aoide session reap` exited nonzero with no message".to_string()
        } else {
            said.to_string()
        });
    }
    let v: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| "`aoide session reap --json` printed no parseable envelope".to_string())?;
    let message = v
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("sessions rechecked")
        .to_string();
    match v.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(message),
        Some(other) => Err(format!("`aoide session reap` reported status {other}: {message}")),
        None => Err("`aoide session reap --json` output had no status field".to_string()),
    }
}

/// Dispatch ONE session recheck: exec the core `aoide` binary
/// (`daemon::bin::core_bin()`, protocol's sibling resolver — never a bare
/// `"aoide"` relying on PATH alone) as `aoide session reap --announce --json`
/// — the liveness/rehook sweep (reap dead sessions, decay `stopped` →
/// `idle`, prune orphaned hook records, refresh every live agent's
/// transcript fields) the ~12s `aoide-graph-reap.timer` runs periodically —
/// so the Terminals/Conductor `[ reap ]` control triggers it NOW instead of
/// waiting up to a full timer period. This call is NOT inside
/// `with_stage_lock` — see `protocol::bin`'s module doc for why that would
/// matter if it ever were.
///
/// `--announce` is what makes the click ANSWER: the toast is unconditional here,
/// where a human pressed something, while the timer's own sweeps stay silent
/// unless they actually changed the roster (see `reap::reap_and_announce`). The
/// notification is raised by the child, not here — this thread only audits.
///
/// `--now` is the other half of "a human pressed something": the click takes
/// every idle worker shell `aoide spawn` left behind, rather than waiting out
/// the two-day silence the unattended sweep requires
/// (`reap::REAP_SPAWNED_SHELL_STALE_SECS`). It is passed EXPLICITLY here and
/// not inferred — this runs on a detached thread with no tty, so
/// `reap::with_human_gesture`'s own probe would read it as the timer.
///
/// Runs on a DETACHED thread. `session reap` shells out to `hyprctl clients -j`
/// for window liveness; that is normally instant, but a hung compositor query
/// must never tie up this connection's own thread waiting on it (§3b's
/// thread-per-connection accept loop keeps a stuck query from starving anyone
/// else, but there is still no reason to hold it). So this SPAWNS and returns
/// immediately — the same no-block posture [`dispatch_usage_refresh`] takes —
/// and the thread collects the child and audits the outcome via
/// [`classify_recheck`]. The Terminals/Conductor gadgets update themselves off
/// the resulting `sessions.json`/`hooks.json`/`graph.json` writes through their
/// own FileView watches regardless of what this logs; the audit line is for the
/// operator, not to push data. Best-effort throughout: a spawn failure or a
/// classify error is audited, never panicked or propagated.
fn dispatch_recheck_sessions() {
    std::thread::spawn(|| {
        let result = match std::process::Command::new(daemon::bin::core_bin())
            .args(["session", "reap", "--announce", "--now", "--json"])
            .output()
        {
            Ok(out) => classify_recheck(
                out.status.success(),
                &String::from_utf8_lossy(&out.stdout),
                &String::from_utf8_lossy(&out.stderr),
            ),
            Err(e) => Err(format!("spawning `aoide session reap`: {e}")),
        };
        let (event, detail) = match result {
            Ok(msg) => ("recheck", msg),
            Err(e) => ("recheck-failed", e),
        };
        let _ = daemon::audit(
            &daemon::default_audit_log(),
            daemon::Door::Daemon,
            daemon::EventClass::Audit,
            "shellbridge",
            event,
            &detail,
        );
    });
}

// ── session actions (the acknowledged session-menu bridge) ─────────────────

/// A wire value that becomes a bare argv token: a SESSION ID. Stricter than
/// [`safe_action_value`] below — a session id is a bookkeeping key with no
/// legitimate use for whitespace, so any is refused outright.
fn safe_session_id(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars().all(|c| !c.is_whitespace() && !c.is_control())
}

// ── the read-only trace query (`sessiontrace`) ────────────────────────────

/// Now, in epoch milliseconds — the `at` a `sessiontrace` answer carries, so a
/// reader can say WHEN the snapshot it is looking at was read instead of
/// implying it is live. `0` (never a fabricated time) if the clock is before
/// the epoch.
fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Read one child pipe to its end on its OWN thread, forwarding chunks to the
/// wait loop. The thread is never JOINED: it ends by itself at EOF (or when the
/// wait loop drops the receiver), and in the one case where it does not — a
/// DESCENDANT inherited the pipe and holds it open — it holds its
/// [`ReaderSlot`] until it finally exits. That is bounded memory (one 64 KiB
/// chunk) but not bounded COUNT, which is why the slot exists.
///
/// This mirrors the producer pull's own reader
/// (`protocol/src/agents.rs::eidolon_export`): a reader that is joined
/// unconditionally defeats its caller's wall clock, because a descendant
/// holding the pipe makes `read` block forever.
fn pump_pipe<R: std::io::Read>(mut pipe: R, tx: std::sync::mpsc::SyncSender<Vec<u8>>) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    // The wait loop is gone: stop reading and let this thread's
                    // own end of the pipe close with it.
                    break;
                }
            }
        }
    }
}

/// Take everything a pipe has delivered so far into `sink`, without ever
/// blocking: `eof` is set when the sender is gone (the reader thread ended),
/// `over` when the read has passed [`MAX_ANSWER_BYTES`].
fn collect_chunks(
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    sink: &mut Vec<u8>,
    eof: &mut bool,
    over: &mut bool,
) {
    use std::sync::mpsc::TryRecvError;
    loop {
        match rx.try_recv() {
            Ok(chunk) => {
                if sink.len() + chunk.len() > MAX_ANSWER_BYTES as usize {
                    *over = true;
                } else {
                    sink.extend_from_slice(&chunk);
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *eof = true;
                break;
            }
        }
    }
}

/// The reader threads a process may have outstanding at once. A reader thread
/// normally ends the moment its pipe reaches EOF; a descendant that inherited
/// the pipe keeps it open, and the thread then holds its slot until it exits.
/// Above this cap a query is REFUSED without starting a child at all — a bound
/// one timeout per tick could outrun is no bound (the same reasoning as the
/// producer pull's own `PULL_READERS`).
const MAX_TRACE_READERS: usize = 8;

/// Slots free right now.
static TRACE_READERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(MAX_TRACE_READERS);

/// Take one slot from `free` (which counts the slots still FREE) if any is
/// left, without ever waiting. Pure over its own counter so the cap arithmetic
/// is testable without racing the process-wide pool.
fn take_slot(free: &std::sync::atomic::AtomicUsize, _cap: usize) -> bool {
    use std::sync::atomic::Ordering;
    free.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
        if n > 0 { Some(n - 1) } else { None }
    })
    .is_ok()
}

/// Hand one slot back.
fn release_slot(free: &std::sync::atomic::AtomicUsize, cap: usize) {
    use std::sync::atomic::Ordering;
    let _ = free.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| Some((n + 1).min(cap)));
}

/// One outstanding-reader slot, moved INTO the reader thread so it drops when
/// that thread exits and not a moment sooner. Acquire never waits: a refused
/// query is answered honestly and the next tick tries again.
struct ReaderSlot;

impl ReaderSlot {
    fn acquire() -> Option<Self> {
        take_slot(&TRACE_READERS, MAX_TRACE_READERS).then_some(ReaderSlot)
    }
}

impl Drop for ReaderSlot {
    fn drop(&mut self) {
        release_slot(&TRACE_READERS, MAX_TRACE_READERS);
    }
}

/// Run `argv` through the core binary (`daemon::bin::core_bin()`, protocol's
/// sibling resolver — never a bare `"aoide"` relying on PATH alone) with a
/// WALL-CLOCK bound that actually holds. `Ok((exited_ok, stdout, stderr))`, or
/// `Err(reason)` when no complete answer arrived: the child could not be
/// spawned, it outlived `timeout` (killed and REAPED here — `wait`, so no
/// zombie and no live child survives the bound), a DESCENDANT held its pipes
/// open past [`CHILD_DRAIN_GRACE`], or the read passed [`MAX_ANSWER_BYTES`].
/// A partial read is DISCARDED in every one of those cases, never parsed as if
/// it were whole. No shell is involved anywhere: `argv` is passed to
/// `Command::args` whole.
fn run_core_bounded(
    argv: &[String],
    timeout: std::time::Duration,
) -> Result<(bool, String, String), String> {
    // The reader slots come FIRST: no child is started for a read whose pipes
    // cannot be accounted for.
    let Some(out_slot) = ReaderSlot::acquire() else {
        return Err("too many trace reads are still open — not started".to_string());
    };
    let Some(err_slot) = ReaderSlot::acquire() else {
        return Err("too many trace reads are still open — not started".to_string());
    };
    let mut child = std::process::Command::new(daemon::bin::core_bin())
        .args(argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            // The first two argv elements only — always the command path,
            // never a value.
            format!(
                "spawning `{} {}`: {e}",
                argv.first().map(String::as_str).unwrap_or("aoide"),
                argv.get(1).map(String::as_str).unwrap_or("")
            )
        })?;
    let (out_tx, out_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(READ_CHUNKS);
    let (err_tx, err_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(READ_CHUNKS);
    match child.stdout.take() {
        Some(pipe) => {
            std::thread::spawn(move || {
                let _slot = out_slot;
                pump_pipe(pipe, out_tx);
            });
        }
        None => drop(out_tx),
    }
    match child.stderr.take() {
        Some(pipe) => {
            std::thread::spawn(move || {
                let _slot = err_slot;
                pump_pipe(pipe, err_tx);
            });
        }
        None => drop(err_tx),
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let (mut out_eof, mut err_eof) = (false, false);
    let mut over = false;
    let status = loop {
        collect_chunks(&out_rx, &mut out, &mut out_eof, &mut over);
        collect_chunks(&err_rx, &mut err, &mut err_eof, &mut over);
        match child.try_wait() {
            Ok(Some(status)) => {
                // The child is gone. Its pipes normally reach EOF at once; give
                // them a short grace, then DISCARD rather than answer from a
                // partial read (a descendant may still hold them open).
                let grace_end = std::time::Instant::now() + CHILD_DRAIN_GRACE;
                while !(out_eof && err_eof) {
                    let left = grace_end.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    std::thread::sleep(CHILD_POLL.min(left));
                    collect_chunks(&out_rx, &mut out, &mut out_eof, &mut over);
                    collect_chunks(&err_rx, &mut err, &mut err_eof, &mut over);
                }
                if !(out_eof && err_eof) {
                    return Err(format!(
                        "the child exited but its pipes stayed open ({}) — the partial read was \
                         discarded",
                        if out_eof { "stderr held by a descendant" } else { "a descendant holds stdout" }
                    ));
                }
                break status;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "no answer within {}s — the read was abandoned and the child killed",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(CHILD_POLL);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the child could not be waited for — the read was abandoned".to_string());
            }
        }
    };
    if over {
        return Err(format!(
            "the answer passed the {MAX_ANSWER_BYTES}-byte read ceiling — discarded"
        ));
    }
    Ok((
        status.success(),
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    ))
}

/// Does this raw wire line NAMED the trace verb (whatever else was wrong with
/// it)? The one question `handle_conn`'s refusal path asks of an unparseable
/// line: a `sessiontrace` that failed its own gate must be answered, and every
/// other unknown line keeps its plain drop-and-audit.
fn wire_names_sessiontrace(line: &str) -> bool {
    serde_json::from_str::<Value>(line.trim())
        .ok()
        .and_then(|v| v.get("cmd").and_then(Value::as_str).map(str::to_string))
        .as_deref()
        == Some("sessiontrace")
}

/// The ONE JSON line a `sessiontrace` refusal answers with: the CLI's own
/// taught refusal carried verbatim (`message`), its machine `reason` beside it,
/// and the shape that was asked for — never a session id echoed back that the
/// CLI itself did not resolve to.
fn trace_refusal(session_id: &str, clip: &str, lines: usize, reason: &str, message: &str) -> Value {
    json!({
        "ok": false,
        "sessionId": session_id,
        "clip": clip,
        "lines": lines,
        "reason": reason,
        "message": message,
    })
}

/// Dispatch ONE read-only trace query: re-exec the existing CLI
/// (`aoide session trace <id> --tail N --clip <clip> --json`) and shape its
/// own envelope into the ONE line the card reads — `{ok, sessionId, clip,
/// lines, trace, at, steps, stepsOmitted}`, or a refusal
/// ([`trace_refusal`]). Nothing is resolved, folded or re-parsed here: the
/// CLI's resolver, capability refusal and projection ARE the answer, which is
/// why this is a re-exec and not a second reader.
///
/// The child is bounded by [`TRACE_QUERY_TIMEOUT`]; a spawn failure or a
/// timeout is an honest refusal (`no-answer`), never an empty step list — the
/// card must be able to tell "nothing was emitted" from "nobody answered".
fn dispatch_session_trace(session_id: &str, lines: usize, clip: &str) -> Value {
    let argv: Vec<String> = vec![
        "session".to_string(),
        "trace".to_string(),
        session_id.to_string(),
        "--tail".to_string(),
        lines.to_string(),
        "--clip".to_string(),
        clip.to_string(),
        "--json".to_string(),
    ];
    let (stdout, stderr) = match run_core_bounded(&argv, TRACE_QUERY_TIMEOUT) {
        Ok((_exited_ok, stdout, stderr)) => (stdout, stderr),
        Err(reason) => return trace_refusal(session_id, clip, lines, "no-answer", &reason),
    };
    // `--json` puts its envelope on a DIFFERENT stream depending on where the
    // command failed (stdout once dispatched, stderr for a usage error the
    // parser refused first) — the same two-stream read
    // [`session_action_reply`] holds, so a refusal is never mistaken for
    // gibberish.
    let Some((ok, message, data)) = outcome_envelope(&stdout).or_else(|| outcome_envelope(&stderr))
    else {
        let fallback = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            "`aoide session trace` printed no parseable answer".to_string()
        };
        return trace_refusal(session_id, clip, lines, "no-answer", &fallback);
    };
    if !ok {
        let data = data.unwrap_or(Value::Null);
        let reason = data
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("refused");
        // The CLI resolved `<id>` itself; when it named the session it
        // resolved to (an ambiguous or remote target may), that is the
        // identity the answer carries.
        let resolved = data
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or(session_id);
        return trace_refusal(resolved, clip, lines, reason, &message);
    }
    let data = data.unwrap_or(Value::Null);
    let resolved = data
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or(session_id);
    json!({
        "ok": true,
        "sessionId": resolved,
        "clip": clip,
        "lines": lines,
        // The trace PATH, so a reader (or an operator) can see WHICH file the
        // snapshot came off.
        "trace": data.get("trace").cloned().unwrap_or(Value::Null),
        // Read time, epoch ms — the age a reader is shown is this answer's
        // own, never the time some earlier answer was painted.
        "at": epoch_ms(),
        "steps": data.get("steps").cloned().unwrap_or_else(|| json!([])),
        // How many steps the projection's own caps dropped, so a bounded
        // answer says so instead of looking complete.
        "stepsOmitted": data.get("stepsOmitted").cloned().unwrap_or_else(|| json!(0)),
    })
}

/// A wire value that becomes a bare argv token: a PROJECT NAME (`project`'s
/// `project` field, `createproject`/`editproject`'s `name`). Ordinary spaces
/// ARE legal here — "My Project" is a real name, and an argv element is
/// passed to `Command::arg` whole, never through a shell — so only
/// emptiness, a leading `-` (flag-shaped), and control characters are
/// refused. Session ids keep the stricter [`safe_session_id`] above.
fn safe_action_value(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && !s.chars().any(char::is_control)
}

/// A wire value that becomes a filesystem path argument. Whitespace IS legal
/// here — `/home/khoa/My Documents` is a real directory, and an argv element
/// is passed to `Command::arg` whole, never through a shell — so this is a
/// separate rule, not a reuse of the one above. Absolute by requirement,
/// which also settles the leading-dash question: a string starting with `/`
/// can never be read as a flag, so no `-` check is needed here.
fn safe_action_path(s: &str) -> bool {
    s.starts_with('/') && !s.chars().any(char::is_control)
}

/// Which kind of thing a bridge action's plan is about: a session id (the
/// original five session-menu actions, `sessionaction`) or a project name
/// (zero-session project creation/editing, `projectaction`, P-14 M1 §2d).
/// [`run_session_step`]/[`dispatch_session_action`] are generic over this so
/// the two whitelists share one sequencer instead of two copies of it — the
/// five pre-existing session actions run through the exact same code path as
/// before, byte-identical reply and audit shapes included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionSubject<'a> {
    Session(&'a str),
    Project(&'a str),
}

impl<'a> ActionSubject<'a> {
    /// The reply/audit identity key: `"sessionId"` for a session action
    /// (unchanged), `"name"` for a project action (brief §2d).
    fn key(self) -> &'static str {
        match self {
            Self::Session(_) => "sessionId",
            Self::Project(_) => "name",
        }
    }

    /// The raw id/name this subject carries.
    fn value(self) -> &'a str {
        match self {
            Self::Session(s) | Self::Project(s) => s,
        }
    }

    /// The noun used in a generated fallback message (`"session grant done"`
    /// vs `"project create done"`).
    fn noun(self) -> &'static str {
        match self {
            Self::Session(_) => "session",
            Self::Project(_) => "project",
        }
    }

    /// The audit event-name prefix — kept byte-identical to the pre-existing
    /// `"sessionaction"`/`"sessionaction-failed"`/`"sessionaction-partial"`
    /// literals for a session subject.
    fn event_prefix(self) -> &'static str {
        match self {
            Self::Session(_) => "sessionaction",
            Self::Project(_) => "projectaction",
        }
    }

    /// Dispatch to whichever whitelist owns this subject — the ONE call site
    /// both [`dispatch_session_action`] and `parse_command`'s wire gates
    /// consult.
    fn plan(self, action: &str, fields: &Value) -> Option<Vec<Vec<String>>> {
        match self {
            Self::Session(id) => session_action_args(id, action, fields),
            Self::Project(name) => project_action_args(name, action, fields),
        }
    }
}

/// The closed, five-action session-menu whitelist — a PLAN, not one argv.
/// One action is one or two invocations, run in order, stopping at the first
/// failure; `Option<Vec<Vec<String>>>` is deliberate over a single-argv
/// function plus a second for the two-step case: `parse_command`'s own
/// `"sessionaction"` arm gates on `session_action_args(…)?`, which only ever
/// needs *an* `Option`, so this shape keeps that call site compiling
/// unedited AND keeps ONE authority for the whitelist and one call site for
/// the gate (CRAFT: one authority per fact).
///
/// A strict whitelist, not a translator: exactly five actions, anything else
/// is `None`. No generic exec, no arbitrary argv, ever — every element of
/// every returned vector is either a literal from this function or a value
/// that passed [`safe_action_value`]/[`safe_action_path`] below.
///
/// The field shapes are the song-side session menu's own, exactly — they are
/// not the shapes an API designer would pick in isolation, and the bridge
/// matching the UI is the whole point of this slice. Do not "normalize"
/// them.
fn session_action_args(session_id: &str, action: &str, fields: &Value) -> Option<Vec<Vec<String>>> {
    if !safe_session_id(session_id) {
        return None;
    }
    let id = session_id.to_string();
    match action {
        "undying" => {
            let state = fields.get("state").and_then(Value::as_str)?;
            if !matches!(state, "on" | "off") {
                return None;
            }
            Some(vec![vec![
                "session".to_string(),
                "grant".to_string(),
                "undying".to_string(),
                state.to_string(),
                "--id".to_string(),
                id,
            ]])
        }
        "project" => {
            // No `clear` field: an empty STRING is the clear request, which
            // is what the menu's "Automatic from directory" row sends. But
            // `project` must actually BE a JSON string — a missing key or a
            // non-string value (`null`, a number, …) is refused outright,
            // never read as an implicit clear: a malformed wire line must
            // never mutate anything.
            let name = fields.get("project").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                Some(vec![vec![
                    "session".to_string(),
                    "project".to_string(),
                    "--id".to_string(),
                    id,
                    "--clear".to_string(),
                ]])
            } else if safe_action_value(name) {
                Some(vec![vec![
                    "session".to_string(),
                    "project".to_string(),
                    "--id".to_string(),
                    id,
                    "--project".to_string(),
                    name.to_string(),
                ]])
            } else {
                None
            }
        }
        // `fields` is not consulted at all: extra keys are accepted and
        // ignored rather than refused, since rejecting unknown keys would
        // break the menu the first time it grew a field.
        "kill" => Some(vec![vec![
            "session".to_string(),
            "kill".to_string(),
            "--id".to_string(),
            id,
        ]]),
        "createproject" | "editproject" => {
            let name = fields.get("name").and_then(Value::as_str)?.trim();
            if !safe_action_value(name) {
                return None;
            }
            let raw_paths = fields.get("paths").and_then(Value::as_array)?;
            // An empty list is `None`, not an instruction to erase: an
            // "exact replacement" with nothing to replace with is a mistake.
            // No length cap: the list is bounded by the wire's own line
            // length, not by a count guessed in advance.
            if raw_paths.is_empty() {
                return None;
            }
            let mut paths = Vec::with_capacity(raw_paths.len());
            for p in raw_paths {
                // One bad element rejects the whole action; never filter and
                // proceed. A non-string element fails the same way here.
                let p = p.as_str()?;
                if !safe_action_path(p) {
                    return None;
                }
                paths.push(p.to_string());
            }
            if action == "createproject" {
                // Two invocations, order load-bearing: `--new` is slice A's
                // refuse-when-the-name-exists flag — the bridge never
                // pre-checks whether a name is taken, the CLI is the one
                // authority for that.
                let mut add = vec!["project".to_string(), "add".to_string(), name.to_string()];
                add.extend(paths);
                add.push("--new".to_string());
                let assign = vec![
                    "session".to_string(),
                    "project".to_string(),
                    "--id".to_string(),
                    id,
                    "--project".to_string(),
                    name.to_string(),
                ];
                Some(vec![add, assign])
            } else {
                // The roots are replaced exactly; the name is immutable (the
                // lookup key, never a rename) — an unknown name is refused
                // by the CLI, not by the bridge.
                let mut edit = vec!["project".to_string(), "edit".to_string(), name.to_string()];
                edit.extend(paths);
                Some(vec![edit])
            }
        }
        _ => None,
    }
}

/// The project-scoped, zero-session whitelist (`projectaction`, P-14 M1
/// §2d) — mirrors [`session_action_args`] exactly: a PLAN of one or more
/// argv vectors, run in order by the same sequencer ([`dispatch_session_action`]
/// via [`ActionSubject::plan`]), shape-only checks (never state — the CLI
/// stays the one authority for whether a name or host actually exists), one
/// bad element anywhere refuses the whole action. No session id appears
/// anywhere in this function or in anything it builds.
///
/// Wire shape: `{"cmd":"projectaction","action":"create|edit|removehost",
/// "name":"…","paths":[…],"hosts":[{"name":"…","roots":[…]}]}` —
/// `fields` here IS the whole parsed wire object, so `paths`/`hosts` are
/// read at the top level, never nested under a `fields` key.
///
/// - `create`: `project add <name> <paths…> --new`, then per host `project
///   add <name> <roots…> --host <h>` (roots may be empty — membership-only,
///   the same shape `--host` with no roots gives the CLI directly, §2b).
/// - `edit`: `project edit <name> <paths…>`, then per host: non-empty roots
///   → `project edit <name> <roots…> --host <h>` (replace that host's roots
///   exactly); empty roots → `project add <name> --host <h>` (membership
///   only — an `edit --host` with nothing to replace with would wipe an
///   existing host's roots, which is never the intent here).
/// - `removehost`: `project remove <name> --host <h>`, refused unless
///   exactly one host is given.
fn project_action_args(name: &str, action: &str, fields: &Value) -> Option<Vec<Vec<String>>> {
    if !safe_action_value(name) {
        return None;
    }

    // Every host entry, validated shape-only: a real object, a name that
    // passes the same pure check the CLI's own `--host` validator starts
    // with (`valid_node_name` — no I/O, registration is the CLI's job), and
    // roots that are all real paths. One bad element anywhere in this array
    // refuses the whole action, same as `paths` below.
    let hosts: Vec<(String, Vec<String>)> = match fields.get("hosts") {
        None => Vec::new(),
        Some(Value::Array(items)) => {
            let mut hosts = Vec::with_capacity(items.len());
            for item in items {
                let host_name = item.get("name")?.as_str()?;
                if !aoide_storage::node_store::valid_node_name(host_name) {
                    return None;
                }
                let mut roots = Vec::new();
                match item.get("roots") {
                    None => {}
                    Some(Value::Array(raw_roots)) => {
                        for r in raw_roots {
                            let r = r.as_str()?;
                            if !safe_action_path(r) {
                                return None;
                            }
                            roots.push(r.to_string());
                        }
                    }
                    // Present but not an array: the same whole-action
                    // refusal `paths` and `hosts` itself hold below — an
                    // absent `roots` key is the only shape that means
                    // "membership-only".
                    Some(_) => return None,
                }
                hosts.push((host_name.to_string(), roots));
            }
            hosts
        }
        Some(_) => return None,
    };

    match action {
        "removehost" => {
            if hosts.len() != 1 {
                return None;
            }
            let (host_name, _roots) = &hosts[0];
            Some(vec![vec![
                "project".to_string(),
                "remove".to_string(),
                name.to_string(),
                "--host".to_string(),
                host_name.clone(),
            ]])
        }
        "create" | "edit" => {
            let raw_paths = fields.get("paths").and_then(Value::as_array)?;
            // An empty list is `None`, not an instruction to erase — the
            // same rule `createproject`/`editproject` hold above.
            if raw_paths.is_empty() {
                return None;
            }
            let mut paths = Vec::with_capacity(raw_paths.len());
            for p in raw_paths {
                let p = p.as_str()?;
                if !safe_action_path(p) {
                    return None;
                }
                paths.push(p.to_string());
            }

            let mut plan = Vec::with_capacity(1 + hosts.len());
            let mut first = vec![
                "project".to_string(),
                if action == "create" { "add".to_string() } else { "edit".to_string() },
                name.to_string(),
            ];
            first.extend(paths);
            if action == "create" {
                first.push("--new".to_string());
            }
            plan.push(first);

            for (host_name, roots) in hosts {
                let verb = if action == "create" || roots.is_empty() { "add" } else { "edit" };
                let mut step = vec!["project".to_string(), verb.to_string(), name.to_string()];
                step.extend(roots);
                step.push("--host".to_string());
                step.push(host_name);
                plan.push(step);
            }
            Some(plan)
        }
        _ => None,
    }
}

/// The `status`-is-ok flag, `message`, and optional `data` payload of one
/// `--json` envelope, from whichever stream carried it. `data` is `None`
/// when the key is absent — carried through verbatim by
/// [`session_action_reply`] when a step's CLI outcome has one (a `kill`
/// reply's resolved target/pid, say).
fn outcome_envelope(stream: &str) -> Option<(bool, String, Option<Value>)> {
    let v: Value = serde_json::from_str(stream.trim()).ok()?;
    let ok = v.get("status").and_then(Value::as_str)? == "ok";
    let message = v.get("message").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let data = v.get("data").cloned();
    Some((ok, message, data))
}

/// Shape the ONE JSON reply line for an acknowledged bridge action — pure
/// and total, mirroring [`classify_recheck`]'s own rule: success is the real
/// output, not the exit status. `--json` puts its envelope on a DIFFERENT
/// stream depending on where the command failed: a dispatched command
/// prints it on stdout, but a usage error the parser refused BEFORE dispatch
/// prints it on stderr with an empty stdout (`protocol/src/door.rs:800-814`)
/// — exactly the path `createproject`/`editproject` take until slice A
/// lands, so reading stdout alone would hand QML a raw JSON blob as its
/// "message". `subject` is generic over `sessionaction`/`projectaction`
/// (P-14 M1 §2d) — a session subject reproduces every field byte-identical
/// to before that split.
fn session_action_reply(
    subject: ActionSubject,
    action: &str,
    exited_ok: bool,
    stdout: &str,
    stderr: &str,
) -> Value {
    let (ok, message, data) = match outcome_envelope(stdout).or_else(|| outcome_envelope(stderr)) {
        Some((status_ok, msg, data)) => {
            let ok = exited_ok && status_ok;
            let noun = subject.noun();
            let message = if !msg.is_empty() {
                msg
            } else if ok {
                format!("{noun} {action} done")
            } else {
                format!("{noun} {action} failed")
            };
            (ok, message, data)
        }
        None => {
            let stderr = stderr.trim();
            let stdout = stdout.trim();
            let message = if !stderr.is_empty() {
                stderr.to_string()
            } else if !stdout.is_empty() {
                stdout.to_string()
            } else {
                format!("`aoide {action}` printed no parseable envelope")
            };
            (false, message, None)
        }
    };
    let mut reply = json!({
        "ok": ok,
        "message": message,
        "action": action,
    });
    reply[subject.key()] = json!(subject.value());
    if let Some(data) = data {
        reply["data"] = data;
    }
    reply
}

/// One step, spawned: exec the core `aoide` binary
/// (`daemon::bin::core_bin()`, protocol's sibling resolver — never a bare
/// `"aoide"` relying on PATH alone) with `argv` plus `--json`, the exact
/// pattern `dispatch_usage_refresh`/`dispatch_recheck_sessions` already use.
/// This call is NOT inside `with_stage_lock` — see `protocol::bin`'s module
/// doc for why that would matter if it ever were.
fn run_session_step(subject: ActionSubject, action: &str, argv: &[String]) -> Value {
    match std::process::Command::new(daemon::bin::core_bin())
        .args(argv)
        .arg("--json")
        .output()
    {
        Ok(out) => session_action_reply(
            subject,
            action,
            out.status.success(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        ),
        // The first TWO argv elements only — always the command path, never
        // a value — same discipline every audit line here holds.
        Err(e) => {
            let mut reply = json!({
                "ok": false,
                "message": format!("spawning `aoide {} {}`: {e}", argv[0], argv[1]),
                "action": action,
            });
            reply[subject.key()] = json!(subject.value());
            reply
        }
    }
}

/// One audit line per ACTION, never per step: detail is the action name and
/// the status, and nothing else — never a project name, a path, or a
/// session id. This deliberately departs from `dispatch_usage_refresh`/
/// `dispatch_recheck_sessions`, which reuse the CLI's own `message`: those
/// two commands take no arguments, this one does, and a `message` could grow
/// to quote one — house rule, an audit line never carries argument values.
/// `partial` gets its own event name so an operator scanning the log can see
/// a half-applied action without reading the message. The dispatched
/// commands are separately audited by the child processes' own `dispatch`
/// inside `aoided`; this line records only that the desk asked. `subject`'s
/// noun (`"session"`/`"project"`) is the only thing that varies from the
/// original `"session action …"` wording, so a session subject's message is
/// byte-identical to before the `projectaction` split.
fn audit_bridge_action(subject: ActionSubject, action: &str, event: &str, outcome: &str) {
    let _ = daemon::audit(
        &daemon::default_audit_log(),
        daemon::Door::Daemon,
        daemon::EventClass::Audit,
        "shellbridge",
        event,
        &format!("{} action {action}: {outcome}", subject.noun()),
    );
}

/// The sequencer: run one acknowledged bridge action's whole plan, on the
/// CALLER's thread — `handle_conn` is what detaches it. Rebuilds the plan
/// through [`ActionSubject::plan`] — [`session_action_args`] or
/// [`project_action_args`], the SAME authority `parse_command`'s wire gate
/// already consulted; a `None` here is unreachable through the wire but
/// total by construction, never a panic. Steps run in order, stopping at the
/// first failure: a failing FIRST step returns that reply unchanged (nothing
/// ran, nothing changed); a failing LATER step means an earlier step already
/// changed the world, so the reply says so honestly (`partial: true`) rather
/// than rolling back — deleting a project the operator may already want, to
/// tidy up a failure they can see and fix in one click, is worse than the
/// partial state. No retries, no queueing: one spawn per step, one reply per
/// action. Generic over [`ActionSubject`] (P-14 M1 §2d) — a session subject
/// runs the exact same five actions, byte-identical replies and audit
/// lines, as before the split.
fn dispatch_session_action(subject: ActionSubject, action: &str, fields: &Value) -> Value {
    let Some(plan) = subject.plan(action, fields) else {
        let mut reply = json!({
            "ok": false,
            "message": format!("unsupported {} action", subject.noun()),
            "action": action,
        });
        reply[subject.key()] = json!(subject.value());
        return reply;
    };

    let mut last = Value::Null;
    for (i, argv) in plan.iter().enumerate() {
        let reply = run_session_step(subject, action, argv);
        let step_ok = reply.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !step_ok {
            if i == 0 {
                audit_bridge_action(
                    subject,
                    action,
                    &format!("{}-failed", subject.event_prefix()),
                    "failed",
                );
                return reply;
            }
            let cli_message = reply.get("message").and_then(Value::as_str).unwrap_or("");
            audit_bridge_action(
                subject,
                action,
                &format!("{}-partial", subject.event_prefix()),
                "partial",
            );
            let message = match subject {
                // Take the created name from the PLAN, never re-reading
                // `fields`, so the plan stays the single authority for what
                // actually ran. Wording unchanged from before the split.
                ActionSubject::Session(_) => {
                    let name = plan[0].get(2).map(String::as_str).unwrap_or("");
                    format!("project {name} created; assigning the session failed: {cli_message}")
                }
                ActionSubject::Project(name) => {
                    format!(
                        "project {name} partially updated; step {} failed: {cli_message}",
                        i + 1
                    )
                }
            };
            let mut reply = json!({
                "ok": false,
                "message": message,
                "partial": true,
                "action": action,
            });
            reply[subject.key()] = json!(subject.value());
            return reply;
        }
        last = reply;
    }
    audit_bridge_action(subject, action, subject.event_prefix(), "ok");
    last
}

/// Run shellbridge: seed the `sessions.json`/`hooks.json` stage files (v0
/// shapes) atomically, then bind the unix socket and serve commands forever.
/// Only a fatal bind failure returns (with an error document dispatch reports);
/// on success this never returns — the systemd unit is `Type=simple` and stays
/// up on the blocking accept loop.
pub fn run() -> serde_json::Value {
    let sock = socket_path();
    let stage = conducting_stage_dir();

    // Seed the two stage files with their documented shapes (empty registries).
    let sessions = json!({
        "schemaVersion": "0",
        // records: { sessionId, agent, windowAddress, cwd, state, startedAt,
        //            parentSessionId? (optional spawned-by edge, `aoide graph link`) }
        "sessions": []
    });
    let hooks = json!({
        "schemaVersion": "0",
        // records: { sessionId, phase, updatedAt }
        "hooks": []
    });

    // Seed each registry ONLY when absent/corrupt — a populated roster is
    // preserved across this restart (see [`seed_if_absent`]). The unconditional
    // re-seed this replaced was the transient-drop bug: every shellbridge restart
    // (a nixos switch / compositor restart cascades through
    // graphical-session.target) wiped all live sessions to `[]`.
    let mut wrote: Vec<String> = Vec::new();
    let s_path = stage.join("sessions.json");
    let h_path = stage.join("hooks.json");
    if let Some(p) =
        seed_if_absent(&s_path, &serde_json::to_string_pretty(&sessions).unwrap(), "sessions")
    {
        wrote.push(p);
    }
    if let Some(p) =
        seed_if_absent(&h_path, &serde_json::to_string_pretty(&hooks).unwrap(), "hooks")
    {
        wrote.push(p);
    }

    // Bind the socket. `RuntimeDirectory=aoide` on the unit creates the parent
    // dir; a stale socket from an unclean shutdown would make bind fail with
    // EADDRINUSE, so remove it first (the path is single-owner per user).
    if let Some(parent) = sock.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&sock);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(e) => {
            let _ = daemon::audit(
                &daemon::default_audit_log(),
                daemon::Door::Daemon,
                daemon::EventClass::Audit,
                "shellbridge",
                "bind-failed",
                &format!("could not bind {}: {e}", sock.display()),
            );
            return json!({
                "process": "shellbridge",
                "state": "error",
                "error": format!("bind {}: {e}", sock.display()),
                "socket": sock.to_string_lossy(),
                "stageDir": stage.to_string_lossy(),
                "wrote": wrote,
            });
        }
    };

    let _ = daemon::audit(
        &daemon::default_audit_log(),
        daemon::Door::Daemon,
        daemon::EventClass::Audit,
        "shellbridge",
        "started",
        &format!("shellbridge online; listening on {}", sock.display()),
    );

    // Spawn the Hyprland window→session event listener on a background thread:
    // it is the AUTHORITATIVE, creation-time source of each session's
    // `windowAddress` (concepts/Terminal-Commander), keeping the widget's
    // click-to-jump reliable instead of depending on the lazy hook-time backfill.
    // It runs concurrently with — and can never block or kill — the accept loop,
    // and degrades to a no-op off-Hyprland (logs once, returns).
    std::thread::spawn(crate::graph::run_hypr_window_listener);

    // Serve forever. `serve` never returns and never panics on client input.
    serve(&listener);

    // Only reached if `incoming()` ends (listener closed) — treat as a clean
    // stop so systemd's `Restart=on-failure` can bring us back.
    json!({
        "process": "shellbridge",
        "state": "stopped",
        "socket": sock.to_string_lossy(),
        "stageDir": stage.to_string_lossy(),
        "wrote": wrote,
    })
}

// ── the herald ledger ─────────────────────────────────────────────────────

/// Read `stage/herald.json`, apply `f`, write it back atomically.
///
/// Every ledger mutation goes through here. §3b made the accept loop
/// thread-per-connection (it used to be one connection at a time, which is
/// what let this read-modify-write get away with no lock of its own), so two
/// herald pushes landing on the same instant are now a real race; the
/// read-modify-write is wrapped in `with_stage_lock` to close it. Never call
/// this from inside a CLI re-exec path (`dispatch_session_action`,
/// `dispatch_recheck_sessions`, `dispatch_usage_refresh`,
/// `dispatch_rice_mode_toggle`) — holding the stage lock across a blocking
/// child-process wait is a deadlock waiting to happen; none of those paths
/// touch the herald ledger today, and that must stay true. A missing or
/// corrupt file is not an error: it reads as an empty ledger and is
/// rewritten whole, so a truncated write can never wedge notifications shut.
fn edit_ledger<F, T>(f: F) -> std::io::Result<T>
where
    F: FnOnce(&mut Vec<crate::herald::Notification>) -> T,
{
    aoide_storage::fs::with_stage_lock(|| {
        let path = crate::herald::herald_path();
        let mut file: crate::herald::HeraldFile = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let out = f(&mut file.notifications);
        file.schema_version = crate::herald::HERALD_SCHEMA.to_string();
        let text = serde_json::to_string_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        aoide_storage::fs::atomic_write(&path, &format!("{text}\n"))?;
        Ok(out)
    })
}

/// File one notification. Sender text is DATA: it is deserialised into the
/// record shape and written back out, never parsed or interpreted.
fn dispatch_herald_push(notification: Value) {
    let notif: crate::herald::Notification = match serde_json::from_value(notification) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[aoide/shellbridge] herald push with a malformed record: {e}");
            return;
        }
    };
    let id = notif.id.clone();
    if let Err(e) = edit_ledger(move |list| {
        let taken = std::mem::take(list);
        *list = crate::herald::apply_push(taken, notif);
    }) {
        eprintln!("[aoide/shellbridge] could not write the herald ledger: {e}");
    } else {
        let _ = daemon::audit(
            &daemon::default_audit_log(),
            daemon::Door::Daemon,
            daemon::EventClass::Audit,
            "shellbridge",
            "herald.push",
            &format!("filed notification {id}"),
        );
    }
}

/// Drop one card from the ledger (or all of them on `*`).
fn dispatch_herald_dismiss(id: String) {
    let res = edit_ledger(|list| {
        if id == "*" {
            let n = list.len();
            list.clear();
            n > 0
        } else {
            crate::herald::apply_dismiss(list, &id)
        }
    });
    if let Err(e) = res {
        eprintln!("[aoide/shellbridge] could not write the herald ledger: {e}");
    }
}

/// Type a summons verdict into the waiting session, then drop the card.
///
/// Runs on a DETACHED thread and audits its own outcome: the injection walks
/// the stage files and writes to the session's control socket, which must never
/// block the accept loop — the same posture `dispatch_usage_refresh` and
/// `dispatch_recheck_sessions` already take. The `still awaiting` guard lives
/// inside `graph permit`'s answer path, not here, so a human who answered in
/// the terminal while the card stood is never typed over.
fn dispatch_herald_verdict(id: String, verdict: String) {
    std::thread::spawn(move || {
        let outcome = crate::graph::answer_summons(&id, &verdict);
        let _ = daemon::audit(
            &daemon::default_audit_log(),
            daemon::Door::Daemon,
            daemon::EventClass::Audit,
            "shellbridge",
            "herald.verdict",
            &outcome.message,
        );
        // The card comes down either way — an answered summons is answered
        // even if the session had already moved on and nothing was typed.
        // NOTE the id swap: the wire carries the SESSION id (that is what a
        // verdict is addressed to), while the ledger entry is keyed by the
        // CARD id. `summons_card_id` is the one place that mapping lives.
        dispatch_herald_dismiss(crate::graph::summons_card_id(&id));
    });
}

/// Send one newline-delimited JSON line to the running shellbridge.
///
/// The client half of this module: `aoide herald push` and `graph permit` reach
/// the daemon through here rather than writing `stage/herald.json` themselves,
/// so the daemon stays the single writer.
pub fn send_line(line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = UnixStream::connect(socket_path())?;
    stream.write_all(line.trim_end().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

/// LANE IDENTITY P-ID3 (G7) — the shellbridge socket's CROSS-UID floor, pure
/// and unit-tested without a real different-uid connection (same shape
/// `aoide_secrets::broker::admin_gate` already holds: a plain `Option<u32>`
/// in, an `Option<String>` refusal reason out). `None` (admitted) only when
/// the peer's kernel-attested uid equals `my_euid` — this process's OWN
/// euid, since shellbridge always runs as the operator's own uid, the SAME
/// uid every legitimate connector already runs as (the QML herald/bar
/// widgets, `aoide herald push` off dunst's script hook, `session permit`
/// raising its own summons — every one of them same-uid, none of them a
/// DIFFERENT uid). An unidentified peer (`SO_PEERCRED` read failed) is
/// refused the same fail-closed way a mismatched uid is, never treated as
/// benign.
///
/// **This closes a CROSS-uid gap only — it does NOT stop a same-uid
/// attacker.** Under OQ1-A (LANE IDENTITY's thesis) every legitimate
/// connector above already shares this exact uid with anything hostile a
/// prompt-injected agent could run, so a same-uid process forging
/// `{"cmd":"heraldverdict",...}` is an OQ1-A-INHERENT residual this floor
/// does not close — see `CONTRACTS.md`'s identity section for the honest
/// statement of what remains open on the verdict door specifically.
fn cross_uid_gate(peer: Option<crate::graph::identity::PeerCred>, my_euid: u32) -> Option<String> {
    match peer {
        Some(p) if p.uid == my_euid => None,
        Some(p) => Some(format!(
            "shellbridge connection refused: peer uid {} does not match this process's own uid {my_euid}",
            p.uid
        )),
        None => Some(
            "shellbridge connection refused: peer uid could not be determined (SO_PEERCRED read failed)"
                .to_string(),
        ),
    }
}

fn serve(listener: &UnixListener) {
    // SAFETY: `geteuid()` takes no arguments and cannot fail — the same
    // call `identity.rs`'s own test makes.
    let my_euid = unsafe { libc::geteuid() };
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let peer = crate::graph::identity::peer_cred(&stream);
                if let Some(reason) = cross_uid_gate(peer, my_euid) {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "peercred-refused",
                        &reason,
                    );
                    continue; // Dropped, unconditionally — never reaches `handle_conn`.
                }
                std::thread::spawn(move || handle_conn(stream));
            }
            Err(e) => eprintln!("[aoide/shellbridge] accept error (continuing): {e}"),
        }
    }
}

/// Handle ONE client connection, on its OWN thread (`serve` spawns one per
/// accepted connection): read newline-delimited JSON lines and act on each.
/// Containment is now per-connection, not per-process — a panic here dies
/// with its own thread instead of unwinding into `serve`, and a slow or idle
/// connection can no longer starve any other. A read error (dropped
/// connection) ends only THIS connection, an unparseable/unknown line is
/// audited and skipped, and a focus dispatch failure is logged. No read
/// timeout is set on the accepted stream: the shared QML socket legitimately
/// idles between human gestures, and idleness was never the fault here —
/// serial accept was.
fn handle_conn(stream: UnixStream) {
    let mut reply = stream.try_clone().ok();
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            // Dropped connection / read error: done with this connection only.
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        match parse_command(&line) {
            Some(BridgeCommand::SessionAction { session_id, action, fields }) => {
                // The connection is dedicated to this one action and its reply clone is
                // the only channel the acknowledgement has. Without it the action would
                // run unacknowledged — a kill firing while the caller is told nothing
                // happened — so it does not run at all.
                let Some(mut reply) = reply.take() else {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "sessionaction-noreply",
                        &format!("session action {action}: no reply channel; not dispatched"),
                    );
                    return;
                };
                std::thread::spawn(move || {
                    use std::io::Write;
                    let subject = ActionSubject::Session(&session_id);
                    let _ = writeln!(reply, "{}", dispatch_session_action(subject, &action, &fields));
                });
                return;
            }
            Some(BridgeCommand::ProjectAction { name, action, fields }) => {
                // Same one-shot-connection discipline as `SessionAction`
                // above: no reply channel means the action does not run.
                let Some(mut reply) = reply.take() else {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "projectaction-noreply",
                        &format!("project action {action}: no reply channel; not dispatched"),
                    );
                    return;
                };
                std::thread::spawn(move || {
                    use std::io::Write;
                    let subject = ActionSubject::Project(&name);
                    let _ = writeln!(reply, "{}", dispatch_session_action(subject, &action, &fields));
                });
                return;
            }
            Some(BridgeCommand::Focus { address }) => match crate::graph::focus_window(&address) {
                Ok(()) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "focus",
                        &format!("focused window {address}"),
                    );
                }
                Err(e) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "focus-failed",
                        &format!("{} ({}): {}", address, e.reason, e.message),
                    );
                }
            },
            Some(BridgeCommand::FocusSession { session_id }) => {
                match crate::graph::focus_session(&session_id) {
                    Ok(()) => {
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "focus",
                            &format!("focused session {session_id}"),
                        );
                    }
                    Err(e) => {
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "focus-failed",
                            &format!("session {} ({}): {}", session_id, e.reason, e.message),
                        );
                    }
                }
            }
            Some(BridgeCommand::Power { action }) => match dispatch_power(action) {
                Ok(()) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "power",
                        &format!("spawned power action {}", action.as_str()),
                    );
                }
                Err(e) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "power-failed",
                        &format!("power action {}: {e}", action.as_str()),
                    );
                }
            },
            Some(BridgeCommand::ToggleRiceMode) => match dispatch_rice_mode_toggle() {
                Ok(message) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "ricemode",
                        &message,
                    );
                }
                Err(e) => {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "ricemode-failed",
                        &e,
                    );
                }
            },
            // Fire-and-forget: dispatch_usage_refresh audits its OWN outcome from
            // a detached thread (the re-exec's live fetch is ≤15s, too long to
            // block the accept loop inline — see the function). Nothing to match
            // on here, unlike the arms above.
            Some(BridgeCommand::RefreshUsage) => dispatch_usage_refresh(),
            // Fire-and-forget, same posture: dispatch_recheck_sessions re-execs
            // `aoide session reap` on a detached thread and audits its own outcome
            // (the sweep shells out to hyprctl, which must never block the accept
            // loop). The gadgets refresh off the resulting stage writes.
            Some(BridgeCommand::RecheckSessions) => dispatch_recheck_sessions(),
            // The ledger writes are inline: they are a read-modify-write of one
            // small local file. Concurrent pushes from different connections'
            // threads (§3b) are serialised by `edit_ledger`'s own
            // `with_stage_lock`, not by the accept loop.
            Some(BridgeCommand::HeraldPush { notification }) => {
                dispatch_herald_push(*notification)
            }
            Some(BridgeCommand::HeraldDismiss { id }) => dispatch_herald_dismiss(id),
            // Detached, like the other two fire-and-forget arms: this one walks
            // the stage files and writes to a session's control socket.
            Some(BridgeCommand::HeraldVerdict { id, verdict }) => {
                dispatch_herald_verdict(id, verdict)
            }
            // The read-only trace query: answered on this connection like
            // `sessionaction` (the caller reads one line back), and run on its
            // OWN thread because it re-execs the CLI — a bounded child
            // ([`run_core_bounded`]) must never sit between the accept loop and
            // the next connection. No reply channel means the query does not
            // run at all, the same discipline the two action arms hold.
            Some(BridgeCommand::SessionTrace { session_id, lines, clip }) => {
                let Some(mut reply) = reply.take() else {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "sessiontrace-noreply",
                        "trace query: no reply channel; not dispatched",
                    );
                    return;
                };
                // A polled READ answers on the connection and audits only its
                // REFUSALS: one line per second per card is not a human
                // gesture, and the log's job is to hold what was refused (and
                // why), never a tick-by-tick echo of what was looked at.
                std::thread::spawn(move || {
                    use std::io::Write;
                    let answer = dispatch_session_trace(&session_id, lines, &clip);
                    if answer.get("ok").and_then(Value::as_bool) != Some(true) {
                        let reason = answer
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("failed");
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "sessiontrace-refused",
                            &format!("trace query clip={clip} lines={lines} refused: {reason}"),
                        );
                    }
                    let _ = writeln!(reply, "{answer}");
                });
                return;
            }
            None => {
                let _ = daemon::audit(
                    &daemon::default_audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "unparseable",
                    &format!("dropped one unparseable or unknown command line ({} bytes)", line.len()),
                );
                // `sessiontrace` is the one verb whose caller is PARKED waiting
                // for an answer, so a malformed one must not leave it there: a
                // line that names this verb and fails its own gate is answered
                // with the refusal instead of silence (the widget's own client
                // normalises what it sends, so this is the door's honesty, not
                // its fast path).
                if wire_names_sessiontrace(&line) {
                    if let Some(mut reply) = reply.take() {
                        let answer = trace_refusal(
                            "",
                            "",
                            0,
                            "bad-request",
                            "a sessiontrace line takes a non-blank sessionId, a positive integer \
                             `lines` (clamped to 40 from above), and `clip` line|detail",
                        );
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "sessiontrace-refused",
                            "trace query refused: bad-request",
                        );
                        std::thread::spawn(move || {
                            use std::io::Write;
                            let _ = writeln!(reply, "{answer}");
                        });
                        return;
                    }
                }
            }
        }
    }
}

// ── Tests (the socket-command wire contract) ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── cross_uid_gate (LANE IDENTITY P-ID3, G7) ────────────────────────

    #[test]
    fn cross_uid_gate_admits_a_matching_euid() {
        assert_eq!(
            cross_uid_gate(Some(crate::graph::identity::PeerCred { uid: 1000, pid: 42 }), 1000),
            None
        );
    }

    #[test]
    fn cross_uid_gate_refuses_a_mismatched_uid() {
        assert!(cross_uid_gate(Some(crate::graph::identity::PeerCred { uid: 1001, pid: 42 }), 1000).is_some());
    }

    #[test]
    fn cross_uid_gate_refuses_an_unidentified_peer() {
        // Fail-closed, never a benign default — the same posture
        // `admin_gate` holds for a `SO_PEERCRED` read that failed.
        assert!(cross_uid_gate(None, 1000).is_some());
    }

    /// End-to-end against a REAL socketpair (mirrors `identity.rs`'s own
    /// `peer_cred_on_a_scratch_socketpair_matches_this_processs_own_identity`):
    /// a connection entirely local to this process reports THIS process's
    /// own euid, which `cross_uid_gate` then admits — proving the floor
    /// does not refuse the legitimate same-uid caller (the desktop QML, the
    /// dunst hook, `session permit`'s own raise — every real connector Phase
    /// 0 identified) it must never touch.
    #[test]
    fn a_real_same_process_socketpair_is_admitted() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().expect("socketpair");
        let peer = crate::graph::identity::peer_cred(&a);
        let my_euid = unsafe { libc::geteuid() };
        assert_eq!(cross_uid_gate(peer, my_euid), None);
    }

    #[test]
    fn parse_command_accepts_a_valid_focuswindow() {
        assert_eq!(
            parse_command(r#"{"cmd":"focuswindow","address":"0x55aabb"}"#),
            Some(BridgeCommand::Focus {
                address: "0x55aabb".to_string()
            })
        );
        // Trailing newline / surrounding whitespace is tolerated (wire lines
        // arrive newline-terminated) and the address is trimmed.
        assert_eq!(
            parse_command("  {\"cmd\":\"focuswindow\",\"address\":\" 0xABC \"}\n"),
            Some(BridgeCommand::Focus {
                address: "0xABC".to_string()
            })
        );
    }

    #[test]
    fn parse_command_accepts_a_valid_focussession() {
        assert_eq!(
            parse_command(r#"{"cmd":"focussession","sessionId":"conduct-1-2"}"#),
            Some(BridgeCommand::FocusSession {
                session_id: "conduct-1-2".to_string()
            })
        );
        // Trimmed like focuswindow's address.
        assert_eq!(
            parse_command("{\"cmd\":\"focussession\",\"sessionId\":\" abc \"}\n"),
            Some(BridgeCommand::FocusSession {
                session_id: "abc".to_string()
            })
        );
        // Empty/absent sessionId → None (never dispatch a blank session jump).
        assert_eq!(parse_command(r#"{"cmd":"focussession","sessionId":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"focussession"}"#), None);
    }

    #[test]
    fn parse_command_accepts_a_herald_push_and_rejects_an_unfilable_one() {
        let line = r#"{"cmd":"heraldpush","notification":{"id":"7","summary":"hi"}}"#;
        match parse_command(line) {
            Some(BridgeCommand::HeraldPush { notification }) => {
                assert_eq!(notification.get("id").unwrap(), "7");
            }
            other => panic!("expected a herald push, got {other:?}"),
        }
        // A record with no usable id could neither replace its predecessor nor
        // be dismissed later, so it never reaches the ledger.
        assert_eq!(
            parse_command(r#"{"cmd":"heraldpush","notification":{"summary":"hi"}}"#),
            None
        );
        assert_eq!(
            parse_command(r#"{"cmd":"heraldpush","notification":{"id":"  "}}"#),
            None
        );
        assert_eq!(parse_command(r#"{"cmd":"heraldpush"}"#), None);
    }

    #[test]
    fn a_verdict_is_only_ever_one_of_the_two_real_answers() {
        assert_eq!(
            parse_command(r#"{"cmd":"heraldverdict","id":"s1","verdict":"approve"}"#),
            Some(BridgeCommand::HeraldVerdict {
                id: "s1".to_string(),
                verdict: "approve".to_string(),
            })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"heraldverdict","id":" s1 ","verdict":" deny "}"#),
            Some(BridgeCommand::HeraldVerdict {
                id: "s1".to_string(),
                verdict: "deny".to_string(),
            })
        );
        // A permission gate has no safe direction to default to, so anything
        // that is not exactly one of the two answers is dropped at the wire
        // rather than resolved. This is the defect the whole herald retcon
        // exists to kill — the old daemon-drawn card could not tell a click on
        // "deny" from a click anywhere else, and approved.
        for bad in [
            r#"{"cmd":"heraldverdict","id":"s1","verdict":"approved"}"#,
            r#"{"cmd":"heraldverdict","id":"s1","verdict":"APPROVE"}"#,
            r#"{"cmd":"heraldverdict","id":"s1","verdict":"yes"}"#,
            r#"{"cmd":"heraldverdict","id":"s1","verdict":""}"#,
            r#"{"cmd":"heraldverdict","id":"","verdict":"approve"}"#,
            r#"{"cmd":"heraldverdict","id":"s1"}"#,
        ] {
            assert_eq!(parse_command(bad), None, "{bad}");
        }
    }

    #[test]
    fn parse_command_accepts_a_herald_dismiss() {
        assert_eq!(
            parse_command(r#"{"cmd":"heralddismiss","id":"7"}"#),
            Some(BridgeCommand::HeraldDismiss { id: "7".to_string() })
        );
        // `*` is the clear-the-desk form.
        assert_eq!(
            parse_command(r#"{"cmd":"heralddismiss","id":"*"}"#),
            Some(BridgeCommand::HeraldDismiss { id: "*".to_string() })
        );
        assert_eq!(parse_command(r#"{"cmd":"heralddismiss","id":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"heralddismiss"}"#), None);
    }

    #[test]
    fn parse_command_accepts_every_valid_power_action() {
        let cases = [
            ("lock", PowerAction::Lock),
            ("logout", PowerAction::Logout),
            ("suspend", PowerAction::Suspend),
            ("hibernate", PowerAction::Hibernate),
            ("reboot", PowerAction::Reboot),
            ("shutdown", PowerAction::Shutdown),
        ];
        for (wire, want) in cases {
            assert_eq!(
                parse_command(&format!(r#"{{"cmd":"power","action":"{wire}"}}"#)),
                Some(BridgeCommand::Power { action: want }),
                "power action {wire} must parse"
            );
        }
        // Whitespace around the action is trimmed (wire lines arrive
        // newline-terminated), same tolerance as focuswindow's address.
        assert_eq!(
            parse_command("  {\"cmd\":\"power\",\"action\":\" lock \"}\n"),
            Some(BridgeCommand::Power {
                action: PowerAction::Lock
            })
        );
    }

    #[test]
    fn parse_command_rejects_bad_power_actions() {
        // Unknown action → None (a typo must never reach a dispatch).
        assert_eq!(parse_command(r#"{"cmd":"power","action":"explode"}"#), None);
        // Case matters — the wire contract is lowercase.
        assert_eq!(parse_command(r#"{"cmd":"power","action":"Reboot"}"#), None);
        // Empty / absent / non-string action → None.
        assert_eq!(parse_command(r#"{"cmd":"power","action":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"power"}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"power","action":42}"#), None);
    }

    #[test]
    fn parse_command_accepts_a_valid_ricemode() {
        assert_eq!(
            parse_command(r#"{"cmd":"ricemode"}"#),
            Some(BridgeCommand::ToggleRiceMode)
        );
        // No payload is expected or read — extra fields are simply ignored.
        assert_eq!(
            parse_command("  {\"cmd\":\"ricemode\"}\n"),
            Some(BridgeCommand::ToggleRiceMode)
        );
    }

    #[test]
    fn parse_command_accepts_a_valid_refreshusage() {
        assert_eq!(
            parse_command(r#"{"cmd":"refreshusage"}"#),
            Some(BridgeCommand::RefreshUsage)
        );
        // No payload is expected or read — extra fields are simply ignored,
        // and surrounding whitespace/newline is tolerated like every command.
        assert_eq!(
            parse_command("  {\"cmd\":\"refreshusage\"}\n"),
            Some(BridgeCommand::RefreshUsage)
        );
        // A typo is NOT this command (the gatekeeper rule — an unparsed command goes
        // nowhere, the `{cmd:"powermenu"}` scar).
        assert_eq!(parse_command(r#"{"cmd":"refresh"}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"usagerefresh"}"#), None);
    }

    // ── the read-only trace query (B2.2 / B3.2) ─────────────────────────

    /// `sessiontrace` with and without `lines`; the id trimmed; `lines` clamped
    /// from above (never refused for being too big); `clip` defaulted to
    /// `line`.
    #[test]
    fn parse_command_accepts_a_sessiontrace_with_and_without_lines() {
        assert_eq!(
            parse_command(r#"{"cmd":"sessiontrace","sessionId":"Aoide-7d23"}"#),
            Some(BridgeCommand::SessionTrace {
                session_id: "Aoide-7d23".to_string(),
                lines: TRACE_LINES_DEFAULT,
                clip: "line".to_string(),
            })
        );
        // Trimmed like `focussession`'s id, and an explicit clip is carried.
        assert_eq!(
            parse_command("{\"cmd\":\"sessiontrace\",\"sessionId\":\" Aoide-7d23 \",\"lines\":8,\"clip\":\"detail\"}\n"),
            Some(BridgeCommand::SessionTrace {
                session_id: "Aoide-7d23".to_string(),
                lines: 8,
                clip: "detail".to_string(),
            })
        );
        // An explicit `null`/blank-shaped absence is the default, not a refusal.
        assert_eq!(
            parse_command(r#"{"cmd":"sessiontrace","sessionId":"s1","lines":null,"clip":null}"#),
            Some(BridgeCommand::SessionTrace {
                session_id: "s1".to_string(),
                lines: TRACE_LINES_DEFAULT,
                clip: "line".to_string(),
            })
        );
        // Clamped from ABOVE: a caller asking for more gets the bound.
        assert_eq!(
            parse_command(r#"{"cmd":"sessiontrace","sessionId":"s1","lines":100000}"#),
            Some(BridgeCommand::SessionTrace {
                session_id: "s1".to_string(),
                lines: TRACE_LINES_MAX,
                clip: "line".to_string(),
            })
        );
        // `lines` at the cap is itself.
        assert_eq!(
            parse_command(r#"{"cmd":"sessiontrace","sessionId":"s1","lines":40}"#),
            Some(BridgeCommand::SessionTrace {
                session_id: "s1".to_string(),
                lines: TRACE_LINES_MAX,
                clip: "line".to_string(),
            })
        );
    }

    /// A blank/non-id-shaped id, a nonsense `lines`, or an unknown `clip` is
    /// refused (never defaulted, never silently widened), exactly like the
    /// wire's other closed sets.
    #[test]
    fn parse_command_refuses_a_bad_sessiontrace() {
        // A line that names the verb but fails its own gate is REFUSED by the
        // parser (never defaulted), and `handle_conn` still answers it — the
        // raw-line question that decision is made on is separate and pure.
        assert!(wire_names_sessiontrace(
            r#"{"cmd":"sessiontrace","sessionId":"s1","clip":"detials"}"#
        ));
        assert!(wire_names_sessiontrace(r#"{"cmd":"sessiontrace"}"#));
        assert!(!wire_names_sessiontrace(r#"{"cmd":"focussession"}"#));
        assert!(!wire_names_sessiontrace("{ not json"));
        for line in [
            r#"{"cmd":"sessiontrace"}"#,
            r#"{"cmd":"sessiontrace","sessionId":""}"#,
            r#"{"cmd":"sessiontrace","sessionId":"   "}"#,
            r#"{"cmd":"sessiontrace","sessionId":"a b"}"#,
            r#"{"cmd":"sessiontrace","sessionId":"-flag"}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","lines":0}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","lines":"8"}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","lines":-1}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","lines":1.5}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","clip":"detials"}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","clip":true}"#,
            r#"{"cmd":"sessiontrace","sessionId":"s1","clip":""}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line} must be refused");
        }
    }

    /// One `sessiontrace` dispatch, end to end through a STUB core binary
    /// (`AOIDE_CORE_BIN`): the argv the daemon builds is asserted by the stub
    /// itself echoing its positional parameters, so this also pins that the
    /// session id travels as ONE argv element with no shell involved.
    #[test]
    fn dispatch_session_trace_carries_the_clis_answer_verbatim() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        let root = aoide_test_support::unique_tmp("shellbridge-trace-ok");
        std::fs::create_dir_all(&root).unwrap();
        let stub = root.join("stub-aoide");
        // `$1 session $2 trace $3 <id> $4 --tail $5 N $6 --clip $7 <clip> $8 --json`
        std::fs::write(
            &stub,
            r#"#!/bin/sh
printf '{"status":"ok","command":"session.trace","message":"1 record(s)","data":{"sessionId":"%s","trace":"%s.jsonl","clip":"%s","steps":[{"id":"2","ts":1789603009102,"kind":"thinking","text":"argv: %s %s %s %s %s","error":false,"clipped":true}],"stepsOmitted":3}}\n' "$3" "$3" "$7" "$4" "$5" "$6" "$7" "$8"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_CORE_BIN", &stub);

        let answer = dispatch_session_trace("Aoide-7d23", 5, "detail");
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["sessionId"], "Aoide-7d23");
        assert_eq!(answer["clip"], "detail");
        assert_eq!(answer["lines"], 5);
        assert_eq!(answer["trace"], "Aoide-7d23.jsonl");
        assert_eq!(answer["stepsOmitted"], 3, "the CLI's own bound is carried");
        assert!(answer["at"].as_i64().unwrap_or(0) > 0, "the answer says when it was read");
        let text = answer["steps"][0]["text"].as_str().unwrap();
        assert_eq!(
            text, "argv: --tail 5 --clip detail --json",
            "the id is one argv element, the flags are separate ones, no shell: {text}"
        );
        assert_eq!(answer["steps"][0]["clipped"], true);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A refusal is the CLI's OWN taught refusal, verbatim, with its machine
    /// `reason` beside it and no `steps` — so a card can tell a refusal from a
    /// session that emitted nothing.
    #[test]
    fn dispatch_session_trace_passes_a_refusal_through_verbatim() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        let root = aoide_test_support::unique_tmp("shellbridge-trace-refused");
        std::fs::create_dir_all(&root).unwrap();
        let stub = root.join("stub-aoide");
        std::fs::write(
            &stub,
            r#"#!/bin/sh
printf '{"status":"error","command":"session.trace","message":"`Aoide-x` has no readable trace at /x/none.jsonl","data":{"reason":"no-trace","sessionId":"Aoide-x"}}\n'
exit 1
"#,
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_CORE_BIN", &stub);

        let answer = dispatch_session_trace("Aoide-x", 12, "line");
        assert_eq!(answer["ok"], false, "{answer}");
        assert_eq!(answer["reason"], "no-trace");
        assert_eq!(answer["sessionId"], "Aoide-x", "the CLI's own resolved identity");
        assert!(answer["message"].as_str().unwrap().contains("no readable trace at"), "{answer}");
        assert!(answer.get("steps").is_none(), "a refusal carries no step list");

        // A binary that prints no envelope at all is an honest `no-answer`,
        // never a silent empty step list.
        std::fs::write(&stub, "#!/bin/sh\necho 'aoided must be running'\nexit 1\n").unwrap();
        let answer = dispatch_session_trace("Aoide-x", 12, "line");
        assert_eq!(answer["ok"], false);
        assert_eq!(answer["reason"], "no-answer");
        assert!(answer["message"].as_str().unwrap().contains("aoided must be running"), "{answer}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The wall-clock bound: a child that never exits is KILLED AND REAPED —
    /// its own pid (the stub `exec`s the sleeper, so the pid it writes IS the
    /// child) is gone from `/proc` afterwards, so no query leaves a process
    /// behind.
    #[test]
    fn a_slow_child_is_killed_and_reaped_at_the_wall_clock_bound() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        let root = aoide_test_support::unique_tmp("shellbridge-trace-timeout");
        std::fs::create_dir_all(&root).unwrap();
        let pidfile = root.join("child.pid");
        let stub = root.join("stub-aoide");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\necho $$ > {}\nexec sleep 30\n",
                pidfile.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_CORE_BIN", &stub);

        let argv: Vec<String> = vec!["session".to_string(), "trace".to_string(), "s1".to_string()];
        let started = std::time::Instant::now();
        let out = run_core_bounded(&argv, std::time::Duration::from_millis(300));
        assert!(
            out.is_err(),
            "a child that outlives the bound must not answer: {out:?}"
        );
        assert!(out.unwrap_err().contains("no answer within"));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the bound must be enforced by US, not waited out"
        );

        let pid: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
        let gone = !std::path::Path::new(&format!("/proc/{pid}")).exists();
        assert!(gone, "the timed-out child (pid {pid}) is still alive");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The failure mode the bound must survive, REPRODUCED: the direct child
    /// exits at once but a DESCENDANT inherited its stdout and holds the pipe
    /// open. An unconditional `join()` on the reader would block for the
    /// descendant's whole lifetime, defeating the wall clock; here the read is
    /// DISCARDED after the grace and the caller returns promptly.
    #[test]
    fn a_descendant_holding_the_pipe_open_never_defeats_the_bound() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        let root = aoide_test_support::unique_tmp("shellbridge-trace-heldpipe");
        std::fs::create_dir_all(&root).unwrap();
        let pidfile = root.join("descendant.pid");
        let stub = root.join("stub-aoide");
        // The backgrounded `sleep` inherits this script's stdout (the pipe the
        // runner is reading); the script itself exits immediately, so the pipe
        // stays open with nobody writing to it.
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\nsleep 30 &\necho $! > {}\necho 'partial output'\nexit 0\n",
                pidfile.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_CORE_BIN", &stub);

        let argv: Vec<String> = vec!["session".to_string(), "trace".to_string(), "s1".to_string()];
        let started = std::time::Instant::now();
        let out = run_core_bounded(&argv, std::time::Duration::from_millis(300));
        let elapsed = started.elapsed();
        assert!(
            out.is_err(),
            "a pipe still held open is not the whole answer: {out:?}"
        );
        let reason = out.unwrap_err();
        assert!(reason.contains("stayed open"), "{reason}");
        assert!(
            elapsed < CHILD_DRAIN_GRACE + std::time::Duration::from_secs(3),
            "the grace, not the descendant's lifetime, must set the clock (took {elapsed:?})"
        );

        // Clean up the descendant this test deliberately created.
        if let Ok(text) = std::fs::read_to_string(&pidfile) {
            if let Ok(pid) = text.trim().parse::<i32>() {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The reader-slot cap, over its OWN counter (the process-wide pool is
    /// shared with every other test in this binary, so exhausting it here would
    /// refuse a concurrent dispatch): past the cap a read is refused without
    /// starting a child, a returned slot is reusable, and a release can never
    /// inflate the pool above the cap.
    #[test]
    fn reader_slots_are_capped_and_returned() {
        let cap = 2usize;
        let free = std::sync::atomic::AtomicUsize::new(cap);
        assert!(take_slot(&free, cap));
        assert!(take_slot(&free, cap));
        assert!(!take_slot(&free, cap), "the cap must refuse, never wait");
        release_slot(&free, cap);
        assert!(take_slot(&free, cap), "a returned slot is reusable");
        release_slot(&free, cap);
        release_slot(&free, cap);
        release_slot(&free, cap);
        assert_eq!(
            free.load(std::sync::atomic::Ordering::SeqCst),
            cap,
            "a release can never push the pool above its cap"
        );
        // And the real pool starts the way the constant says it does.
        assert!(ReaderSlot::acquire().is_some());
    }

    #[test]
    fn parse_command_accepts_a_valid_rechecksessions() {
        assert_eq!(
            parse_command(r#"{"cmd":"rechecksessions"}"#),
            Some(BridgeCommand::RecheckSessions)
        );
        // No payload is expected or read — extra fields ignored, surrounding
        // whitespace/newline tolerated like every command.
        assert_eq!(
            parse_command("  {\"cmd\":\"rechecksessions\"}\n"),
            Some(BridgeCommand::RecheckSessions)
        );
        // A near-miss is NOT this command — an unparsed cmd goes nowhere.
        assert_eq!(parse_command(r#"{"cmd":"recheck"}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"recheckSession"}"#), None);
    }

    // ── classify_recheck (a quiet sweep is a successful sweep) ─────────────────

    #[test]
    fn classify_recheck_ok_envelope_returns_its_message() {
        let s = classify_recheck(
            true,
            r#"{"status":"ok","command":"session.reap","message":"reaped 1 dead session(s); dropped 1 total; decayed 0 stopped → idle; cleared 0 orphaned parent link(s); dropped 2 orphaned hook record(s)"}"#,
            "",
        );
        assert!(s.is_ok());
        assert!(s.unwrap().contains("orphaned hook record"));
    }

    #[test]
    fn classify_recheck_nothing_to_reap_still_succeeds() {
        // The common case: a periodic-cadence sweep with nothing dead. Status is
        // "ok" and the file is untouched — that is a successful recheck, NOT a
        // failure (the whole point of judging the envelope, not just exit 0).
        let s = classify_recheck(
            true,
            r#"{"status":"ok","command":"session.reap","message":"nothing to reap (all sessions live)"}"#,
            "",
        );
        assert_eq!(s, Ok("nothing to reap (all sessions live)".to_string()));
    }

    #[test]
    fn classify_recheck_error_status_and_nonzero_exit_are_failures() {
        // A real stage read/write failure exits 0-in-shape but reports "error".
        let s = classify_recheck(
            true,
            r#"{"status":"error","command":"session.reap","message":"could not write sessions.json: permission denied"}"#,
            "",
        );
        assert!(s.is_err());
        assert!(s.unwrap_err().contains("permission denied"));
        // Non-zero exit, reason spoken on stderr.
        assert_eq!(
            classify_recheck(false, "", "boom on stderr"),
            Err("boom on stderr".to_string())
        );
        // Non-zero exit, silent → still explains itself, never a blank reason.
        assert!(classify_recheck(false, "", "").unwrap_err().contains("nonzero"));
        // Exit 0 but not the JSON envelope → not a proven sweep (the ipc.rs lesson).
        assert!(classify_recheck(true, "not json at all", "").unwrap_err().contains("parseable"));
    }

    // ── classify_usage_refresh (success is the real output, not the exit) ─────

    #[test]
    fn classify_usage_refresh_ok_envelope_returns_its_message() {
        let s = classify_usage_refresh(
            true,
            r#"{"status":"ok","command":"usage","message":"today 42 tokens — state/usage.json written"}"#,
            "",
        );
        assert_eq!(s, Ok("today 42 tokens — state/usage.json written".to_string()));
    }

    #[test]
    fn classify_usage_refresh_degraded_live_still_succeeds() {
        // The whole point: a `live:{ok:false}` block (no creds / OAuth rejection
        // / transport error) is NOT a refresh failure — the file was still
        // written and the gadget renders it degraded. Status stays "ok".
        let s = classify_usage_refresh(
            true,
            r#"{"status":"ok","command":"usage","message":"written","data":{"live":{"ok":false,"error":"no ~/.claude credentials"}}}"#,
            "",
        );
        assert!(s.is_ok(), "degraded live must not read as a failed refresh: {s:?}");
    }

    #[test]
    fn classify_usage_refresh_error_status_is_a_failure() {
        // A real write failure exits 0-in-shape but reports status "error".
        let s = classify_usage_refresh(
            true,
            r#"{"status":"error","command":"usage","message":"failed to write state/usage.json: permission denied"}"#,
            "",
        );
        assert!(s.is_err());
        assert!(s.unwrap_err().contains("permission denied"));
    }

    #[test]
    fn classify_usage_refresh_nonzero_exit_is_a_failure_with_a_reason() {
        // Non-zero exit, reason spoken on stderr.
        let s = classify_usage_refresh(false, "", "boom on stderr");
        assert_eq!(s, Err("boom on stderr".to_string()));
        // Non-zero exit, silent → still explains itself, never a blank reason.
        let s = classify_usage_refresh(false, "", "");
        assert!(s.unwrap_err().contains("nonzero"));
    }

    #[test]
    fn classify_usage_refresh_unparseable_exit_zero_is_a_failure() {
        // Exit 0 alone is NOT success — output that isn't the JSON envelope
        // means the call didn't land the way we think (the ipc.rs lesson).
        let s = classify_usage_refresh(true, "not json at all", "");
        assert!(s.is_err());
        assert!(s.unwrap_err().contains("parseable"));
        // Valid JSON but no status field is likewise not a proven refresh.
        let s = classify_usage_refresh(true, r#"{"command":"usage"}"#, "");
        assert!(s.is_err());
    }

    #[test]
    fn rice_mode_toggle_target_is_a_two_way_toggle_not_a_three_way_cycle() {
        // Staging locks to declarative...
        assert_eq!(rice_mode_toggle_target(RiceMode::Staging), "declarative");
        // ...and BOTH declarative and draft unlock back to plain staging —
        // there is no generic "next draft" a bare click could cycle into.
        assert_eq!(rice_mode_toggle_target(RiceMode::Declarative), "stage");
        assert_eq!(rice_mode_toggle_target(RiceMode::Draft), "stage");
    }

    // ── rice_mode_toggle_default_song (the bar-toggle-only baseline-song fix) ──

    #[test]
    fn declarative_toggle_passes_the_env_song_explicitly_when_set() {
        assert_eq!(
            rice_mode_toggle_default_song("declarative", Some("sonata")),
            Some("sonata".to_string())
        );
        // Surrounding whitespace is trimmed, same tolerance as the wire commands.
        assert_eq!(
            rice_mode_toggle_default_song("declarative", Some("  sonata  ")),
            Some("sonata".to_string())
        );
    }

    #[test]
    fn declarative_toggle_falls_back_to_the_bare_call_when_env_is_absent_or_blank() {
        // Unset (outside the systemd service, or before a rebuild lands the
        // env var) must not turn a working toggle into a broken one.
        assert_eq!(rice_mode_toggle_default_song("declarative", None), None);
        // Present but blank/whitespace-only is treated the same as absent.
        assert_eq!(rice_mode_toggle_default_song("declarative", Some("")), None);
        assert_eq!(rice_mode_toggle_default_song("declarative", Some("   ")), None);
    }

    #[test]
    fn stage_toggle_is_unaffected_by_the_env_var_either_way() {
        // The staging direction stays on whatever's currently being edited
        // (current_staged_song()-driven, commands/mode.rs) regardless of
        // AOIDE_DEFAULT_SONG — this fix is scoped to the declarative
        // direction only.
        assert_eq!(rice_mode_toggle_default_song("stage", Some("sonata")), None);
        assert_eq!(rice_mode_toggle_default_song("stage", None), None);
    }

    #[test]
    fn parse_command_rejects_bad_or_empty_input() {
        // Empty / absent address → None (never dispatch a blank focus).
        assert_eq!(parse_command(r#"{"cmd":"focuswindow","address":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"focuswindow","address":"   "}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"focuswindow"}"#), None);
        // Unknown command → None.
        assert_eq!(parse_command(r#"{"cmd":"explode","address":"0x1"}"#), None);
        // Missing cmd → None.
        assert_eq!(parse_command(r#"{"address":"0x1"}"#), None);
        // Malformed / non-object JSON → None (the loop logs + ignores).
        assert_eq!(parse_command("not json at all"), None);
        assert_eq!(parse_command("{ broken"), None);
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("[1,2,3]"), None);
    }

    // ── session actions (the acknowledged session-menu bridge) ─────────────

    /// Build a `Vec<String>` argv/plan-row from string literals — test-only
    /// sugar so the argv-exactness assertions below read as plain literals.
    fn sv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- parse gate: acceptance --

    #[test]
    fn parse_command_accepts_a_session_project_assignment() {
        match parse_command(
            r#"{"cmd":"sessionaction","sessionId":"s1","action":"project","fields":{"project":"aoide"}}"#,
        ) {
            Some(BridgeCommand::SessionAction { session_id, action, fields }) => {
                assert_eq!(session_id, "s1");
                assert_eq!(action, "project");
                assert_eq!(fields["project"], "aoide");
            }
            other => panic!("expected a session project assignment, got {other:?}"),
        }
    }

    #[test]
    fn parse_command_accepts_a_session_project_clear() {
        // An explicit empty STRING is the clear request, not a rejection.
        assert!(matches!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"project","fields":{"project":""}}"#
            ),
            Some(BridgeCommand::SessionAction { .. })
        ));
    }

    #[test]
    fn parse_command_accepts_a_session_kill() {
        assert!(matches!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"s1","action":"kill"}"#),
            Some(BridgeCommand::SessionAction { .. })
        ));
        // Extra unknown keys are accepted and ignored.
        assert!(matches!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"kill","fields":{"force":true}}"#
            ),
            Some(BridgeCommand::SessionAction { .. })
        ));
    }

    #[test]
    fn parse_command_accepts_a_session_undying_toggle() {
        for state in ["on", "off"] {
            match parse_command(&format!(
                r#"{{"cmd":"sessionaction","sessionId":"s1","action":"undying","fields":{{"state":"{state}"}}}}"#
            )) {
                Some(BridgeCommand::SessionAction { fields, .. }) => {
                    assert_eq!(fields["state"], state);
                }
                other => panic!("expected an undying toggle for {state}, got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_command_accepts_a_createproject_and_an_editproject() {
        for action in ["createproject", "editproject"] {
            let line = format!(
                r#"{{"cmd":"sessionaction","sessionId":"s1","action":"{action}","fields":{{"name":"aoide","paths":["/home/khoa/Aoide"]}}}}"#
            );
            assert!(
                matches!(parse_command(&line), Some(BridgeCommand::SessionAction { .. })),
                "{action} must parse"
            );
        }
    }

    #[test]
    fn parse_command_accepts_a_projectaction_with_no_session_id() {
        // P-14 M1 §2d: `projectaction` is zero-session — no `sessionId`
        // anywhere on the wire, in the parsed command, or in `fields`.
        let line = r#"{"cmd":"projectaction","action":"create","name":"n1proj","paths":["/srv/n1proj"]}"#;
        match parse_command(line) {
            Some(BridgeCommand::ProjectAction { name, action, fields }) => {
                assert_eq!(name, "n1proj");
                assert_eq!(action, "create");
                assert!(fields.get("sessionId").is_none());
            }
            other => panic!("expected a ProjectAction, got {other:?}"),
        }
    }

    // -- parse gate: rejection --

    #[test]
    fn parse_command_rejects_an_unknown_session_action() {
        for action in ["reboot", "", "removeproject"] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"sessionaction","sessionId":"s1","action":"{action}"}}"#
                )),
                None,
                "{action} must be refused"
            );
        }
    }

    #[test]
    fn parse_command_rejects_a_session_action_with_no_session_id() {
        assert_eq!(parse_command(r#"{"cmd":"sessionaction","action":"kill"}"#), None);
        assert_eq!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"","action":"kill"}"#),
            None
        );
        assert_eq!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"   ","action":"kill"}"#),
            None
        );
    }

    #[test]
    fn parse_command_rejects_a_flag_shaped_session_id_or_name() {
        assert_eq!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"--id","action":"kill"}"#),
            None
        );
        assert_eq!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"-x","action":"kill"}"#),
            None
        );
        assert_eq!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"createproject","fields":{"name":"-rf","paths":["/a"]}}"#
            ),
            None
        );
    }

    #[test]
    fn parse_command_rejects_an_undying_state_that_is_not_on_or_off() {
        for fields in [r#"{"state":"yes"}"#, r#"{"state":true}"#, r#"{"on":true}"#, r#"{}"#] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"sessionaction","sessionId":"s1","action":"undying","fields":{fields}}}"#
                )),
                None,
                "{fields} must be refused"
            );
        }
    }

    #[test]
    fn parse_command_rejects_a_project_action_with_a_missing_or_non_string_project_field() {
        // A missing key, `null`, or a non-string value never mutates
        // anything — the whitelist drops the request as unknown rather than
        // guessing at "clear".
        for fields in [r#"{}"#, r#"{"project":null}"#, r#"{"project":5}"#] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"sessionaction","sessionId":"s1","action":"project","fields":{fields}}}"#
                )),
                None,
                "{fields} must be refused"
            );
        }
        // An absent `fields` key entirely is the same as `{}` above.
        assert_eq!(
            parse_command(r#"{"cmd":"sessionaction","sessionId":"s1","action":"project"}"#),
            None
        );
    }

    #[test]
    fn parse_command_rejects_a_project_edit_with_no_paths() {
        // An "exact replacement" with nothing to replace with is a mistake,
        // never an instruction to erase.
        for action in ["createproject", "editproject"] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"sessionaction","sessionId":"s1","action":"{action}","fields":{{"name":"aoide","paths":[]}}}}"#
                )),
                None,
                "{action} with an empty paths list must be refused"
            );
        }
    }

    #[test]
    fn parse_command_rejects_a_relative_or_control_charactered_path() {
        // One bad element rejects the whole action.
        let cases = ["[\"work/aoide\"]", "[\"~/Aoide\"]", "[\"/home/khoa/a\\nb\"]", "[123]"];
        for paths in cases {
            let line = format!(
                "{{\"cmd\":\"sessionaction\",\"sessionId\":\"s1\",\"action\":\"createproject\",\"fields\":{{\"name\":\"aoide\",\"paths\":{paths}}}}}"
            );
            assert_eq!(parse_command(&line), None, "{paths} must be refused");
        }
    }

    #[test]
    fn parse_command_rejects_a_createproject_with_no_name() {
        assert_eq!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"createproject","fields":{"paths":["/a"]}}"#
            ),
            None
        );
        assert_eq!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"createproject","fields":{"name":"","paths":["/a"]}}"#
            ),
            None
        );
        assert_eq!(
            parse_command(
                r#"{"cmd":"sessionaction","sessionId":"s1","action":"createproject","fields":{"name":"   ","paths":["/a"]}}"#
            ),
            None
        );
    }

    // -- argv exactness --

    #[test]
    fn session_action_args_builds_the_exact_undying_argv_for_both_states() {
        assert_eq!(
            session_action_args("s1", "undying", &json!({"state":"on"})),
            Some(vec![sv(&["session", "grant", "undying", "on", "--id", "s1"])])
        );
        assert_eq!(
            session_action_args("s1", "undying", &json!({"state":"off"})),
            Some(vec![sv(&["session", "grant", "undying", "off", "--id", "s1"])])
        );
    }

    #[test]
    fn session_action_args_builds_the_exact_project_assignment_argv() {
        assert_eq!(
            session_action_args("s1", "project", &json!({"project":"aoide"})),
            Some(vec![sv(&["session", "project", "--id", "s1", "--project", "aoide"])])
        );
    }

    #[test]
    fn session_action_args_builds_the_exact_project_clear_argv() {
        assert_eq!(
            session_action_args("s1", "project", &json!({"project":""})),
            Some(vec![sv(&["session", "project", "--id", "s1", "--clear"])])
        );
    }

    #[test]
    fn session_action_args_rejects_a_missing_or_non_string_project_field() {
        // `project` must BE a JSON string: a missing key, `null`, or a
        // number all read as unknown and refuse the whole action — never as
        // an implicit clear.
        assert_eq!(session_action_args("s1", "project", &json!({})), None);
        assert_eq!(session_action_args("s1", "project", &json!({"project": null})), None);
        assert_eq!(session_action_args("s1", "project", &json!({"project": 5})), None);
    }

    #[test]
    fn session_action_args_builds_the_exact_kill_argv() {
        assert_eq!(
            session_action_args("s1", "kill", &json!({})),
            Some(vec![sv(&["session", "kill", "--id", "s1"])])
        );
    }

    #[test]
    fn session_action_args_builds_createprojects_two_argvs_in_order() {
        let plan = session_action_args(
            "s1",
            "createproject",
            &json!({"name":"aoide","paths":["/a","/b"]}),
        )
        .expect("createproject must build a plan");
        assert_eq!(
            plan,
            vec![
                sv(&["project", "add", "aoide", "/a", "/b", "--new"]),
                sv(&["session", "project", "--id", "s1", "--project", "aoide"]),
            ]
        );
        assert_eq!(plan[0].last().map(String::as_str), Some("--new"));
    }

    #[test]
    fn session_action_args_builds_the_exact_editproject_argv() {
        assert_eq!(
            session_action_args("s1", "editproject", &json!({"name":"aoide","paths":["/a","/b"]})),
            Some(vec![sv(&["project", "edit", "aoide", "/a", "/b"])])
        );
    }

    #[test]
    fn session_action_args_never_admits_whitespace_in_a_session_id() {
        assert_eq!(session_action_args("a b", "kill", &json!({})), None);
        assert_eq!(session_action_args("a\nb", "kill", &json!({})), None);
        assert_eq!(session_action_args("a\tb", "kill", &json!({})), None);
    }

    #[test]
    fn session_action_args_admits_ordinary_spaces_but_rejects_control_characters_in_a_name() {
        // "My Project" is a real, legal name across all three shapes that
        // carry one — argv elements are passed to `Command::arg` whole,
        // never through a shell, so a space is no more dangerous here than
        // in a path.
        assert_eq!(
            session_action_args("s1", "project", &json!({"project":"My Project"})),
            Some(vec![sv(&["session", "project", "--id", "s1", "--project", "My Project"])])
        );
        assert_eq!(
            session_action_args(
                "s1",
                "createproject",
                &json!({"name":"My Project","paths":["/a"]})
            ),
            Some(vec![
                sv(&["project", "add", "My Project", "/a", "--new"]),
                sv(&["session", "project", "--id", "s1", "--project", "My Project"]),
            ])
        );
        assert_eq!(
            session_action_args(
                "s1",
                "editproject",
                &json!({"name":"My Project","paths":["/a"]})
            ),
            Some(vec![sv(&["project", "edit", "My Project", "/a"])])
        );
        // A space is not a blanket whitespace exemption: control characters
        // are still refused.
        assert_eq!(
            session_action_args("s1", "project", &json!({"project":"a\u{1b}b"})),
            None
        );
        assert_eq!(session_action_args("s1", "project", &json!({"project":"a\nb"})), None);
    }

    #[test]
    fn session_action_args_admits_a_space_in_a_path_too() {
        // A PATH containing a space was always accepted — a different rule
        // ([`safe_action_path`]), same underlying reasoning.
        assert_eq!(
            session_action_args(
                "s1",
                "editproject",
                &json!({"name":"aoide","paths":["/home/khoa/My Documents"]})
            ),
            Some(vec![sv(&["project", "edit", "aoide", "/home/khoa/My Documents"])])
        );
    }

    // -- projectaction argv exactness (P-14 M1 §2d) --

    #[test]
    fn project_action_args_builds_a_zero_session_create_plan() {
        let plan = project_action_args(
            "n1proj",
            "create",
            &json!({
                "paths": ["/srv/n1proj"],
                "hosts": [{"name": "n1", "roots": ["/srv/n1/proj"]}],
            }),
        )
        .expect("create must build a plan");
        assert_eq!(
            plan,
            vec![
                sv(&["project", "add", "n1proj", "/srv/n1proj", "--new"]),
                sv(&["project", "add", "n1proj", "/srv/n1/proj", "--host", "n1"]),
            ]
        );
        // No session id in any step of the plan.
        for step in &plan {
            assert!(!step.contains(&"--id".to_string()));
        }
    }

    #[test]
    fn project_action_args_refuses_a_create_with_no_paths() {
        // An "exact replacement"/creation with nothing to place is a
        // mistake, never an instruction to erase — same rule
        // `createproject`/`editproject` hold above.
        assert_eq!(project_action_args("n1proj", "create", &json!({"paths": []})), None);
        assert_eq!(project_action_args("n1proj", "create", &json!({})), None);
        assert_eq!(project_action_args("n1proj", "edit", &json!({"paths": []})), None);
    }

    #[test]
    fn project_action_args_refuses_an_ill_shaped_host_name() {
        // One bad host entry refuses the whole action — the same
        // whole-array discipline `paths` holds.
        assert_eq!(
            project_action_args(
                "n1proj",
                "create",
                &json!({"paths": ["/srv/n1proj"], "hosts": [{"name": "-rf", "roots": []}]})
            ),
            None
        );
        assert_eq!(
            project_action_args(
                "n1proj",
                "create",
                &json!({"paths": ["/srv/n1proj"], "hosts": [{"name": "N1", "roots": []}]})
            ),
            None
        );
        assert_eq!(
            project_action_args(
                "n1proj",
                "removehost",
                &json!({"hosts": [{"name": "", "roots": []}]})
            ),
            None
        );
    }

    #[test]
    fn an_edit_with_a_host_and_no_roots_reexecs_add_and_keeps_its_roots() {
        // Membership-only: an `edit --host` with nothing to replace with
        // would wipe an existing host's roots, so an empty-roots host
        // re-execs `add` instead (membership kept, roots untouched) — see
        // the doc comment above `project_action_args`.
        let plan = project_action_args(
            "n1proj",
            "edit",
            &json!({
                "paths": ["/srv/n1proj"],
                "hosts": [{"name": "n1", "roots": []}],
            }),
        )
        .expect("edit must build a plan");
        assert_eq!(
            plan,
            vec![
                sv(&["project", "edit", "n1proj", "/srv/n1proj"]),
                sv(&["project", "add", "n1proj", "--host", "n1"]),
            ]
        );
    }

    #[test]
    fn an_edit_with_a_host_and_roots_reexecs_edit_with_those_roots() {
        // Non-empty roots replace that host's roots exactly, via `edit`.
        let plan = project_action_args(
            "n1proj",
            "edit",
            &json!({
                "paths": ["/srv/n1proj"],
                "hosts": [{"name": "n1", "roots": ["/srv/n1/proj"]}],
            }),
        )
        .expect("edit must build a plan");
        assert_eq!(
            plan,
            vec![
                sv(&["project", "edit", "n1proj", "/srv/n1proj"]),
                sv(&["project", "edit", "n1proj", "/srv/n1/proj", "--host", "n1"]),
            ]
        );
    }

    #[test]
    fn a_host_entry_with_a_non_array_roots_field_refuses_the_whole_action() {
        // A present-but-non-array `roots` is a bad element, same as an
        // ill-shaped host name or path — never silently read as "no roots".
        // An ABSENT `roots` key is the only shape that still means
        // membership-only (covered above).
        assert_eq!(
            project_action_args(
                "n1proj",
                "create",
                &json!({"paths": ["/srv/n1proj"], "hosts": [{"name": "n1", "roots": "not-an-array"}]})
            ),
            None
        );
        assert_eq!(
            project_action_args(
                "n1proj",
                "edit",
                &json!({"paths": ["/srv/n1proj"], "hosts": [{"name": "n1", "roots": 42}]})
            ),
            None
        );
    }

    // -- reply shaping --

    #[test]
    fn a_session_action_reply_reports_an_ok_outcome_with_its_own_message() {
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "project",
            true,
            r#"{"status":"ok","command":"session.project","message":"session project updated","gated":false}"#,
            "",
        );
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["message"], "session project updated");
        assert_eq!(reply["action"], "project");
        assert_eq!(reply["sessionId"], "s1");
        assert!(reply.get("data").is_none());
    }

    #[test]
    fn a_session_action_reply_carries_the_cli_outcomes_data_verbatim() {
        // A `kill` reply can show the resolved target/pid this way.
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "kill",
            true,
            r#"{"status":"ok","command":"session.kill","message":"session killed","data":{"pid":1234,"target":"s1"}}"#,
            "",
        );
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["data"], json!({"pid": 1234, "target": "s1"}));
    }

    #[test]
    fn a_session_action_reply_reports_an_error_outcome_as_not_ok() {
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "kill",
            false,
            r#"{"status":"error","command":"session.kill","message":"session is not registered locally","gated":true}"#,
            "",
        );
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["message"], "session is not registered locally");
        // The `gated` marker never reaches QML and never makes a reply ok.
        assert!(reply.get("gated").is_none());
    }

    #[test]
    fn a_session_action_reply_reads_a_usage_envelope_off_stderr() {
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "createproject",
            false,
            "",
            r#"{"status":"usage","command":"project.add","message":"unrecognized flag `--new` for `aoide project add`","gated":false}"#,
        );
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["message"], "unrecognized flag `--new` for `aoide project add`");
    }

    #[test]
    fn a_session_action_reply_falls_back_when_neither_stream_is_an_envelope() {
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "kill",
            false,
            "boom",
            "aoided must be running for session management",
        );
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["message"], "aoided must be running for session management");

        let reply = session_action_reply(ActionSubject::Session("s1"), "kill", false, "", "");
        assert_eq!(reply["ok"], false);
        assert_ne!(reply["message"].as_str().unwrap_or(""), "");
    }

    #[test]
    fn a_session_action_reply_is_one_wire_line() {
        let reply = session_action_reply(
            ActionSubject::Session("s1"),
            "project",
            true,
            r#"{"status":"ok","command":"session.project","message":"a\nb"}"#,
            "",
        );
        assert!(!reply.to_string().contains('\n'));
        assert_eq!(reply["action"], "project");
        assert_eq!(reply["sessionId"], "s1");
    }

    // ── the accept loop (§3b): idleness must never starve another connection ──

    /// Confirms `serve` spawns a thread per connection rather than serving
    /// serially: an idle, persistent client — exactly what Quickshell's own
    /// shared socket is, held open between human gestures — must never block
    /// a later client's line from ever being dispatched. This test binds its
    /// OWN listener, so `socket_path()` is never consulted and the live
    /// shellbridge socket is never touched.
    ///
    /// Client 2 sends an action the whitelist REFUSES, so the observable is
    /// the `unparseable` audit line landing in `<root>/log` — a refused line
    /// spawns no child process at all, so this test cannot reach a real
    /// binary or a live daemon under any env mishap. `daemon::bin::core_bin()`
    /// *is* redirectable via `AOIDE_CORE_BIN` for a future test wanting a
    /// real reply line, but a unit test spawning it here would be exactly the
    /// "fresh binary against the live system" this slice's hard rules forbid.
    ///
    /// This test MUST fail on the pre-§3b serial `serve`: client 2's line
    /// never reaches `handle_conn` while client 1 sits open, so the accept
    /// loop is blocked in `handle_conn(client 1)` forever.
    #[test]
    fn an_idle_persistent_client_never_blocks_the_next_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "AOIDE_AUDIT_LOG",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
        ]);
        let root = aoide_test_support::unique_tmp("shellbridge-idle-client");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("AOIDE_STAGE_DIR", &root);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let sock = root.join("test-shellbridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener)); // never joined

        // Client 1: connect and hold the link open, writing nothing — the
        // exact shape of Quickshell's own shared socket idling between human
        // gestures.
        let _client1 = UnixStream::connect(&sock).unwrap();

        // Client 2: a refused sessionaction — the whitelist drops it before
        // any child process is ever spawned.
        {
            use std::io::Write;
            let mut client2 = UnixStream::connect(&sock).unwrap();
            client2
                .write_all(b"{\"cmd\":\"sessionaction\",\"sessionId\":\"s1\",\"action\":\"reboot\"}\n")
                .unwrap();
            client2.flush().unwrap();
        }

        let log = root.join("log");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut seen = false;
        while std::time::Instant::now() < deadline {
            if let Ok(text) = std::fs::read_to_string(&log) {
                if text.contains("unparseable") {
                    seen = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            seen,
            "client 2's line was never audited within 2s — an idle client 1 is blocking \
             the accept loop (the pre-3b serial-accept regression)"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    // §3b's thread-per-connection accept loop means N herald pushes on N
    // distinct connections can now run `edit_ledger` from N different
    // threads at once. `edit_ledger` wraps its read-modify-write in
    // `with_stage_lock` for exactly this reason — this test is the one that
    // would go red (a lost update: fewer than N notifications land) if that
    // lock were ever dropped.
    #[test]
    fn n_concurrent_herald_pushes_all_land_in_the_ledger() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "AOIDE_AUDIT_LOG",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
        ]);
        let root = aoide_test_support::unique_tmp("shellbridge-herald-race");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("AOIDE_STAGE_DIR", &root);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let sock = root.join("test-shellbridge-herald.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener)); // never joined, own listener

        const N: usize = 12;
        let senders: Vec<_> = (0..N)
            .map(|i| {
                let sock = sock.clone();
                std::thread::spawn(move || {
                    use std::io::Write;
                    let mut client = UnixStream::connect(&sock).unwrap();
                    let line = format!(
                        "{{\"cmd\":\"heraldpush\",\"notification\":{{\"id\":\"race-{i}\",\
                         \"app\":\"test\",\"summary\":\"s\",\"body\":\"b\",\
                         \"urgency\":\"normal\",\"progress\":-1,\"timeoutMs\":0,\
                         \"receivedAt\":\"now\",\"kind\":\"toast\"}}}}\n"
                    );
                    client.write_all(line.as_bytes()).unwrap();
                    client.flush().unwrap();
                })
            })
            .collect();
        for s in senders {
            s.join().unwrap();
        }

        let path = root.join("herald.json");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut count = 0;
        while std::time::Instant::now() < deadline {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(file) = serde_json::from_str::<crate::herald::HeraldFile>(&text) {
                    count = file.notifications.len();
                    if count == N {
                        break;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            count, N,
            "expected all {N} concurrent herald pushes to land in the ledger, found \
             {count} instead — a lost update means edit_ledger's read-modify-write is \
             racing under the thread-per-connection accept loop"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
