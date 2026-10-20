//! The `workspaceaction` verb: bind a compositor workspace to a project, or clear the
//! binding. The focused workspace is resolved here, in the process that owns the
//! compositor adapter; the binding itself is `aoide workspace set|clear`, run through
//! the core CLI.

use aoide_conduct::graph::{focused_workspace, NO_COMPOSITOR};
use aoide_conduct::shellbridge::{audit_bridge_action, outcome_triple, run_core_step, safe_action_value};
use serde_json::{json, Value};

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
    pub(crate) fn from_wire(action: &str, fields: &Value) -> Option<Self> {
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
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Set { .. } => "set",
            Self::Clear => "clear",
        }
    }

    /// The ONE argv this binding plans for one RESOLVED workspace id — exactly
    /// the argv the CLI takes, so `--new` is the only optional element and
    /// nothing here re-implements a binding. `--json` is appended by the
    /// runner ([`run_core_step`]), never planned here.
    pub(crate) fn plan(&self, ws: i64) -> Vec<String> {
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

/// The ONE reply line for a `workspaceaction` —
/// `{ok, message, action, workspace, project?, data?}`. Same read as every
/// other acknowledged action ([`outcome_triple`], the CLI's own `message`
/// and `data` carried verbatim), keyed by the RESOLVED workspace id, so a
/// caller that omitted the workspace learns which one it actually touched.
/// `project` rides only on `set` — the one action that has one.
pub(crate) fn workspace_reply(
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
pub(crate) fn dispatch_workspace_action(workspace: Option<i64>, binding: &WorkspaceBinding) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testutil::env_lock;
    use crate::bridge::testutil::short_sock;
    use aoide_conduct::shellbridge::serve;
    use crate::bridge::VERBS;
    use crate::bridge::testutil::{stub_workspace_core};
    use aoide_test_support::sv;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::{UnixListener, UnixStream};
    use crate::bridge::{parse_command, BridgeCommand};

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
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
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

    /// GATED on Unix with its reason (see `aoide_test_support::fake_hyprctl`): the
    /// fixture is a `#!/bin/sh` shim `CreateProcess` cannot launch, over a
    /// compositor native Windows does not run.
    #[cfg(unix)]
    #[test]
    fn dispatch_workspace_action_resolves_an_omitted_workspace_through_the_adapter() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_workspace_core("shellbridge-ws-focused");
        let (hypr, shim) = aoide_test_support::fake_hyprctl("ws-action-focused");
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
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _sig = aoide_test_support::EnvSaver::capture(&["HYPRLAND_INSTANCE_SIGNATURE"]);
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

    /// The §0 corollary's own test for the newest arm: a `workspaceaction`
    /// line that the whitelist REFUSES over a REAL socket is dropped, audited
    /// (`unparseable`, a byte count, never the payload), and — because the
    /// click that sent it is parked on a reply — ANSWERED with a `bad-request`
    /// refusal, the same rule `sessiontrace` holds. No child is spawned: one
    /// refused line, whose reply is the whole observable.
    #[test]
    fn a_refused_workspaceaction_line_is_audited_and_answered_over_the_socket() {
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
        let root = aoide_test_support::unique_tmp("shellbridge-ws-refused");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
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
        std::thread::spawn(move || serve(&listener, VERBS)); // never joined

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
}
