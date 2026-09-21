//! The single command dispatcher.
//!
//! Both the CLI door (`bin/aoide.rs`) and the MCP door (`mcp.rs`) call
//! [`dispatch`] with a command path + parsed args/flags. There is ONE
//! implementation of every command; the doors cannot drift ("three doors,
//! one schema", concepts/Agent-Interface). A2A (`a2a.rs`, CONTRACTS.md §6)
//! reuses these same command handlers directly rather than dispatching
//! every JSON-RPC method through here.
//!
//! `dispatch()` itself is thin: it looks up the [`crate::registry::Registry`]
//! (built once via [`registry`]) and either calls the matched command's
//! handler, returns the not-implemented envelope, or returns an
//! unknown-command usage error — then appends to the single audit log and
//! applies the gate tail uniformly. Every command's actual behavior lives in
//! its DOMAIN crate's `commands` module (`aoide_conduct::commands`,
//! `aoide_storage::commands`, … — Phase 9 restructure,
//! docs/architecture/PACKAGE-LAYOUT.md), assembled here by
//! `commands/mod.rs::all()`. (`aoide_song::commands`/`aoide_screen::commands`
//! are the same pattern one door over — they assemble into `lyra`'s
//! registry, not this crate's, since P-A5.)
//!
//! `Invocation` moved to `aoide-protocol` (Phase 2 restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) — it's the type that broke the cycle
//! (`Command.handler` is `fn(&Invocation) -> Outcome`) and is re-exported here
//! so every existing `crate::dispatch::Invocation` caller is untouched.

pub use aoide_protocol::Invocation;

use crate::daemon;
use crate::output::Outcome;
use crate::registry::Registry;
use std::sync::OnceLock;

/// The process-wide command registry, built once from every command group's
/// `register()`. `cli.rs`, `mcp.rs`, and `dispatch()` all read through this
/// single instance.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(crate::commands::all)
}

/// The audit-log path in effect (flag override → `aoide.auditLog` default).
/// The function itself moved to `aoide-protocol` (Phase 9 restructure) — both
/// its inputs are protocol types; re-exported here so every existing
/// `crate::dispatch::audit_log_path` caller is untouched.
pub(crate) use aoide_protocol::audit_log_path;

/// Dispatch one invocation to its handler and return the structured outcome.
/// Every path here also appends to the single audit log — both doors inherit
/// the same policy surface (concepts/Governance).
pub fn dispatch(inv: &Invocation) -> Outcome {
    let cmd = inv.dotted();
    let meta = registry().get(&inv.path);

    let outcome = match meta {
        Some(m) if m.implemented => (m.handler)(inv),
        Some(m) => Outcome::not_implemented(cmd.clone(), m.gated).with_data(serde_json::json!({
            "path": m.path,
            "args": inv.args,
            "flags": inv.flags,
        })),
        None => Outcome::usage(
            cmd.clone(),
            format!("unknown command: `{}`", cmd.replace('.', " ")),
        ),
    };

    // Wire the audit-log append as a real code path for every dispatch.
    // The logged text is [`audit_message`]'s, not the outcome's own: one
    // family of commands carries a ceremony secret in its human wording, and
    // `$AOIDE_ROOT/log` is read back (the conductor's LOG panel tails it).
    let status = match outcome.status {
        crate::output::Status::Ok => "ok",
        crate::output::Status::Error => "error",
        crate::output::Status::Usage => "usage",
        crate::output::Status::NotImplemented => "not-implemented",
    };
    let log = audit_log_path(inv);
    let _ = daemon::audit(
        &log,
        inv.door,
        daemon::EventClass::Audit,
        &cmd,
        status,
        &audit_message(&inv.path, &cmd, status, &outcome.message),
    );

    // Mark gated commands so both doors surface the gate uniformly.
    match meta {
        Some(m) if m.gated => outcome.gated(true),
        _ => outcome,
    }
}

/// Is this command path part of the pairing ceremony — `pair` (the one
/// command the family is named after), a subcommand of it (`pair.reject`,
/// `pair.watch`), or the wrapper that drives the same legs (`mesh.pair`)?
///
/// **Matched on the path's own segments, never on the message.** A secret
/// rides whatever wording a handler happens to choose, so the withholding
/// must not depend on reading what was said. A `NNN-NNN` scan is the
/// fragile version of this: it catches a SAS only in exactly that spelling
/// (the operator's own `740 729` is a code
/// `aoide_client::commands::code_matches` accepts, and a shape-based rule
/// would have to guess at spacing), it says nothing about a malformed
/// or partial entry, and it does not describe at all the id or name an
/// operator typed — which the refusals echo back verbatim. Worse, it makes
/// the guarantee a hostage to wording: any future envelope that prints the
/// code differently leaks silently, and nothing test-fails. Keyed on the
/// path, the rule holds for text nobody has written yet.
fn is_pair_ceremony(path: &[String]) -> bool {
    path.iter().any(|segment| segment == "pair")
}

