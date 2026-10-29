//! The desktop verbs of the desktop's own shellbridge socket. `strillix shellbridge --run` serves
//! [`SOCKET`] (`$XDG_RUNTIME_DIR/strillix/bridge.sock`) through `aoide_protocol::shellbridge::run_on`.
//! Core's verbs are aoided's, on `aoide/shellbridge.sock`, and this process serves none of them. A line naming one of these verbs is parsed here ([`parse_command`]) and served here
//! ([`serve_line`]).

use aoide_conduct::shellbridge::audit_bridge_action;
use aoide_protocol::shellbridge::{drop_unparsed, wire_cmd, Flow, Socket, UnixStream, Verb};
use aoide_protocol as daemon;
use serde_json::{json, Value};

use component::{component_arg, component_command, component_slot, component_verb};
use power::{dispatch_power, spawn_detached, PowerAction};
use rice::{
    dispatch_rice_declare, dispatch_rice_draft, dispatch_rice_menu, dispatch_rice_mode, notify,
    rice_declare_audit, rice_name, RiceDraftAction, RiceModeAction,
};
use workspace::{dispatch_workspace_action, WorkspaceBinding};

mod component;
mod power;
mod rice;
mod workspace;
#[cfg(test)]
pub(crate) mod testutil;

/// The desktop's socket: `$XDG_RUNTIME_DIR/strillix/bridge.sock`. ShellBridge.qml sends the
/// desktop verbs here and the core verbs to aoided's socket.
pub fn socket_path() -> std::path::PathBuf {
    aoide_storage::runtime_dir::runtime_root().join("strillix").join("bridge.sock")
}

/// The audit log this socket's drops, refusals and outcomes land in: `$STRILLIX_ROOT/log`.
pub fn audit_log() -> std::path::PathBuf {
    strillix_rice::paths::audit_log()
}

/// The desktop socket's rules: the desktop verbs and no core arm, logged to the desktop's trail.
pub static SOCKET: Socket = Socket { verbs: VERBS, log: audit_log, front: None };

/// The verbs `strillix shellbridge --run` registers on its socket. Each is served by
/// [`serve_line`]; no core verb is among them.
pub const VERBS: &[Verb] = &[
    Verb { cmd: "focuswindow", serve: serve_line },
    Verb { cmd: "focussession", serve: serve_line },
    Verb { cmd: "power", serve: serve_line },
    Verb { cmd: "component", serve: serve_line },
    Verb { cmd: "ricemode", serve: serve_line },
    Verb { cmd: "ricemenu", serve: serve_line },
    Verb { cmd: "ricedraft", serve: serve_line },
    Verb { cmd: "ricedeclare", serve: serve_line },
    Verb { cmd: "workspaceaction", serve: serve_line },
];


