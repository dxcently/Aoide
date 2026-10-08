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
use aoide_storage::mode::{load_mode_marker, ModeMarker, RiceMode};
use serde_json::{json, Value};
// The compositor adapter's focused-workspace read and the ONE sentence an
// omitted `<workspace>` with no adapter to ask gets (`graph/workspace.rs`,
// re-exported beside `focused_workspace` at `crate::graph` — the bridge
// resolves an omitted `workspaceaction` workspace with the very function the
// CLI's own caller-side resolution uses, and speaks the CLI's own words).
use crate::graph::{focused_workspace, NO_COMPOSITOR};
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
use aoide_protocol::win_unix::{UnixListener, UnixStream};
use std::path::PathBuf;

/// The shellbridge socket path — **contract**: never computed independently.
/// `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`.
pub fn socket_path() -> PathBuf {
    aoide_storage::runtime_dir::socket_dir().join("shellbridge.sock")
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
/// How long one `ricemenu` answer may take, both children together: under
/// ShellBridge.qml's own five second `replyTimeoutMs`, so the daemon abandons
/// the child and says so before the caller gives up on the daemon.
const RICE_MENU_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
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
    /// `{ "cmd": "ricemode", "action": "stage|declarative", "name": "…" }` — a
    /// row of the bar's RICE menu: switch the stage to the named runtime song,
    /// or lock back to declarative. Fire-and-forget like [`Self::RiceDraft`]:
    /// the outcome is a toast, never a reply. The closed set and its field
    /// rules are [`RiceModeAction`]'s. See [`dispatch_rice_mode`].
    RiceMode { action: RiceModeAction },
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
    /// `{ "cmd": "workspaceaction", "action": "set|clear", "workspace": <int>,
    /// "project": "<name>", "new": <bool> }` — the bar's click that binds a
    /// compositor workspace to a project, or unbinds it (W-P5). Modelled on
    /// [`Self::ProjectAction`]: zero-session (no session id anywhere on the
    /// wire, in the plan, the reply, or the audit line), a closed two-action
    /// set ([`WorkspaceBinding`]), and no binding logic of its own — it plans
    /// the same argv `aoide workspace set [<ws>] <project> [--new]` /
    /// `aoide workspace clear <ws>` take and runs it through the same core
    /// re-exec every acknowledged action uses.
    ///
    /// `workspace` is the compositor's own workspace id, omitted to mean the
    /// FOCUSED one. Unlike a session or project action, one field is resolved
    /// here rather than left to the child: [`dispatch_workspace_action`] asks
    /// the compositor adapter ([`focused_workspace`]) and plans the RESOLVED
    /// integer, because `workspace clear` takes an id and no wire caller can
    /// be expected to know the focused one. A host with no adapter is
    /// ANSWERED (`reason: "no-compositor"`), never dropped — the click is
    /// parked on a reply.
    WorkspaceAction { workspace: Option<i64>, binding: WorkspaceBinding },
    /// `{ "cmd": "ricemenu" }` — the RICE menu's READ: the runtime songbook
    /// and the mode, plus the drafts of the song the current mode is working
    /// on, answered on this connection like [`Self::SessionTrace`]. No
    /// payload: the daemon runs `lyra rice list --json` and, when the mode
    /// names a song ([`drafts_song`]), `lyra rice draft list <song> --json`,
    /// both under one [`RICE_MENU_TIMEOUT`] budget. Nothing is written. See
    /// [`dispatch_rice_menu`].
    RiceMenu,
    /// `{ "cmd": "ricedraft", "action": "enter|new|save", "name": "…" }` — a
    /// picker click that enters a draft, forks a new one and enters it, or
    /// saves the stage as a draft. Fire-and-forget like [`Self::RiceMode`]:
    /// the outcome is a toast, never a reply. The closed set and its field
    /// rules are [`RiceDraftAction`]'s; the plan, in `rice` argv, is
    /// [`rice_draft_plan`]. See [`dispatch_rice_draft`].
    RiceDraft { action: RiceDraftAction },
}

/// The two things a `ricemode` line can ask for. A closed set: an unknown or
/// non-string action, a `stage` with no usable `name`, or a `declarative`
/// carrying one is refused at the wire (`parse_command` → `None`, no reply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiceModeAction {
    /// Stage this runtime song. Whether it exists is the CLI's call.
    Stage { name: String },
    /// Lock back to the declared song; the CLI resolves which that is.
    Declarative,
}

impl RiceModeAction {
    fn from_wire(v: &Value) -> Option<Self> {
        let name = v.get("name");
        match v.get("action").and_then(Value::as_str)? {
            "stage" => {
                let name = name?.as_str()?;
                safe_session_id(name).then(|| Self::Stage { name: name.to_string() })
            }
            "declarative" => name.is_none().then_some(Self::Declarative),
            _ => None,
        }
    }

    /// The wire name back, for the audit line.
    fn as_str(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Declarative => "declarative",
        }
    }
}

/// The three things a `ricedraft` line can ask for. A closed set: an unknown
/// or non-string action, an `enter` with no usable `name`, or a `new`/`save`
/// carrying one is refused at the wire (`parse_command` → `None`, no reply),
/// never dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiceDraftAction {
    /// Route the stage into this draft. The name is one argv token and no
    /// more: whether it is a valid draft name is the CLI's call —
    /// `aoide-song` is lyra-only and conduct never depends on it, so the
    /// regex lives in one place. Whether the draft EXISTS is nobody's check
    /// here: `rice mode draft` forks the current stage into a valid name that
    /// is not there yet, so an enter is enter-or-create.
    Enter { name: String },
    /// Mint the next free draft name from the staged song, then enter it.
    New,
    /// Snapshot the stage as a draft of the staged song. Never changes mode,
    /// and is refused while declarative is locked ([`rice_draft_plan`]).
    Save,
}

impl RiceDraftAction {
    /// Parse one wire line's `action` and its own fields. `new` and `save`
    /// take no `name`: one beside them is a caller that meant `enter`, and
    /// silently ignoring it would let that mistake read as success (the
    /// [`WorkspaceBinding`] `clear` rule).
    fn from_wire(v: &Value) -> Option<Self> {
        let name = v.get("name");
        match v.get("action").and_then(Value::as_str)? {
            "enter" => {
                let name = name?.as_str()?;
                safe_session_id(name).then(|| Self::Enter { name: name.to_string() })
            }
            "new" => name.is_none().then_some(Self::New),
            "save" => name.is_none().then_some(Self::Save),
            _ => None,
        }
    }

    /// The wire name back, for the audit line.
    fn as_str(&self) -> &'static str {
        match self {
            Self::Enter { .. } => "enter",
            Self::New => "new",
            Self::Save => "save",
        }
    }
}

/// The two things a `workspaceaction` line can ask for. A closed set: an
/// unknown action word, a `clear` carrying a `project`, or a `project` that is
/// not a name is refused at the wire (`parse_command` → `None`, no reply, like
/// every other line the whitelists refuse), never dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBinding {
    /// Bind. `new` is `--new`: register the name when it is unregistered
    /// (name-only, no folder) and bind it in one call — the CLI stays the one
    /// authority on whether the name exists, the bridge never pre-checks.
    Set { project: String, new: bool },
    /// Unbind.
    Clear,
}