/// The audit MESSAGE one dispatch writes.
///
/// Every other command audits its outcome verbatim — this crate's whole
/// record of what happened. The pairing ceremony does not, because its
/// envelopes carry ceremony secrets **in free text**: the requester's own SAS
/// (`aoide_client::commands`'s "confirmation code {sas}" arms), the approver's
/// reply SAS on its own commit, and whatever the operator typed at this door —
/// an id or a name, which the refusals echo back verbatim. `$AOIDE_ROOT/log`
/// is read back — the conductor's own LOG panel tails it
/// (`conductor/src/app.rs`'s `reload_log` → `eventview::read_history` on
/// `aoide_protocol::default_audit_log`) — so a code written here is a code
/// published on someone's screen.
///
/// The line therefore keeps the operation and the status (which the record's
/// own `command`/`status` fields carry anyway, so a reader loses nothing) and
/// drops the free text wholesale. This is the LOG's copy, never the door's:
/// `Outcome.message`/`--json`/human output are untouched, because the two
/// operators still have to read their codes to each other off their own
/// screens — the same "only the stored copy is bounded" posture
/// `aoide_protocol::audit`'s message clamp takes, one field over.
fn audit_message(path: &[String], cmd: &str, status: &str, message: &str) -> String {
    if is_pair_ceremony(path) {
        format!(
            "{cmd} {status} — pairing ceremony: message withheld from the audit log \
             (it can carry a confirmation or reply code)"
        )
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Door;
    use crate::output::Status;
    use std::collections::BTreeMap;

    fn inv(path: &[&str], args: &[&str]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: BTreeMap::new(),
            door: Door::Cli,
        }
    }

    #[test]
    fn unknown_command_is_usage_exit_2() {
        let out = dispatch(&inv(&["nope", "nope"], &[]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.render(false).1, crate::output::exit::USAGE);
        assert!(out.message.contains("unknown command"));
    }

    #[test]
    fn not_implemented_command_carries_the_stub_envelope_and_gate() {
        // `rice declare` (the gated+not-implemented example this test used to
        // exercise) moved to lyra at P-A5. `content approve` is core's own
        // gated + not-implemented stub — same shape, still here.
        let out = dispatch(&inv(&["content", "approve"], &["/tmp/dusk"]));
        assert_eq!(out.status, Status::NotImplemented);
        assert_eq!(out.render(false).1, crate::output::exit::NOT_IMPLEMENTED);
        assert!(out.gated, "content.approve is a gated command");
        assert_eq!(out.data.unwrap()["args"][0], "/tmp/dusk");
    }

    #[test]
    fn implemented_command_dispatches_to_its_handler() {
        let out = dispatch(&inv(&["guide"], &[]));
        assert_eq!(out.status, Status::Ok);
        assert!(out.data.unwrap()["text"].as_str().unwrap().contains("aoide"));
    }

    /// Task #138's design record §5 reviewer checklist item 3, the dangerous
    /// one: the external-command probe (`aoide_protocol::door::run`) must
    /// stay OUT of `dispatch()`'s own `None =>` arm above — that
    /// "simplification" would leave the golden green while silently
    /// granting `PATH` execution to MCP, A2A, and the aoided socket, since
    /// `dispatch()` (unlike `run`) is the one door-agnostic point every
    /// non-CLI door reaches directly. This is the tripwire: even with a
    /// real `aoide-foo` executable sitting on `PATH`, dispatching an
    /// unregistered path on any non-CLI door must still be a plain
    /// unknown-command usage error, because `dispatch()` has no PATH-probing
    /// logic of its own — shaped on `require_cli_tty_refuses_every_non_cli_
    /// door` (`crates/conduct/src/graph/grant.rs`), which loops the same
    /// three doors.
    #[test]
    fn an_unregistered_path_on_a_non_cli_door_is_still_unknown_command() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_cli_dispatch_non_cli_door_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("aoide-foo");
        std::fs::write(&plugin, "#!/bin/sh\nexit 0\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&plugin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&plugin, perms).unwrap();
        }
        std::env::set_var("PATH", &dir);

        for door in [Door::Mcp, Door::Daemon, Door::A2a] {
            let out = dispatch(&Invocation {
                path: vec!["foo".to_string()],
                args: vec![],
                flags: BTreeMap::new(),
                door,
            });
            assert_eq!(out.status, Status::Usage, "door {door:?}: {}", out.message);
            assert!(out.message.contains("unknown command"), "door {door:?}: {}", out.message);
        }

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn dotted(path: &[&str]) -> (Vec<String>, String) {
        let segs: Vec<String> = path.iter().map(|s| s.to_string()).collect();
        let cmd = segs.join(".");
        (segs, cmd)
    }

    /// The pairing ceremony's audit copy keeps the operation and the status
    /// and drops the text — the text is where the SAS rides
    /// (`client::commands`'s "confirmation code {sas}" envelopes, the
    /// approver's reply SAS). Every path segment spelling of the family is
    /// covered, `mesh.pair` included: its report embeds each leg's own
    /// outcome text verbatim (`aoide_client::mesh::classify`'s `detail`), so it
    /// inherits the same leak and the same withholding.
    #[test]
    fn pair_ceremony_audit_text_withholds_the_outcome_message() {
        let sentinel = "839-035";
        for path in [&["pair"][..], &["pair", "reject"], &["pair", "watch"], &["mesh", "pair"]] {
            let (segs, cmd) = dotted(path);
            let human = format!(
                "pairing request sent to `osaka` (http://box:8710/) — confirmation code {sentinel} — \
                 read this aloud (or otherwise out-of-band) to osaka's operator"
            );
            let audited = audit_message(&segs, &cmd, "ok", &human);
            assert!(!audited.contains(sentinel), "{cmd}: the audit copy must not carry the code: {audited}");
            assert!(!audited.contains("confirmation code"), "{cmd}: no fragment of the text either: {audited}");
            assert!(audited.contains(&cmd), "{cmd}: the operation stays named: {audited}");
            assert!(audited.contains("ok"), "{cmd}: the status stays named: {audited}");
        }
    }

    /// "Even when malformed": the withholding is keyed on the command path,
    /// so a mistyped code the gate itself would still accept (`code_matches`
    /// strips whitespace, so `740 729` is `740-729`), a code-shaped id echoed
    /// back in a refusal, or a message carrying nothing at all, all leave the
    /// log in the SAME shape. Nothing in this path parses the message.
    #[test]
    fn pair_ceremony_audit_text_does_not_depend_on_the_message_shape() {
        let (segs, cmd) = dotted(&["pair"]);
        let texts = [
            "code mismatch — try 1 of 3; 2 more before this request is auto-denied",
            "no pending pairing request with id `740 729` (unknown, already resolved, or expired)",
            "pairing request sent to `osaka` — confirmation code 740729 — read this aloud",
            "",
        ];
        let first = audit_message(&segs, &cmd, "error", texts[0]);
        for text in texts {
            let audited = audit_message(&segs, &cmd, "error", text);
            assert_eq!(audited, first, "the audit copy must not vary with the message: {text:?}");
            assert!(!audited.contains("740"), "{audited}");
        }
    }

    /// The withholding is the pairing ceremony's alone — every other command
    /// still audits exactly what it reported, byte for byte (`session prune`'s
    /// own test in `conductor_integration.rs` pins the same seam end to end).
    #[test]
    fn ordinary_commands_audit_their_outcome_message_verbatim() {
        let (segs, cmd) = dotted(&["session", "prune"]);
        let message = "pruned 2 done sessions from /tmp/dusk";
        assert_eq!(audit_message(&segs, &cmd, "ok", message), message);
    }

    /// End-to-end through the real dispatcher, over a real log file: the
    /// pair family's line loses what the operator typed while the RETURNED
    /// outcome still prints it (the human ceremony is untouched), and an
    /// ordinary command's line keeps its text — the control that proves the
    /// assertion above is reading a log that really was written.
    #[test]
    fn dispatch_withholds_typed_pair_input_from_the_log_and_keeps_the_human_outcome() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("aoide_cli_pair_audit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        std::env::set_var("AOIDE_STATE_DIR", dir.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", dir.join("stage"));
        let log = dir.join("log");

        // A refused `pair reject <target>`: the target is TYPED input, and
        // the taught unknown-id error echoes it back. No pending state is
        // read or written outside this tempdir — nothing here dials anyone.
        let sentinel = "839-035";
        let pair_out = dispatch(&Invocation {
            path: vec!["pair".to_string(), "reject".to_string()],
            args: vec![sentinel.to_string()],
            flags: BTreeMap::from([("audit-log".to_string(), log.to_string_lossy().into_owned())]),
            door: Door::Cli,
        });
        assert_eq!(pair_out.status, Status::Error, "{}", pair_out.message);
        assert!(
            pair_out.message.contains(sentinel),
            "the human outcome keeps what was typed at the door: {}",
            pair_out.message
        );

        let guide_out = dispatch(&Invocation {
            path: vec!["guide".to_string()],
            args: vec![],
            flags: BTreeMap::from([("audit-log".to_string(), log.to_string_lossy().into_owned())]),
            door: Door::Cli,
        });

        let contents = std::fs::read_to_string(&log).expect("dispatch wrote the audit log");
        let records: Vec<serde_json::Value> = contents
            .lines()
            .map(|line| serde_json::from_str(line).expect("one JSON-lines record per dispatch"))
            .collect();
        let pair_record = records
            .iter()
            .find(|r| r["command"] == "pair.reject")
            .unwrap_or_else(|| panic!("no pair.reject record in:\n{contents}"));
        assert_eq!(pair_record["status"], "error");
        let audited = pair_record["message"].as_str().unwrap();
        assert!(
            !audited.contains(sentinel),
            "the pair family's audit line must not carry the typed code: {audited}"
        );
        assert!(audited.contains("pair.reject") && audited.contains("error"), "{audited}");

        let guide_record = records
            .iter()
            .find(|r| r["command"] == "guide")
            .unwrap_or_else(|| panic!("no guide record in:\n{contents}"));
        assert_eq!(
            guide_record["message"].as_str().unwrap(),
            guide_out.message,
            "an ordinary command's audit line is its own message, verbatim"
        );

        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