/// One parsed desktop command from the shellbridge socket wire (newline-delimited
/// JSON). The wire shape is defined by ShellBridge.qml.
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
    /// strillix lock / hyprctl / systemctl.
    Power { action: PowerAction },
    /// `{ "cmd": "component", "slot": "…", "verb": "…", "arg": "…"? }` — a
    /// cross-component call (ShellBridge.qml's `component`): the component that
    /// fills `slot` does `verb`, with `arg` when it takes one. Fire-and-forget,
    /// like [`Self::Power`]; the names are held to the closed charsets of
    /// [`component_slot`], [`component_verb`] and [`component_arg`], and the
    /// spelling is [`component_command`].
    Component { slot: String, verb: String, arg: Option<String> },
    /// `{ "cmd": "ricemode", "action": "stage|declarative", "name": "…" }` — a
    /// row of the bar's RICE menu: switch the stage to the named runtime rice,
    /// or lock back to declarative. Fire-and-forget like [`Self::RiceDraft`]:
    /// the outcome is a toast, never a reply. The closed set and its field
    /// rules are [`RiceModeAction`]'s. See [`dispatch_rice_mode`].
    RiceMode { action: RiceModeAction },
    /// `{ "cmd": "workspaceaction", "action": "set|clear", "workspace": <int>,
    /// "project": "<name>", "new": <bool> }` — the bar's click that binds a
    /// compositor workspace to a project, or unbinds it (W-P5). Modelled on
    /// the core `projectaction`: zero-session (no session id anywhere on the
    /// wire, in the plan, the reply, or the audit line), a closed two-action
    /// set ([`WorkspaceBinding`]), and no binding logic of its own — it plans
    /// the same argv `aoide workspace set [<ws>] <project> [--new]` /
    /// `aoide workspace clear <ws>` take and runs it through the same core
    /// re-exec every acknowledged action uses.
    ///
    /// `workspace` is the compositor's own workspace id, omitted to mean the
    /// FOCUSED one. Unlike a session or project action, one field is resolved
    /// here rather than left to the child: [`dispatch_workspace_action`] asks
    /// the compositor adapter (`crate::hypr::focused_workspace`) and plans the RESOLVED
    /// integer, because `workspace clear` takes an id and no wire caller can
    /// be expected to know the focused one. A host with no adapter is
    /// ANSWERED (`reason: "no-compositor"`), never dropped — the click is
    /// parked on a reply.
    WorkspaceAction { workspace: Option<i64>, binding: WorkspaceBinding },
    /// `{ "cmd": "ricemenu" }` — the RICE menu's READ: the runtime rices
    /// and the mode, plus the drafts of the rice the current mode is working
    /// on, answered on this connection like the core `sessiontrace`. No
    /// payload: the daemon runs `strillix rice list --json` and, when the mode
    /// names a rice (`drafts_rice`), `strillix rice draft list <rice> --json`,
    /// both under one `RICE_MENU_TIMEOUT` budget. Nothing is written. See
    /// [`dispatch_rice_menu`].
    RiceMenu,
    /// `{ "cmd": "ricedraft", "action": "enter|new|save|drop", "name": "…" }` —
    /// a picker click that enters a draft, forks a new one and enters it,
    /// saves the stage as a draft, or deletes a saved one. Fire-and-forget like [`Self::RiceMode`]:
    /// the outcome is a toast, never a reply. The closed set and its field
    /// rules are [`RiceDraftAction`]'s; the plan, in `rice` argv, is
    /// `rice_draft_plan`. See [`dispatch_rice_draft`].
    RiceDraft { action: RiceDraftAction },
    /// `{ "cmd": "ricedeclare", "name": "<rice>" }` — the menu's Declare row:
    /// make `name` the rice this host performs on its next rebuild. Answered
    /// on this connection like [`Self::RiceMenu`], because the outcome is a
    /// balloon the surface paints, never a toast. `name` must be a rice name
    /// ([`rice_name`]: lowercase letters, digits, hyphens, no leading hyphen),
    /// so it is one argv token and safe to name in the audit line; every rule
    /// about WHICH rice may be declared is the CLI's. See
    /// [`dispatch_rice_declare`].
    RiceDeclare { name: String },
}

/// Parse ONE desktop wire line into a [`BridgeCommand`]. Pure and total: a verb
/// with an empty or absent required field, or a field outside its shape, yields
/// `None` — never a panic and never a default.
pub fn parse_command(line: &str) -> Option<BridgeCommand> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("cmd").and_then(Value::as_str)? {
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
        "component" => {
            let slot = v.get("slot")?.as_str()?;
            let verb = v.get("verb")?.as_str()?;
            let arg = match v.get("arg") {
                None => None,
                Some(a) => Some(a.as_str().filter(|s| component_arg(s))?.to_string()),
            };
            (component_slot(slot) && component_verb(verb))
                .then(|| BridgeCommand::Component { slot: slot.to_string(), verb: verb.to_string(), arg })
        }
        "ricemode" => RiceModeAction::from_wire(&v).map(|action| BridgeCommand::RiceMode { action }),
        "ricemenu" => Some(BridgeCommand::RiceMenu),
        "ricedraft" => RiceDraftAction::from_wire(&v).map(|action| BridgeCommand::RiceDraft { action }),
        "ricedeclare" => {
            let name = v.get("name")?.as_str()?;
            rice_name(name).then(|| BridgeCommand::RiceDeclare { name: name.to_string() })
        }
        _ => None,
    }
}