impl WorkspaceBinding {
    /// Parse one wire line's `action` and its own fields. SHAPE only, never
    /// state — whether the project exists, and whether this workspace is
    /// already bound, are the CLI's checks, not the bridge's.
    fn from_wire(action: &str, fields: &Value) -> Option<Self> {
        match action {
            "set" => {
                // A missing, `null`, or non-string `project` is refused
                // outright: there is no "clear" reading of a malformed
                // bind request.
                let project = fields.get("project").and_then(Value::as_str)?.trim();
                if !safe_action_value(project) {
                    return None;
                }
                let new = match fields.get("new") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(new)) => *new,
                    // Present but not a bool: refused, never read as false.
                    Some(_) => return None,
                };
                Some(Self::Set { project: project.to_string(), new })
            }
            // `clear` carries a workspace and nothing else: a `project` or a
            // `new` beside it is a caller that meant `set`, and silently
            // ignoring the field would let that mistake read as a successful
            // unbind.
            "clear" => {
                if fields.get("project").is_some() || fields.get("new").is_some() {
                    return None;
                }
                Some(Self::Clear)
            }
            _ => None,
        }
    }

    /// The wire name back, for the reply and the audit line.
    fn as_str(&self) -> &'static str {
        match self {
            Self::Set { .. } => "set",
            Self::Clear => "clear",
        }
    }

    /// The ONE argv this binding plans for one RESOLVED workspace id — exactly
    /// the argv the CLI takes, so `--new` is the only optional element and
    /// nothing here re-implements a binding. `--json` is appended by the
    /// runner ([`run_core_step`]), never planned here.
    fn plan(&self, ws: i64) -> Vec<String> {
        match self {
            Self::Set { project, new } => {
                let mut argv = vec![
                    "workspace".to_string(),
                    "set".to_string(),
                    ws.to_string(),
                    project.clone(),
                ];
                if *new {
                    argv.push("--new".to_string());
                }
                argv
            }
            Self::Clear => vec!["workspace".to_string(), "clear".to_string(), ws.to_string()],
        }
    }
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

    /// The program + args this action spawns. `lock` asks the compositor to
    /// start hyprlock rather than spawning it here: this unit runs with
    /// NoNewPrivileges, which a child inherits, and hyprlock's PAM check needs
    /// the setuid `unix_chkpwd`, so a hyprlock spawned here refuses every
    /// password. `logout` exits the compositor; the rest are systemd commands.
    fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Lock => ("hyprctl", &["dispatch", "exec", "hyprlock"]),
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
        "workspaceaction" => {
            let action = v.get("action")?.as_str()?;
            // `fields` here IS the whole wire object, the `projectaction`
            // shape: `project`/`new` sit at the top level, never nested.
            let binding = WorkspaceBinding::from_wire(action, &v)?;
            // An INTEGER, the same value `SessionRecord.workspace` holds: a
            // string or a float is refused (`as_i64`), a NEGATIVE is accepted
            // — Hyprland's named/special workspaces carry negative ids, and
            // the CLI's own `workspace_id` parses the same `i64`. Absent (or
            // `null`) means "the focused one", resolved at dispatch.
            let workspace = match v.get("workspace") {
                None | Some(Value::Null) => None,
                Some(raw) => Some(raw.as_i64()?),
            };
            Some(BridgeCommand::WorkspaceAction { workspace, binding })
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
        "ricemode" => RiceModeAction::from_wire(&v).map(|action| BridgeCommand::RiceMode { action }),
        "ricemenu" => Some(BridgeCommand::RiceMenu),
        "ricedraft" => RiceDraftAction::from_wire(&v).map(|action| BridgeCommand::RiceDraft { action }),
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

/// Dispatch ONE `ricemode` action: `lyra rice mode stage <name>` or a bare
/// `lyra rice mode declarative` (the CLI resolves the declared twin), waited
/// for through [`run_rice_step`]. The sibling `lyra` is the callee because
/// `rice` lives there, not in this running binary. NOT inside
/// `with_stage_lock`: holding it across a blocking child wait is a deadlock
/// waiting to happen. UNBOUNDED like every mutation here. `Ok` is the CLI's
/// own `message`; `Err` is its refusal or the spawn error.
fn dispatch_rice_mode(action: &RiceModeAction) -> Result<String, String> {
    let argv: Vec<String> = match action {
        RiceModeAction::Stage { name } => vec!["rice".into(), "mode".into(), "stage".into(), name.clone()],
        RiceModeAction::Declarative => vec!["rice".into(), "mode".into(), "declarative".into()],
    };
    match run_rice_step(&argv) {
        (true, message, _) => Ok(message),
        (false, message, _) => Err(message),
    }
}

/// Fire a detached `notify-send "Aoide" <message>` — the same reaper-thread
/// idiom as [`dispatch_power`]'s spawned child, so a slow or hung
/// `notify-send` can never block a connection. A spawn failure is logged and
/// nothing more: the action this toast reports already ran, and a dead or
/// missing `notify-send` must not turn its outcome into a different one.
fn notify(message: &str) {
    match std::process::Command::new("notify-send")
        .arg("Aoide")
        .arg(message)
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("[aoide/shellbridge] notify-send failed (the action's own outcome stands): {e}"),
    }
}

// ── the RICE menu (`ricemenu`, `ricedraft`) ───────────────────────────────

/// Pure plan: the `lyra` argv, in order and without `--json`, that one
/// `ricedraft` action takes in this mode, or the taught refusal that ends it.
/// `enter` and `new` route the stage into a draft, which `rice mode draft`
/// refuses while declarative is locked, so from `Declarative` they unlock
/// first — with a BARE `rice mode stage`, which restores the remembered
/// `stagingSong` (house rule 10), never the declared song. `save`
/// never changes the mode, and it is refused while declarative is locked: the
/// stage there is the DECLARED song (the lock re-pins it), `rice draft save`
/// nests under the song `stage/livery.json` names, and the listing
/// ([`drafts_song`]) names `stagingSong` — a save would land under a song the
/// picker never shows. The unlock is one click away, so the refusal costs one
/// click. `new` plans only the save: the draft's name is minted by the CLI and
/// is not known until that step has answered, so [`dispatch_rice_draft`]
/// appends the entering step itself.
fn rice_draft_plan(mode: RiceMode, action: &RiceDraftAction) -> Result<Vec<Vec<String>>, &'static str> {
    let step = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<String>>();
    let mut plan = Vec::new();
    if mode == RiceMode::Declarative {
        if *action == RiceDraftAction::Save {
            return Err("declarative mode is locked — unlock first, then save the draft");
        }
        plan.push(step(&["rice", "mode", "stage"]));
    }
    match action {
        RiceDraftAction::Enter { name } => plan.push(step(&["rice", "mode", "draft", name.as_str()])),
        RiceDraftAction::New | RiceDraftAction::Save => plan.push(step(&["rice", "draft", "save"])),
    }
    Ok(plan)
}