/// Serve ONE desktop line: parse it and act on it. An arm that took the
/// connection's one reply channel returns [`Flow::Answered`]; every other arm
/// keeps the connection reading ([`Flow::Next`]). A line that does not parse is
/// audited and dropped, and a malformed `workspaceaction` is refused, because its
/// caller is parked on the reply.
pub(crate) fn serve_line(line: &str, reply: &mut Option<UnixStream>) -> Flow {
    match parse_command(line) {
        Some(BridgeCommand::WorkspaceAction { workspace, binding }) => {
            // Same one-shot-connection discipline as the core `sessionaction`
            // and `projectaction`: no reply channel means the action does not run.
            let action = binding.as_str();
            let Some(mut reply) = reply.take() else {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "workspaceaction-noreply",
                    &format!("workspace action {action}: no reply channel; not dispatched"),
                );
                return Flow::Answered;
            };
            std::thread::spawn(move || {
                use std::io::Write;
                let _ = writeln!(reply, "{}", dispatch_workspace_action(workspace, &binding));
            });
            return Flow::Answered;
        }

        Some(BridgeCommand::Focus { address }) => match crate::hypr::focus_window(&address) {
            Ok(()) => {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "focus",
                    &format!("focused window {address}"),
                );
            }
            Err(e) => {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "focus-failed",
                    &format!("{} ({}): {}", address, e.reason, e.message),
                );
            }
        },

        Some(BridgeCommand::FocusSession { session_id }) => {
            match crate::hypr::focus_session(&session_id) {
                Ok(()) => {
                    let _ = daemon::audit(
                        &audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "focus",
                        &format!("focused session {session_id}"),
                    );
                }
                Err(e) => {
                    let _ = daemon::audit(
                        &audit_log(),
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
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "power",
                    &format!("spawned power action {}", action.as_str()),
                );
            }
            Err(e) => {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "power-failed",
                    &format!("power action {}: {e}", action.as_str()),
                );
            }
        },

        Some(BridgeCommand::Component { slot, verb, arg }) => {
            let (prog, args) = component_command(&slot, &verb, arg.as_deref());
            let (event, outcome) = match spawn_detached(prog, &args) {
                Ok(()) => ("component", "spawned".to_string()),
                Err(e) => ("component-failed", format!("{e}")),
            };
            let _ = daemon::audit(
                &audit_log(),
                daemon::Door::Daemon,
                daemon::EventClass::Audit,
                "shellbridge",
                event,
                &format!("{slot} {verb}: {outcome}"),
            );
        }

        // Fire-and-forget: the outcome is a toast, never a reply. The audit
        // line names the action and its outcome and nothing else — never a
        // rice, which the toast alone carries.
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
            audit_bridge_action(&audit_log(), "ricemode", action.as_str(), event, outcome);
        }

        // Fire-and-forget like `ricemode`: the outcome is a toast, never a
        // reply, and the steps run inline for the same reason (a mode
        // change never takes this process down). The audit line names the
        // action and its outcome and nothing else — never a draft or a
        // rice, which the toast alone carries.
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
            audit_bridge_action(&audit_log(), "ricedraft", action.as_str(), event, outcome);
        }

        // The Declare row: answered on this connection and run on its OWN
        // thread like the menu's read, because the surface paints the
        // answer. A gesture, so every outcome is audited, naming the rice
        // (a validated rice name) and whether the host file changed.
        Some(BridgeCommand::RiceDeclare { name }) => {
            let Some(mut reply) = reply.take() else {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "ricedeclare-noreply",
                    "ricedeclare: no reply channel; not dispatched",
                );
                return Flow::Answered;
            };
            std::thread::spawn(move || {
                use std::io::Write;
                let answer = dispatch_rice_declare(&name);
                let (event, outcome) = rice_declare_audit(&answer);
                audit_bridge_action(&audit_log(), "ricedeclare", &name, event, outcome);
                let _ = writeln!(reply, "{answer}");
            });
            return Flow::Answered;
        }

        // The menu's read: answered on this connection and run on its OWN
        // thread, like `sessiontrace`, because it re-execs the CLI under a
        // bound. No reply channel means the query does not run at all.
        // Only a REFUSAL is audited — a read is not a gesture.
        Some(BridgeCommand::RiceMenu) => {
            let Some(mut reply) = reply.take() else {
                let _ = daemon::audit(
                    &audit_log(),
                    daemon::Door::Daemon,
                    daemon::EventClass::Audit,
                    "shellbridge",
                    "ricemenu-noreply",
                    "ricemenu: no reply channel; not dispatched",
                );
                return Flow::Answered;
            };
            std::thread::spawn(move || {
                use std::io::Write;
                let answer = dispatch_rice_menu();
                if answer.get("ok").and_then(Value::as_bool) != Some(true) {
                    let reason = answer.get("reason").and_then(Value::as_str).unwrap_or("failed");
                    let _ = daemon::audit(
                        &audit_log(),
                        daemon::Door::Daemon,
                        daemon::EventClass::Audit,
                        "shellbridge",
                        "ricemenu-refused",
                        &format!("ricemenu refused: {reason}"),
                    );
                }
                let _ = writeln!(reply, "{answer}");
            });
            return Flow::Answered;
        }

    None => {
        drop_unparsed(&audit_log(), line);
        if wire_cmd(line).as_deref() == Some("workspaceaction") {
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
                    &audit_log(),
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
                return Flow::Answered;
            }
        }
    }
    }
    Flow::Next
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_command_rejects_a_blank_or_absent_focuswindow() {
        // Empty / absent address → None (never dispatch a blank focus).
        assert_eq!(parse_command(r#"{"cmd":"focuswindow","address":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"focuswindow","address":"   "}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"focuswindow"}"#), None);
    }

    /// The desktop socket answers no core verb. A `sessiontrace` line it cannot serve is
    /// audited `unparseable` and dropped with no reply, and the connection stays open, so
    /// the line returned `Flow::Next` and not `Answered`. A malformed `workspaceaction` on
    /// that same connection still gets its own `bad-request` refusal.
    #[test]
    fn a_desktop_socket_drops_a_sessiontrace_silently_and_still_refuses_a_bad_workspaceaction() {
        use crate::bridge::testutil::{env_lock, short_sock};
        use aoide_protocol::shellbridge::serve;
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;

        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "STRILLIX_ROOT",
            "AOIDE_AUDIT_LOG",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_CORE_BIN",
        ]);
        let root = aoide_test_support::unique_tmp("shellbridge-desktop-trace");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        aoide_test_support::pin_stage(root.join("stage"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_CORE_BIN", "/nonexistent/aoide");

        let sock = short_sock("desktop-trace");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener, &SOCKET)); // not joined

        let mut client = UnixStream::connect(&sock).unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let trace_line = r#"{"cmd":"sessiontrace","sessionId":"s1","clip":"detials"}"#;
        let ws_line = r#"{"cmd":"workspaceaction","action":"clear","workspace":3,"project":"aoide"}"#;
        client.write_all(format!("{trace_line}\n").as_bytes()).unwrap();
        client.flush().unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
        let mut heard = String::new();
        let silence = reader.read_line(&mut heard);
        assert!(
            matches!(&silence, Err(e) if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )),
            "a desktop sessiontrace is never answered, and the connection stays open: {silence:?} {heard:?}"
        );

        client.write_all(format!("{ws_line}\n").as_bytes()).unwrap();
        client.flush().unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        let reply: Value = serde_json::from_str(answer.trim()).expect("one JSON reply line");
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(reply["reason"], "bad-request", "{reply}");
        assert!(reply.get("sessionId").is_none(), "a late sessiontrace refusal would carry one: {reply}");
        assert!(
            reply["message"].as_str().unwrap_or("").starts_with("a workspaceaction line"),
            "{reply}"
        );

        let log = root.join("log");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.matches("unparseable").count() >= 2 && text.contains("workspaceaction-refused") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let drops: Vec<&str> = text.lines().filter(|l| l.contains("unparseable")).collect();
        assert_eq!(drops.len(), 2, "one audit entry per dropped line: {text}");
        assert!(drops.iter().any(|l| l.contains(&format!("({} bytes)", trace_line.len()))), "{text}");
        assert!(drops.iter().any(|l| l.contains(&format!("({} bytes)", ws_line.len()))), "{text}");
        assert!(text.contains("workspaceaction-refused"), "{text}");
        assert!(!text.contains("sessiontrace-refused"), "the desktop socket never refuses a sessiontrace: {text}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