/// Run ONE planned step: `lyra <argv> --json`, waited for — the sibling `lyra` ([`daemon::bin::rice_bin`]) because `rice`
/// lives there and not in this running binary — and read through
/// [`outcome_triple`], the one reader of a `--json` envelope. NOT inside
/// `with_stage_lock` (a blocking child wait under it is a deadlock waiting to
/// happen). A child that
/// could not be started is `ok: false` with the spawn error as the message,
/// naming the first two argv words only — the command path, never a value.
fn run_rice_step(argv: &[String]) -> (bool, String, Option<Value>) {
    match std::process::Command::new(daemon::bin::rice_bin())
        .args(argv)
        .arg("--json")
        .output()
    {
        Ok(out) => outcome_triple(
            "rice",
            argv.get(1).map(String::as_str).unwrap_or(""),
            out.status.success(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        ),
        Err(e) => (
            false,
            format!(
                "spawning `lyra {} {}`: {e}",
                argv.first().map(String::as_str).unwrap_or(""),
                argv.get(1).map(String::as_str).unwrap_or("")
            ),
            None,
        ),
    }
}

/// Run ONE `ricedraft` action: [`rice_draft_plan`] for the current mode, each
/// step through [`run_rice_step`], stopping at the first that fails. `Ok` is
/// the last step's own CLI `message`; `Err((message, partial))` is the failing
/// step's (or the planner's refusal, before any step ran), with `partial` true
/// when an earlier step had already succeeded — the mode may be unlocked, or a
/// draft saved, and nothing here rolls it back: the toast says so, and the
/// click that follows is one gesture away.
///
/// `new` is the one action whose second step depends on the first's answer:
/// the CLI mints the name (`rice draft save` with none), and the entering
/// step takes that step's `data.name`. UNBOUNDED like every mutation here: one
/// per human gesture, and killing a half-applied one to tidy up is worse than
/// waiting.
fn dispatch_rice_draft(action: &RiceDraftAction) -> Result<String, (String, bool)> {
    let mut plan = rice_draft_plan(load_mode_marker().mode, action).map_err(|why| (why.to_string(), false))?;
    let mut said = String::new();
    let mut ran = 0;
    while ran < plan.len() {
        let (ok, message, data) = run_rice_step(&plan[ran]);
        if !ok {
            return Err((message, ran > 0));
        }
        if *action == RiceDraftAction::New && plan[ran] == ["rice", "draft", "save"] {
            let Some(name) = data.as_ref().and_then(|d| d.get("name")).and_then(Value::as_str) else {
                return Err(("`rice draft save` answered with no draft name".to_string(), true));
            };
            plan.push(vec!["rice".to_string(), "mode".to_string(), "draft".to_string(), name.to_string()]);
        }
        said = message;
        ran += 1;
    }
    Ok(said)
}

/// Pure decision: which song's drafts the menu lists, from the marker. The
/// menu's `enter` and `new` from `Declarative` unlock through a bare `rice
/// mode stage`, which lands on the remembered `stagingSong` — so that is the
/// song a declarative listing must name, ahead of `song` (the declared one the
/// lock overwrote). Its `save` is refused there ([`rice_draft_plan`]), so no
/// action ever writes under a song this listing does not name. Every other
/// mode is already working on `song`; `stagingSong` is only the fallback for a
/// marker that lacks it.
fn drafts_song(m: &ModeMarker) -> Option<String> {
    match m.mode {
        RiceMode::Declarative => m.staging_song.clone().or_else(|| m.song.clone()),
        RiceMode::Staging | RiceMode::Draft => m.song.clone().or_else(|| m.staging_song.clone()),
    }
}

/// The ONE line a `ricemenu` refusal answers with: the machine `reason`
/// (`cli-failed` — the CLI answered and refused; `no-answer` — nothing
/// readable came back) and a `message` that is the CLI's own where it has one.
fn rice_menu_refusal(reason: &str, message: &str) -> Value {
    json!({ "ok": false, "reason": reason, "message": message })
}

/// Run ONE bounded `lyra <argv> --json` and return its envelope's `data`, or
/// the refusal the menu answers with. `argv` carries no `--json`.
fn run_rice_json(argv: &[&str], timeout: std::time::Duration) -> Result<Value, Value> {
    let mut args: Vec<String> = argv.iter().map(|w| w.to_string()).collect();
    args.push("--json".to_string());
    let label = argv.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let (stdout, stderr) = match run_bin_bounded(&daemon::bin::rice_bin(), &args, timeout) {
        Ok((_exited_ok, stdout, stderr)) => (stdout, stderr),
        Err(message) => return Err(rice_menu_refusal("no-answer", &message)),
    };
    let Some((ok, message, data)) = outcome_envelope(&stdout).or_else(|| outcome_envelope(&stderr)) else {
        return Err(rice_menu_refusal("no-answer", &format!("`lyra {label}` printed no parseable answer")));
    };
    if !ok {
        return Err(rice_menu_refusal("cli-failed", &message));
    }
    data.ok_or_else(|| rice_menu_refusal("no-answer", &format!("`lyra {label}` answered with no data")))
}

/// Dispatch ONE read-only menu query: `lyra rice list --json` under
/// [`RICE_MENU_TIMEOUT`], its `data` (`mode`, `song`, `draft`, `stagingSong`,
/// `declared`, `songs`) passed through verbatim, then — when
/// [`drafts_song`] names a song for the marker — `lyra rice draft list <song>
/// --json` with the budget the first child left, projected to `draftsSong`
/// and `drafts: [{name, savedAt, current}]`. No song means `drafts: []` and
/// no second child; a failed list starts none either. A failed, unreadable
/// or out-of-budget child is a refusal ([`rice_menu_refusal`]), never an empty
/// answer — the menu must tell "nothing there" from "nobody answered".
fn dispatch_rice_menu() -> Value {
    let deadline = std::time::Instant::now() + RICE_MENU_TIMEOUT;
    let mut answer = match run_rice_json(&["rice", "list"], RICE_MENU_TIMEOUT) {
        Ok(Value::Object(map)) => map,
        Ok(_) => return rice_menu_refusal("no-answer", "`lyra rice list` answered with no object"),
        Err(refusal) => return refusal,
    };
    let marker = load_mode_marker();
    let song = drafts_song(&marker);
    let mut drafts: Vec<Value> = Vec::new();
    if let Some(song) = &song {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return rice_menu_refusal("no-answer", "the menu's time budget ran out before the drafts were listed");
        }
        let data = match run_rice_json(&["rice", "draft", "list", song.as_str()], left) {
            Ok(data) => data,
            Err(refusal) => return refusal,
        };
        let Some(listed) = data.get("drafts").and_then(Value::as_array) else {
            return rice_menu_refusal("no-answer", "`lyra rice draft list` answered with no `drafts`");
        };
        drafts = listed
            .iter()
            .map(|d| {
                let field = |key: &str| d.get(key).cloned().unwrap_or(Value::Null);
                json!({ "name": field("name"), "savedAt": field("savedAt"), "current": field("current") })
            })
            .collect();
    }
    answer.insert("ok".to_string(), Value::Bool(true));
    answer.insert("draftsSong".to_string(), json!(song));
    answer.insert("drafts".to_string(), Value::Array(drafts));
    Value::Object(answer)
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
/// inventing new copy (same discipline as [`dispatch_rice_mode`]).
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
/// Runs on a DETACHED thread. Unlike a rice-mode switch (a fast local one,
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
/// sibling resolver — never a bare `"aoide"` relying on PATH alone) under
/// [`run_bin_bounded`]'s wall-clock bound.
fn run_core_bounded(
    argv: &[String],
    timeout: std::time::Duration,
) -> Result<(bool, String, String), String> {
    run_bin_bounded(&daemon::bin::core_bin(), argv, timeout)
}

/// Run `argv` through `bin` — a sibling resolver's answer
/// (`daemon::bin::core_bin()` or `rice_bin()`), never a bare name relying on
/// PATH alone — with a WALL-CLOCK bound that actually holds.
/// `Ok((exited_ok, stdout, stderr))`, or
/// `Err(reason)` when no complete answer arrived: the child could not be
/// spawned, it outlived `timeout` (killed and REAPED here — `wait`, so no
/// zombie and no live child survives the bound), a DESCENDANT held its pipes
/// open past [`CHILD_DRAIN_GRACE`], or the read passed [`MAX_ANSWER_BYTES`].
/// A partial read is DISCARDED in every one of those cases, never parsed as if
/// it were whole. No shell is involved anywhere: `argv` is passed to
/// `Command::args` whole.
fn run_bin_bounded(
    bin: &str,
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
    let mut child = std::process::Command::new(bin)
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
                argv.first().map(String::as_str).unwrap_or(bin),
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
/// One raw line's `cmd` word, whatever else is wrong with it — how a verb
/// whose caller is PARKED on a reply recognises itself after `parse_command`
/// has already dropped the line (`sessiontrace`, `workspaceaction`). Total:
/// malformed JSON names nothing.
fn wire_names(line: &str, cmd: &str) -> bool {
    serde_json::from_str::<Value>(line.trim())
        .ok()
        .and_then(|v| v.get("cmd").and_then(Value::as_str).map(str::to_string))
        .as_deref()
        == Some(cmd)
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

/// One step's CLI outcome as `(ok, message, data)` — pure and total, the ONE
/// authority for reading a `--json` envelope into a reply, shared by every
/// acknowledged bridge action ([`session_action_reply`],
/// [`workspace_reply`]). Mirrors [`classify_recheck`]'s own rule: success is
/// the real output, not the exit status alone.
///
/// `--json` puts its envelope on a DIFFERENT stream depending on where the
/// command failed: a dispatched command prints it on stdout, but a usage
/// error the parser refused BEFORE dispatch prints it on stderr with an
/// empty stdout (`protocol/src/door.rs:800-814`), so reading stdout alone
/// would hand QML a raw JSON blob as its "message". `noun` is the word a
/// generated message uses when the envelope carries none of its own
/// (`"session"`/`"project"`/`"workspace"`); `action` the wire action name.
fn outcome_triple(
    noun: &str,
    action: &str,
    exited_ok: bool,
    stdout: &str,
    stderr: &str,
) -> (bool, String, Option<Value>) {
    match outcome_envelope(stdout).or_else(|| outcome_envelope(stderr)) {
        Some((status_ok, msg, data)) => {
            let ok = exited_ok && status_ok;
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
    }
}

/// Shape the ONE JSON reply line for an acknowledged bridge action.
/// `subject` is generic over `sessionaction`/`projectaction`
/// (P-14 M1 §2d) — a session subject reproduces every field byte-identical
/// to before that split.
fn session_action_reply(
    subject: ActionSubject,
    action: &str,
    exited_ok: bool,
    stdout: &str,
    stderr: &str,
) -> Value {
    let (ok, message, data) = outcome_triple(subject.noun(), action, exited_ok, stdout, stderr);
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

/// The ONE reply line for a `workspaceaction` —
/// `{ok, message, action, workspace, project?, data?}`. Same read as every
/// other acknowledged action ([`outcome_triple`], the CLI's own `message`
/// and `data` carried verbatim), keyed by the RESOLVED workspace id, so a
/// caller that omitted the workspace learns which one it actually touched.
/// `project` rides only on `set` — the one action that has one.
fn workspace_reply(
    binding: &WorkspaceBinding,
    ws: i64,
    exited_ok: bool,
    stdout: &str,
    stderr: &str,
) -> Value {
    let action = binding.as_str();
    let (ok, message, data) = outcome_triple("workspace", action, exited_ok, stdout, stderr);
    let mut reply = json!({
        "ok": ok,
        "message": message,
        "action": action,
        "workspace": ws,
    });
    if let WorkspaceBinding::Set { project, .. } = binding {
        reply["project"] = json!(project);
    }
    if let Some(data) = data {
        reply["data"] = data;
    }
    reply
}

/// One step, spawned: [`run_core_step`] the argv and shape the reply for
/// whichever subject owns the action.
fn run_session_step(subject: ActionSubject, action: &str, argv: &[String]) -> Value {
    match run_core_step(argv) {
        Ok((exited_ok, stdout, stderr)) => {
            session_action_reply(subject, action, exited_ok, &stdout, &stderr)
        }
        Err(message) => {
            let mut reply = json!({
                "ok": false,
                "message": message,
                "action": action,
            });
            reply[subject.key()] = json!(subject.value());
            reply
        }
    }
}

/// Exec the core `aoide` binary (`daemon::bin::core_bin()`, protocol's
/// sibling resolver — never a bare `"aoide"` relying on PATH alone) with
/// `argv` plus `--json`, and WAIT for it: `Ok((exited_ok, stdout, stderr))`,
/// or `Err(message)` when no child could be started at all. The failure
/// message names the first two argv elements only — always the command path,
/// never a value — the same discipline every audit line here holds.
///
/// UNBOUNDED, unlike [`run_core_bounded`]: this runs a mutation, one per
/// human gesture, so there is no caller cadence to bound against and no
/// partial answer worth discarding — killing a half-applied mutation to tidy
/// up a slow one is worse than waiting. This call is NOT inside
/// `with_stage_lock` — see `protocol::bin`'s module doc for why that would
/// matter if it ever were.
fn run_core_step(argv: &[String]) -> Result<(bool, String, String), String> {
    match std::process::Command::new(daemon::bin::core_bin())
        .args(argv)
        .arg("--json")
        .output()
    {
        Ok(out) => Ok((
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )),
        Err(e) => Err(format!(
            "spawning `aoide {} {}`: {e}",
            argv.first().map(String::as_str).unwrap_or(""),
            argv.get(1).map(String::as_str).unwrap_or("")
        )),
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
/// inside `aoided`; this line records only that the desk asked. `noun`
/// (`"session"`/`"project"`/`"workspace"`) is the only thing that varies
/// from the original `"session action …"` wording, so a session subject's
/// message is byte-identical to before the `projectaction` split.
fn audit_bridge_action(noun: &str, action: &str, event: &str, outcome: &str) {
    let _ = daemon::audit(
        &daemon::default_audit_log(),
        daemon::Door::Daemon,
        daemon::EventClass::Audit,
        "shellbridge",
        event,
        &format!("{noun} action {action}: {outcome}"),
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
                    subject.noun(),
                    action,
                    &format!("{}-failed", subject.event_prefix()),
                    "failed",
                );
                return reply;
            }
            let cli_message = reply.get("message").and_then(Value::as_str).unwrap_or("");
            audit_bridge_action(
                subject.noun(),
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
    audit_bridge_action(subject.noun(), action, subject.event_prefix(), "ok");
    last
}

/// Run ONE `workspaceaction` (W-P5): resolve an omitted workspace, plan the
/// CLI argv and exec it. The one compositor-shaped half is that resolution,
/// and it runs in THIS process — the bridge is the process that owns the
/// compositor adapter (`graph/window.rs`) and already spawns `hyprctl` for a
/// focus jump, and it resolves exactly what the CLI's own CALLER resolves when
/// a `<workspace>` is omitted, so the child always receives an integer. Why
/// here and not in the child: `aoide workspace clear` takes a workspace id, and
/// no wire caller can be expected to know the focused one. Nothing else is
/// resolved, folded or re-parsed — the CLI's binding, its refusals
/// (`unknown-project`, and `--new` on a name that is taken) and its own
/// message ARE the answer.
///
/// A host with no adapter is ANSWERED rather than dropped: `reason:
/// "no-compositor"` and the same sentence the CLI's own taught refusal
/// carries ([`NO_COMPOSITOR`], one authority for the text). A malformed LINE
/// is still dropped with no reply, like every other whitelist refusal — but a
/// well-formed click whose workspace merely cannot be resolved is parked on a
/// reply and gets one.
fn dispatch_workspace_action(workspace: Option<i64>, binding: &WorkspaceBinding) -> Value {
    let action = binding.as_str();
    let Some(ws) = workspace.or_else(focused_workspace) else {
        audit_bridge_action("workspace", action, "workspaceaction-failed", "failed");
        return json!({
            "ok": false,
            "action": action,
            "reason": "no-compositor",
            "message": NO_COMPOSITOR,
        });
    };
    let argv = binding.plan(ws);
    match run_core_step(&argv) {
        Ok((exited_ok, stdout, stderr)) => {
            let reply = workspace_reply(binding, ws, exited_ok, &stdout, &stderr);
            let ok = reply.get("ok").and_then(Value::as_bool).unwrap_or(false);
            audit_bridge_action(
                "workspace",
                action,
                if ok { "workspaceaction" } else { "workspaceaction-failed" },
                if ok { "ok" } else { "failed" },
            );
            reply
        }
        Err(message) => {
            audit_bridge_action("workspace", action, "workspaceaction-failed", "failed");
            json!({
                "ok": false,
                "message": message,
                "action": action,
                "workspace": ws,
            })
        }
    }
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
/// `dispatch_rice_mode`) — holding the stage lock across a blocking
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

/// LANE IDENTITY P-ID3 (G7) — the shellbridge socket's CROSS-USER floor, pure
/// and unit-tested without a real different-user connection (same shape
/// `aoide_secrets::broker::admin_gate` already holds: a peer credential in, an
/// `Option<String>` refusal reason out). `None` (admitted) only when the
/// peer's kernel-attested user equals THIS process's own — a uid on Unix, this
/// token's user SID on native Windows (`graph::identity::own_user`, ONE seam
/// for both sides of the comparison). shellbridge always runs as the
/// operator's own user, the SAME user every legitimate connector already runs
/// as (the QML herald/bar widgets, `aoide herald push` off dunst's script
/// hook, `session permit` raising its own summons — every one of them
/// same-user, none of them a DIFFERENT one). An unidentified peer — no
/// peer-credential mechanism, a failed read, or a peer whose pid was reused
/// mid-read — is refused the same fail-closed way a mismatched user is, never
/// treated as benign.
///
/// **This closes a CROSS-uid gap only — it does NOT stop a same-uid
/// attacker.** Under OQ1-A (LANE IDENTITY's thesis) every legitimate
/// connector above already shares this exact uid with anything hostile a
/// prompt-injected agent could run, so a same-uid process forging
/// `{"cmd":"heraldverdict",...}` is an OQ1-A-INHERENT residual this floor
/// does not close — see `CONTRACTS.md`'s identity section for the honest
/// statement of what remains open on the verdict door specifically.
fn cross_uid_gate(peer: Option<crate::graph::identity::PeerCred>) -> Option<String> {
    let own = crate::graph::identity::own_user();
    match (peer.and_then(|p| p.user()), own) {
        (Some(peer), Some(own)) if peer == own => None,
        (Some(peer), Some(own)) => Some(format!(
            "shellbridge connection refused: peer {} does not match this process's own {}",
            peer.label(),
            own.label()
        )),
        (_, None) => Some(
            "shellbridge connection refused: this process's own user could not be determined"
                .to_string(),
        ),
        (None, _) => Some(
            "shellbridge connection refused: the peer's user could not be determined (the kernel \
             identity read failed)"
                .to_string(),
        ),
    }
}

fn serve(listener: &UnixListener) {
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let peer = crate::graph::identity::peer_cred(&stream);
                if let Some(reason) = cross_uid_gate(peer) {
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
            Some(BridgeCommand::WorkspaceAction { workspace, binding }) => {
                // Same one-shot-connection discipline as the two actions
                // above: no reply channel means the action does not run.
                let action = binding.as_str();
                let Some(mut reply) = reply.take() else {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "workspaceaction-noreply",
                        &format!("workspace action {action}: no reply channel; not dispatched"),
                    );
                    return;
                };
                std::thread::spawn(move || {
                    use std::io::Write;
                    let _ = writeln!(reply, "{}", dispatch_workspace_action(workspace, &binding));
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
            // Fire-and-forget: the outcome is a toast, never a reply. The audit
            // line names the action and its outcome and nothing else — never a
            // song, which the toast alone carries.
            Some(BridgeCommand::RiceMode { action }) => {
                let (event, outcome) = match dispatch_rice_mode(&action) {
                    Ok(message) => {
                        notify(&message);
                        ("ricemode", "ok")
                    }
                    Err(message) => {
                        notify(&message);
                        ("ricemode-failed", "failed")
                    }
                };
                audit_bridge_action("ricemode", action.as_str(), event, outcome);
            }
            // Fire-and-forget like `ricemode`: the outcome is a toast, never a
            // reply, and the steps run inline for the same reason (a mode
            // change never takes this process down). The audit line names the
            // action and its outcome and nothing else — never a draft or a
            // song, which the toast alone carries.
            Some(BridgeCommand::RiceDraft { action }) => {
                let (event, outcome) = match dispatch_rice_draft(&action) {
                    Ok(message) => {
                        notify(&message);
                        ("ricedraft", "ok")
                    }
                    Err((message, true)) => {
                        notify(&format!("partial: {message}"));
                        ("ricedraft-partial", "partial")
                    }
                    Err((message, false)) => {
                        notify(&message);
                        ("ricedraft-failed", "failed")
                    }
                };
                audit_bridge_action("ricedraft", action.as_str(), event, outcome);
            }
            // The menu's read: answered on this connection and run on its OWN
            // thread, like `sessiontrace`, because it re-execs the CLI under a
            // bound. No reply channel means the query does not run at all.
            // Only a REFUSAL is audited — a read is not a gesture.
            Some(BridgeCommand::RiceMenu) => {
                let Some(mut reply) = reply.take() else {
                    let _ = daemon::audit(
                        &daemon::default_audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "ricemenu-noreply",
                        "ricemenu: no reply channel; not dispatched",
                    );
                    return;
                };
                std::thread::spawn(move || {
                    use std::io::Write;
                    let answer = dispatch_rice_menu();
                    if answer.get("ok").and_then(Value::as_bool) != Some(true) {
                        let reason = answer.get("reason").and_then(Value::as_str).unwrap_or("failed");
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "ricemenu-refused",
                            &format!("ricemenu refused: {reason}"),
                        );
                    }
                    let _ = writeln!(reply, "{answer}");
                });
                return;
            }
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
                // The verbs whose caller is PARKED waiting for an answer —
                // `sessiontrace`, `workspaceaction` — never leave it there on
                // a malformed line: a line that NAMES one of them and fails
                // its own gate is answered with a refusal instead of silence
                // (the widgets' own clients normalise what they send, so this
                // is the door's honesty, not its fast path). ONE rule for
                // every parked caller; a fire-and-forget verb keeps the plain
                // drop above.
                if wire_names(&line, "sessiontrace") {
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
                if wire_names(&line, "workspaceaction") {
                    if let Some(mut reply) = reply.take() {
                        // Nothing parsed, so the answer names nothing back:
                        // no `workspace` (none was resolved), no `project`,
                        // and no `action` word to echo — the same reason
                        // `trace_refusal` echoes only what the CLI resolved.
                        let answer = json!({
                            "ok": false,
                            "reason": "bad-request",
                            "message": "a workspaceaction line takes action set|clear, an \
                                        integer `workspace` (omitted = the focused one), and \
                                        for set a project name with an optional bool `new`",
                        });
                        let _ = daemon::audit(
                            &daemon::default_audit_log(),
                            daemon::Door::Daemon,
                            daemon::EventClass::Audit,
                            "shellbridge",
                            "workspaceaction-refused",
                            "workspace action refused: bad-request",
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
    fn cross_uid_gate_admits_a_matching_user() {
        let own = crate::graph::identity::own_user().expect("this process's own user");
        let peer = crate::graph::identity::PeerCred::for_user(&own);
        assert_eq!(cross_uid_gate(Some(peer)), None);
    }

    #[test]
    fn cross_uid_gate_refuses_a_mismatched_user() {
        let other = match crate::graph::identity::own_user().expect("this process's own user") {
            crate::graph::identity::PeerUser::Uid(uid) => crate::graph::identity::PeerUser::Uid(uid + 1),
            crate::graph::identity::PeerUser::Sid(sid) => {
                crate::graph::identity::PeerUser::Sid(format!("{sid}-not-this-user"))
            }
        };
        let peer = crate::graph::identity::PeerCred::for_user(&other);
        assert!(cross_uid_gate(Some(peer)).is_some());
    }

    #[test]
    fn cross_uid_gate_refuses_an_unidentified_peer() {
        // Fail-closed, never a benign default — the same posture
        // `admin_gate` holds for a peer-identity read that failed.
        assert!(cross_uid_gate(None).is_some());
    }

    /// End-to-end against a REAL socketpair (mirrors `identity.rs`'s own
    /// `peer_cred_on_a_scratch_socketpair_matches_this_processs_own_identity`):
    /// a connection entirely local to this process reports THIS process's
    /// own user, which `cross_uid_gate` then admits — proving the floor
    /// does not refuse the legitimate same-user caller (the desktop QML, the
    /// dunst hook, `session permit`'s own raise — every real connector Phase
    /// 0 identified) it must never touch. Both hosts run this one: the
    /// socketpair is the socket-type seam, the identity is the peer-cred seam.
    #[test]
    fn a_real_same_process_socketpair_is_admitted() {
        let (a, _b) = UnixStream::pair().expect("socketpair");
        let peer = crate::graph::identity::peer_cred(&a);
        assert_eq!(cross_uid_gate(peer), None);
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
    fn lock_is_started_by_the_compositor_never_as_this_units_child() {
        let (prog, args) = PowerAction::Lock.command();
        assert_eq!((prog, args), ("hyprctl", &["dispatch", "exec", "hyprlock"][..]));
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
    fn parse_command_accepts_ricemenu_and_both_ricemode_shapes() {
        assert_eq!(parse_command(r#"{"cmd":"ricemenu"}"#), Some(BridgeCommand::RiceMenu));
        assert_eq!(parse_command("  {\"cmd\":\"ricemenu\"}\n"), Some(BridgeCommand::RiceMenu));
        assert_eq!(
            parse_command(r#"{"cmd":"ricemode","action":"stage","name":"fugue"}"#),
            Some(BridgeCommand::RiceMode { action: RiceModeAction::Stage { name: "fugue".to_string() } })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricemode","action":"declarative"}"#),
            Some(BridgeCommand::RiceMode { action: RiceModeAction::Declarative })
        );
    }

    #[test]
    fn parse_command_refuses_every_malformed_ricemode_and_the_retired_verbs() {
        for line in [
            r#"{"cmd":"ricemode"}"#,
            r#"{"cmd":"ricemode","action":"stage"}"#,
            r#"{"cmd":"ricemode","action":"stage","name":"-x"}"#,
            r#"{"cmd":"ricemode","action":"stage","name":7}"#,
            r#"{"cmd":"ricemode","action":"declarative","name":"fugue"}"#,
            r#"{"cmd":"ricemode","action":"toggle"}"#,
            r#"{"cmd":"ricemode","action":42}"#,
            r#"{"cmd":"ricedrafts"}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line}");
        }
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
        assert!(wire_names(
            r#"{"cmd":"sessiontrace","sessionId":"s1","clip":"detials"}"#,
            "sessiontrace"
        ));
        assert!(wire_names(r#"{"cmd":"sessiontrace"}"#, "sessiontrace"));
        assert!(!wire_names(r#"{"cmd":"focussession"}"#, "sessiontrace"));
        assert!(!wire_names("{ not json", "sessiontrace"));
        // The same pure question, for the OTHER parked verb — a
        // `workspaceaction` line names itself whatever else it got wrong.
        assert!(wire_names(
            r#"{"cmd":"workspaceaction","action":"clear","project":"aoide"}"#,
            "workspaceaction"
        ));
        assert!(!wire_names(r#"{"cmd":"sessionaction"}"#, "workspaceaction"));
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
    /// GATED on Unix with its reason: the stub is a `#!/bin/sh` script
    /// `CreateProcess` cannot launch (no shebang, no extension), and this test
    /// asserts the argv the shim echoed — the same gate the client crate's
    /// shim groups carry.
    #[cfg(unix)]
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
    /// GATED on Unix with its reason: the stub is a `#!/bin/sh` script
    /// `CreateProcess` cannot launch (no shebang, no extension) — the same
    /// gate the client crate's shim groups carry.
    #[cfg(unix)]
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
    /// GATED on Unix with its reason: the fixture is a `#!/bin/sh` stub whose
    /// `exec sleep` is what gives the bound something to kill, and the
    /// descendant's liveness is read off `/proc` — neither exists on native
    /// Windows (`CreateProcess` runs no script, and the pid's end there is
    /// uncatchable `TerminateProcess`).
    #[cfg(unix)]
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
    /// GATED on Unix with its reason: the fixture is a `#!/bin/sh` stub whose
    /// `sleep 30 &` descendant holds the pipe open, and the cleanup SIGKILLs
    /// that pid — `CreateProcess` runs no script there, and no shell builtin
    /// spawns the descendant this test needs.
    #[cfg(unix)]
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
    fn parse_command_accepts_every_ricedraft_shape() {
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"enter","name":"draft-2"}"#),
            Some(BridgeCommand::RiceDraft {
                action: RiceDraftAction::Enter { name: "draft-2".to_string() }
            })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"new"}"#),
            Some(BridgeCommand::RiceDraft { action: RiceDraftAction::New })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"save"}"#),
            Some(BridgeCommand::RiceDraft { action: RiceDraftAction::Save })
        );
    }

    #[test]
    fn parse_command_refuses_a_ricedraft_it_cannot_name() {
        for line in [
            // `enter` needs a name that is one argv token.
            r#"{"cmd":"ricedraft","action":"enter"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":""}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"-x"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"a b"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"a\n"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":7}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":null}"#,
            // `new` and `save` take none: a name beside them is a caller that
            // meant `enter`, and ignoring it would read as success.
            r#"{"cmd":"ricedraft","action":"new","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"save","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"save","name":null}"#,
            // a closed set
            r#"{"cmd":"ricedraft","action":"drop","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"Enter","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":7}"#,
            r#"{"cmd":"ricedraft","action":null}"#,
            r#"{"cmd":"ricedraft"}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line}");
        }
    }

    #[test]
    fn rice_draft_plan_unlocks_declarative_for_enter_and_new_and_refuses_its_save() {
        let enter = RiceDraftAction::Enter { name: "draft-2".to_string() };
        let unlock = sv(&["rice", "mode", "stage"]);
        let save = sv(&["rice", "draft", "save"]);
        let go = sv(&["rice", "mode", "draft", "draft-2"]);

        // Declarative: `enter` and `new` unlock FIRST, bare — never a song.
        assert_eq!(rice_draft_plan(RiceMode::Declarative, &enter), Ok(vec![unlock.clone(), go.clone()]));
        assert_eq!(rice_draft_plan(RiceMode::Declarative, &RiceDraftAction::New), Ok(vec![unlock, save.clone()]));
        // `save` there would nest under the DECLARED song, which the listing
        // (the staged one) never shows: a taught refusal, nothing planned.
        assert_eq!(
            rice_draft_plan(RiceMode::Declarative, &RiceDraftAction::Save),
            Err("declarative mode is locked — unlock first, then save the draft")
        );
        // Staging and Draft are already unlocked: one step. `new` plans the
        // save alone — the entering step needs the name that step mints.
        for mode in [RiceMode::Staging, RiceMode::Draft] {
            assert_eq!(rice_draft_plan(mode, &enter), Ok(vec![go.clone()]), "{mode:?}");
            assert_eq!(rice_draft_plan(mode, &RiceDraftAction::New), Ok(vec![save.clone()]), "{mode:?}");
            assert_eq!(rice_draft_plan(mode, &RiceDraftAction::Save), Ok(vec![save.clone()]), "{mode:?}");
        }
    }

    #[test]
    fn drafts_song_names_the_song_the_mode_will_work_on() {
        let marker = |mode, song: Option<&str>, staging: Option<&str>| ModeMarker {
            mode,
            song: song.map(str::to_string),
            staging_song: staging.map(str::to_string),
            ..ModeMarker::default()
        };
        // Declarative: the lock overwrote `song` with the declared one; the
        // unlock lands on `stagingSong`, so that is what the picker lists.
        let declarative = marker(RiceMode::Declarative, Some("sonata"), Some("cadenza"));
        assert_eq!(drafts_song(&declarative).as_deref(), Some("cadenza"));
        assert_eq!(drafts_song(&marker(RiceMode::Declarative, Some("sonata"), None)).as_deref(), Some("sonata"));
        // Staging and Draft already work on `song`.
        for mode in [RiceMode::Staging, RiceMode::Draft] {
            assert_eq!(drafts_song(&marker(mode, Some("sonata"), Some("cadenza"))).as_deref(), Some("sonata"));
            assert_eq!(drafts_song(&marker(mode, None, Some("cadenza"))).as_deref(), Some("cadenza"));
            assert_eq!(drafts_song(&marker(mode, None, None)), None);
        }
        assert_eq!(drafts_song(&marker(RiceMode::Declarative, None, None)), None);
    }

    /// A `#!/bin/sh` stand-in for `lyra`, a stage dir holding a marker for
    /// `mode`, and the env that points both at them. It logs every argv it is
    /// given beside itself, and answers the verbs the menu plans with
    /// the envelopes the CLI prints: `list` echoes a fixed songbook, `draft save` mints `draft-3`, `draft list`
    /// lists two drafts, anything else succeeds. A `refuse-draft`,
    /// `refuse-list` or `refuse-songs` file beside it turns that verb into a refusal. NO notify
    /// is ever reached — the tests drive the sequencer, not `handle_conn`.
    ///
    /// GATED on Unix with its reason (its callers are gated with it): a
    /// `#!/bin/sh` script made executable by a mode, which `CreateProcess`
    /// launches neither by shebang nor by an extension-less name.
    #[cfg(unix)]
    fn stub_rice_bin(tag: &str, marker: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
        let root = aoide_test_support::unique_tmp(tag);
        std::fs::create_dir_all(&root).unwrap();
        let stub = root.join("stub-lyra");
        std::fs::write(
            &stub,
            r#"#!/bin/sh
here="$(dirname "$0")"
echo "$*" >> "$here/argv.log"
case "$2 $3" in
"list --json")
  if [ -f "$here/refuse-songs" ]; then
    printf '{"status":"error","message":"songbook unreadable","data":{"reason":"x"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"2 song(s)","data":{"mode":"declarative","song":"sonata","draft":null,"stagingSong":"cadenza","declared":"sonata","songs":[{"name":"cadenza","ok":true},{"name":"sonata","ok":true}]}}\n' ;;
"draft save")
  printf '{"status":"ok","message":"saved","data":{"name":"draft-3"}}\n' ;;
"draft list")
  if [ -f "$here/refuse-list" ]; then
    printf '{"status":"error","message":"no song staged","data":{"reason":"x"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"2 draft(s)","data":{"drafts":[{"song":"%s","name":"draft-1","savedAt":"2026-01-01T00:00:00Z","current":false},{"song":"%s","name":"draft-2","savedAt":null,"current":true}],"scope":"%s"}}\n' "$4" "$4" "$4" ;;
"mode draft")
  if [ -f "$here/refuse-draft" ]; then
    printf '{"status":"error","message":"no song is currently staged","data":{"reason":"no-resolvable-song"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"routed %s","data":{}}\n' "$4" ;;
*)
  printf '{"status":"ok","message":"staged","data":{}}\n' ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::fs::write(root.join("mode.json"), marker).unwrap();
        let env = aoide_test_support::EnvSaver::capture(&["AOIDE_RICE_BIN", "AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_RICE_BIN", &stub);
        std::env::set_var("AOIDE_STAGE_DIR", &root);
        (env, root)
    }

    #[cfg(unix)]
    fn stub_argv_log(root: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(root.join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_new_draft_from_declarative_unlocks_saves_and_enters_the_name_the_cli_minted() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-draft-new",
            r#"{"mode":"declarative","song":"sonata","stagingSong":"cadenza"}"#,
        );
        assert_eq!(dispatch_rice_draft(&RiceDraftAction::New), Ok("routed draft-3".to_string()));
        assert_eq!(
            stub_argv_log(&root),
            ["rice mode stage --json", "rice draft save --json", "rice mode draft draft-3 --json"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn an_enter_from_draft_is_one_step_and_a_save_from_declarative_is_refused_unrun() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-draft-one", r#"{"mode":"draft","song":"sonata","draft":"draft-1"}"#);
        let enter = RiceDraftAction::Enter { name: "draft-2".to_string() };
        assert_eq!(dispatch_rice_draft(&enter), Ok("routed draft-2".to_string()));
        assert_eq!(stub_argv_log(&root), ["rice mode draft draft-2 --json"]);

        std::fs::write(root.join("mode.json"), r#"{"mode":"declarative","song":"sonata"}"#).unwrap();
        std::fs::remove_file(root.join("argv.log")).unwrap();
        // The refusal is the first thing that happens: not partial, no child.
        assert_eq!(
            dispatch_rice_draft(&RiceDraftAction::Save),
            Err(("declarative mode is locked — unlock first, then save the draft".to_string(), false))
        );
        assert!(stub_argv_log(&root).is_empty(), "a refused save starts no child");

        std::fs::write(root.join("mode.json"), r#"{"mode":"staging","song":"sonata"}"#).unwrap();
        assert_eq!(dispatch_rice_draft(&RiceDraftAction::Save), Ok("saved".to_string()));
        assert_eq!(stub_argv_log(&root), ["rice draft save --json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_failing_later_step_is_partial_and_a_failing_first_step_is_not() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-draft-partial",
            r#"{"mode":"declarative","song":"sonata"}"#,
        );
        std::fs::write(root.join("refuse-draft"), "").unwrap();
        let enter = RiceDraftAction::Enter { name: "nope".to_string() };
        // The unlock ran and the entering step refused: the mode is now
        // unlocked, and the answer says so rather than reading as a clean no.
        assert_eq!(dispatch_rice_draft(&enter), Err(("no song is currently staged".to_string(), true)));
        assert_eq!(stub_argv_log(&root), ["rice mode stage --json", "rice mode draft nope --json"]);

        // Already unlocked: the same refusal is the FIRST step, nothing ran.
        std::fs::write(root.join("mode.json"), r#"{"mode":"staging","song":"sonata"}"#).unwrap();
        assert_eq!(dispatch_rice_draft(&enter), Err(("no song is currently staged".to_string(), false)));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_menu_merges_the_songbook_with_the_modes_song_drafts() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-menu-list",
            r#"{"mode":"declarative","song":"sonata","stagingSong":"cadenza"}"#,
        );
        assert_eq!(
            dispatch_rice_menu(),
            json!({
                "ok": true,
                "mode": "declarative",
                "song": "sonata",
                "draft": null,
                "stagingSong": "cadenza",
                "declared": "sonata",
                "songs": [{ "name": "cadenza", "ok": true }, { "name": "sonata", "ok": true }],
                "draftsSong": "cadenza",
                "drafts": [
                    { "name": "draft-1", "savedAt": "2026-01-01T00:00:00Z", "current": false },
                    { "name": "draft-2", "savedAt": null, "current": true },
                ],
            })
        );
        assert_eq!(stub_argv_log(&root), ["rice list --json", "rice draft list cadenza --json"]);

        // A CLI that refuses is a refusal carrying its own words — never an
        // empty list the menu would paint as "no drafts yet".
        std::fs::write(root.join("refuse-list"), "").unwrap();
        assert_eq!(
            dispatch_rice_menu(),
            json!({ "ok": false, "reason": "cli-failed", "message": "no song staged" })
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_failed_rice_list_is_a_refusal_and_starts_no_second_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-menu-nolist", r#"{"mode":"staging","song":"sonata"}"#);
        std::fs::write(root.join("refuse-songs"), "").unwrap();
        assert_eq!(
            dispatch_rice_menu(),
            json!({ "ok": false, "reason": "cli-failed", "message": "songbook unreadable" })
        );
        assert_eq!(stub_argv_log(&root), ["rice list --json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_menu_with_no_song_answers_no_drafts_after_one_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-menu-none", r#"{"mode":"staging"}"#);
        let reply = dispatch_rice_menu();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["draftsSong"], Value::Null);
        assert_eq!(reply["drafts"], json!([]));
        assert_eq!(reply["songs"].as_array().map(Vec::len), Some(2));
        assert_eq!(stub_argv_log(&root), ["rice list --json"], "no song means no drafts child");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_mode_names_the_song_to_stage_and_none_for_declarative() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-mode", r#"{"mode":"staging","song":"sonata"}"#);
        assert_eq!(
            dispatch_rice_mode(&RiceModeAction::Stage { name: "fugue".to_string() }),
            Ok("staged".to_string())
        );
        assert_eq!(dispatch_rice_mode(&RiceModeAction::Declarative), Ok("staged".to_string()));
        assert_eq!(
            stub_argv_log(&root),
            ["rice mode stage fugue --json", "rice mode declarative --json"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The menu over a REAL socket: `ricemenu` is answered with one line
    /// on its own connection and the audit log never learns a song or draft
    /// name; a `ricedraft` line the gate refuses changes nothing and gets NO
    /// reply — it is fire-and-forget, so nothing is parked on one — beyond
    /// the `unparseable` byte count. (No valid `ricedraft` is sent: it would
    /// toast through the real `notify-send`.)
    ///
    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn ricemenu_is_answered_over_the_socket_and_a_refused_ricedraft_is_not() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_stub_env, root) = stub_rice_bin(
            "shellbridge-menu-sock",
            r#"{"mode":"staging","song":"quietsong"}"#,
        );
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_AUDIT_LOG", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let sock = short_sock("menu");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener)); // never joined

        use std::io::Write;
        let mut refused = UnixStream::connect(&sock).unwrap();
        refused.write_all(b"{\"cmd\":\"ricedraft\",\"action\":\"enter\"}\n").unwrap();
        refused.flush().unwrap();
        refused.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
        let mut silence = String::new();
        let heard = BufReader::new(&refused).read_line(&mut silence);
        assert!(
            heard.is_err() || silence.is_empty(),
            "a refused ricedraft is never answered: {silence:?}"
        );

        let mut client = UnixStream::connect(&sock).unwrap();
        client.write_all(b"{\"cmd\":\"ricemenu\"}\n").unwrap();
        client.flush().unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut answer = String::new();
        BufReader::new(client).read_line(&mut answer).unwrap();
        let reply: Value = serde_json::from_str(answer.trim()).expect("one JSON reply line");
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["draftsSong"], "quietsong");
        assert_eq!(reply["songs"].as_array().map(Vec::len), Some(2), "{reply}");
        assert_eq!(reply["drafts"].as_array().map(Vec::len), Some(2), "{reply}");

        let log = root.join("log");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("unparseable") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(text.contains("unparseable"), "{text}");
        assert!(!text.contains("quietsong") && !text.contains("draft-"), "no names in the audit: {text}");

        let _ = std::fs::remove_dir_all(&root);
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

    #[test]
    fn parse_command_accepts_every_workspaceaction_shape() {
        // `set`, with an explicit workspace.
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"set","workspace":3,"project":"aoide"}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: Some(3),
                binding: WorkspaceBinding::Set {
                    project: "aoide".to_string(),
                    new: false,
                },
            })
        );
        // `set --new`, with no workspace at all — "the focused one".
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"set","project":"cadenza","new":true}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: None,
                binding: WorkspaceBinding::Set {
                    project: "cadenza".to_string(),
                    new: true,
                },
            })
        );
        // An explicit `false` is the same ask as an absent `new`.
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"set","project":"aoide","new":false}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: None,
                binding: WorkspaceBinding::Set {
                    project: "aoide".to_string(),
                    new: false,
                },
            })
        );
        // `clear`, explicit workspace, and `clear` with an explicit `null`
        // workspace — read as "no workspace" (the focused one), the same
        // tolerance `sessiontrace`'s `lines` holds for a null.
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"clear","workspace":3}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: Some(3),
                binding: WorkspaceBinding::Clear,
            })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"clear","workspace":null}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: None,
                binding: WorkspaceBinding::Clear,
            })
        );
        // A NEGATIVE id is a real workspace — Hyprland's named and special
        // workspaces carry them, and the CLI's own `workspace_id` parses the
        // same `i64` (the focused-workspace read surfaces one verbatim).
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"set","workspace":-99,"project":"aoide"}"#),
            Some(BridgeCommand::WorkspaceAction {
                workspace: Some(-99),
                binding: WorkspaceBinding::Set {
                    project: "aoide".to_string(),
                    new: false,
                },
            })
        );
        // Ordinary spaces and surrounding whitespace are legal in a name
        // (`safe_action_value`); no session id appears anywhere.
        match parse_command(
            r#"  {"cmd":"workspaceaction","action":"set","workspace":1,"project":" My Project "}"#,
        ) {
            Some(BridgeCommand::WorkspaceAction { binding, .. }) => assert_eq!(
                binding,
                WorkspaceBinding::Set {
                    project: "My Project".to_string(),
                    new: false,
                }
            ),
            other => panic!("expected a WorkspaceAction, got {other:?}"),
        }
    }

    #[test]
    fn parse_command_refuses_a_workspace_action_it_cannot_name() {
        // A closed set of TWO: anything else — including a plausible synonym
        // — is refused at the wire, never dispatched as a default.
        for action in ["unset", "bind", "SET", "", "clearall"] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"workspaceaction","action":"{action}","project":"aoide"}}"#
                )),
                None,
                "{action} must be refused"
            );
        }
        // `set` needs a name that passes the existing name rule: a missing
        // key, `null`, a non-string, an empty/blank string and a
        // flag-shaped one are all refused — there is no reading of a
        // malformed bind request as anything else.
        for fields in [
            r#""workspace":3"#,
            r#""workspace":3,"project":null"#,
            r#""workspace":3,"project":5"#,
            r#""workspace":3,"project":"""#,
            r#""workspace":3,"project":"   ""#,
            r#""workspace":3,"project":"-rf""#,
        ] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"workspaceaction","action":"set",{fields}}}"#
                )),
                None,
                "{fields} must be refused"
            );
        }
        // `new` must BE a bool — a string or a number is refused, never read
        // as false.
        for new in [r#""yes""#, r#"1"#, r#"{}"#] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"workspaceaction","action":"set","workspace":3,"project":"aoide","new":{new}}}"#
                )),
                None,
                "new={new} must be refused"
            );
        }
        // A `clear` carrying a binding field is a caller that meant `set`:
        // refused by name, so the mistake can never read as a successful
        // unbind.
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"clear","workspace":3,"project":"aoide"}"#),
            None
        );
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","action":"clear","workspace":3,"new":true}"#),
            None
        );
        // A workspace id is an INTEGER: a string, a bool and a float are all
        // refused, never coerced.
        for ws in [r#""3""#, r#"true"#, r#"3.5"#, r#"[3]"#] {
            assert_eq!(
                parse_command(&format!(
                    r#"{{"cmd":"workspaceaction","action":"set","workspace":{ws},"project":"aoide"}}"#
                )),
                None,
                "workspace={ws} must be refused"
            );
        }
        // No action at all is refused too.
        assert_eq!(
            parse_command(r#"{"cmd":"workspaceaction","workspace":3,"project":"aoide"}"#),
            None
        );
        // And a blank/absent `cmd` is not this command.
        assert_eq!(parse_command(r#"{"cmd":"workspace","action":"set"}"#), None);
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

    // -- workspaceaction, the bar's bind click (W-P5) --

    /// A stub `aoide` on `AOIDE_CORE_BIN` (the tier `bin::core_bin` resolves
    /// first) that answers with a CLI-shaped `ok` envelope quoting the WHOLE
    /// argv it was handed, so a dispatch test asserts both the planned argv
    /// and the reply shaping with no daemon and no stage write.
    ///
    /// GATED on Unix with its reason (its callers are gated with it): the stub
    /// is a `#!/bin/sh` SCRIPT made executable by a mode, and `CreateProcess`
    /// understands neither a shebang nor an extension-less name — the same
    /// gate the client crate's own shim groups carry.
    #[cfg(unix)]
    fn stub_workspace_core(tag: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
        let root = aoide_test_support::unique_tmp(tag);
        std::fs::create_dir_all(&root).unwrap();
        let stub = root.join("stub-aoide");
        std::fs::write(
            &stub,
            r#"#!/bin/sh
printf '{"status":"ok","command":"workspace","message":"argv: %s","data":{"workspace":%s}}\n' "$*" "$3"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        std::env::set_var("AOIDE_CORE_BIN", &stub);
        (env, root)
    }

    #[test]
    fn workspace_binding_plans_the_exact_argv_for_each_shape() {
        let set = WorkspaceBinding::Set {
            project: "aoide".to_string(),
            new: false,
        };
        assert_eq!(set.plan(3), sv(&["workspace", "set", "3", "aoide"]));
        // `--new` rides LAST, exactly where the CLI's own examples put it,
        // and ONLY when asked for.
        let set_new = WorkspaceBinding::Set {
            project: "cadenza".to_string(),
            new: true,
        };
        assert_eq!(set_new.plan(3), sv(&["workspace", "set", "3", "cadenza", "--new"]));
        assert_eq!(WorkspaceBinding::Clear.plan(3), sv(&["workspace", "clear", "3"]));
        // A negative (special) workspace id goes through verbatim, and a name
        // with a space stays ONE argv element — no shell anywhere.
        assert_eq!(set.plan(-99), sv(&["workspace", "set", "-99", "aoide"]));
        let spaced = WorkspaceBinding::Set {
            project: "My Project".to_string(),
            new: false,
        };
        assert_eq!(spaced.plan(1), sv(&["workspace", "set", "1", "My Project"]));
    }

    /// GATED on Unix with its reason (see [`stub_workspace_core`]): the
    /// fixture is a `#!/bin/sh` shim `CreateProcess` cannot launch.
    #[cfg(unix)]
    #[test]
    fn dispatch_workspace_action_runs_the_planned_argv_and_keys_the_reply() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_workspace_core("shellbridge-ws-set");

        let reply = dispatch_workspace_action(
            Some(3),
            &WorkspaceBinding::Set {
                project: "aoide".to_string(),
                new: false,
            },
        );
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["action"], "set");
        assert_eq!(reply["workspace"], 3);
        assert_eq!(reply["project"], "aoide");
        // The CLI's own message and `data` are carried verbatim, and the
        // argv the child actually got is the plan plus `--json`.
        assert_eq!(reply["message"], "argv: workspace set 3 aoide --json");
        assert_eq!(reply["data"], json!({"workspace": 3}));

        // `--new` is planned, and a clear carries no `project` key at all.
        let reply = dispatch_workspace_action(
            Some(5),
            &WorkspaceBinding::Set {
                project: "cadenza".to_string(),
                new: true,
            },
        );
        assert_eq!(reply["message"], "argv: workspace set 5 cadenza --new --json");

        let reply = dispatch_workspace_action(Some(3), &WorkspaceBinding::Clear);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["action"], "clear");
        assert_eq!(reply["workspace"], 3);
        assert_eq!(reply["message"], "argv: workspace clear 3 --json");
        assert!(
            reply.get("project").is_none(),
            "a clear has no project to name: {reply}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see `testutil::fake_hyprctl`): the
    /// fixture is a `#!/bin/sh` shim `CreateProcess` cannot launch, over a
    /// compositor native Windows does not run.
    #[cfg(unix)]
    #[test]
    fn dispatch_workspace_action_resolves_an_omitted_workspace_through_the_adapter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_workspace_core("shellbridge-ws-focused");
        let (hypr, shim) = crate::graph::testutil::fake_hyprctl("ws-action-focused");
        std::env::set_var("AOIDE_TEST_WS", "7");

        // Both verbs resolve the FOCUSED workspace, so `clear` (which takes an
        // id and no focused default of its own) is reachable from a click
        // with no workspace on the wire.
        let reply = dispatch_workspace_action(
            None,
            &WorkspaceBinding::Set {
                project: "aoide".to_string(),
                new: false,
            },
        );
        assert_eq!(reply["workspace"], 7, "{reply}");
        assert_eq!(reply["message"], "argv: workspace set 7 aoide --json");

        let reply = dispatch_workspace_action(None, &WorkspaceBinding::Clear);
        assert_eq!(reply["workspace"], 7, "{reply}");
        assert_eq!(reply["message"], "argv: workspace clear 7 --json");

        drop(hypr);
        let _ = std::fs::remove_dir_all(&shim);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_workspace_action_answers_a_host_with_no_compositor() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _sig = crate::graph::testutil::EnvVars::save(&["HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        // A core binary that cannot exist: if this refusal were ever a spawn
        // attempt, the reply would say so instead.
        let _bin = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        std::env::set_var("AOIDE_CORE_BIN", "/nonexistent/aoide");

        let reply = dispatch_workspace_action(None, &WorkspaceBinding::Clear);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["action"], "clear");
        assert_eq!(reply["reason"], "no-compositor");
        // The CLI's own sentence, verbatim — one authority for the text.
        assert_eq!(reply["message"], NO_COMPOSITOR);
        assert!(reply.get("workspace").is_none(), "nothing was resolved: {reply}");
    }

    #[test]
    fn workspace_reply_carries_an_error_envelope_and_falls_back_honestly() {
        let clear = WorkspaceBinding::Clear;
        // An error envelope still makes a reply, not an ok.
        let reply = workspace_reply(
            &clear,
            3,
            false,
            r#"{"status":"error","command":"workspace.clear","message":"aoided must be running for project management"}"#,
            "",
        );
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["message"], "aoided must be running for project management");
        assert_eq!(reply["action"], "clear");
        assert_eq!(reply["workspace"], 3);

        // A usage envelope lands on stderr when the parser refused the argv
        // before dispatch — read there, never mistaken for gibberish.
        let reply = workspace_reply(
            &clear,
            3,
            false,
            "",
            r#"{"status":"usage","command":"workspace.clear","message":"`x` is not a workspace id"}"#,
        );
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["message"], "`x` is not a workspace id");

        // Neither stream an envelope: the raw line is the message, and an
        // empty outcome still says something true.
        let reply = workspace_reply(&clear, 3, false, "boom", "");
        assert_eq!(reply["message"], "boom");
        let reply = workspace_reply(&clear, 3, false, "", "");
        assert_ne!(reply["message"].as_str().unwrap_or(""), "");

        // One wire line, always.
        let reply = workspace_reply(
            &WorkspaceBinding::Set {
                project: "a\nb".to_string(),
                new: false,
            },
            3,
            true,
            r#"{"status":"ok","command":"workspace.set","message":"a\nb"}"#,
            "",
        );
        assert!(!reply.to_string().contains('\n'));
        assert_eq!(reply["project"], "a\nb");
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

    /// A socket path SHORT enough for this host's `sun_path` budget. The three
    /// fixtures below bind a REAL socket, and native Windows caps an AF_UNIX
    /// path at 107 bytes (`sun_path` is 108 with its terminator) — measured, on
    /// the per-test `unique_tmp` dir plus a descriptive file name: "AF_UNIX
    /// path is 122 bytes". The socket does not need to live in the fixture's
    /// own dir to be the fixture's socket; the system temp dir plus a short tag
    /// keeps the three tests distinct and the name bounded on BOTH hosts.
    fn short_sock(tag: &str) -> PathBuf {
        aoide_test_support::short_tmp(&format!("sb-{tag}")).join("s.sock")
    }

    /// The §0 corollary's own test for the newest arm: a `workspaceaction`
    /// line that the whitelist REFUSES over a REAL socket is dropped, audited
    /// (`unparseable`, a byte count, never the payload), and — because the
    /// click that sent it is parked on a reply — ANSWERED with a `bad-request`
    /// refusal, the same rule `sessiontrace` holds. No child is spawned: one
    /// refused line, whose reply is the whole observable.
    #[test]
    fn a_refused_workspaceaction_line_is_audited_and_answered_over_the_socket() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "AOIDE_AUDIT_LOG",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_CORE_BIN",
        ]);
        let root = aoide_test_support::unique_tmp("shellbridge-ws-refused");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("AOIDE_STAGE_DIR", &root);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        // A `clear` carrying a `project` is refused by name at the gate; the
        // core binary is pointed somewhere that cannot exist so a spawn would
        // be unmistakable if the gate ever let this through.
        std::env::set_var("AOIDE_CORE_BIN", "/nonexistent/aoide");

        let sock = short_sock("ws-refused");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener)); // never joined

        let mut client = UnixStream::connect(&sock).unwrap();
        {
            use std::io::Write;
            client
                .write_all(
                    b"{\"cmd\":\"workspaceaction\",\"action\":\"clear\",\"workspace\":3,\"project\":\"aoide\"}\n",
                )
                .unwrap();
            client.flush().unwrap();
        }
        // The parked caller is ANSWERED, once, on its own connection.
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(client);
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        let reply: Value = serde_json::from_str(answer.trim()).expect("one JSON reply line");
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(reply["reason"], "bad-request");
        assert!(reply.get("workspace").is_none(), "nothing was resolved: {reply}");
        assert!(reply.get("action").is_none(), "nothing parsed to echo: {reply}");

        // And the drop is audited, with a byte count and no payload.
        let log = root.join("log");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("unparseable") && text.contains("workspaceaction-refused") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(text.contains("unparseable"), "{text}");
        assert!(text.contains("workspaceaction-refused"), "{text}");
        assert!(!text.contains("aoide"), "no payload in the audit line: {text}");

        let _ = std::fs::remove_dir_all(&root);
    }

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

        let sock = short_sock("idle");
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

        let sock = short_sock("herald");
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
