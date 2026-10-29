//! The self-registering command registry (CONTRACTS.md §3, v0).
//!
//! Every command in the tree is described here ONCE, alongside the handler
//! that runs it. The CLI dispatcher (`dispatch.rs`), the `schema --json`
//! emitter, the MCP tool list, and the A2A AgentCard (`a2a.rs`) all derive
//! from this single [`Registry`] — the "three doors, one schema" contract
//! (concepts/Agent-Interface). Nothing else in the crate enumerates commands.
//!
//! Each command group lives in its own `commands/<group>.rs` module and
//! contributes its entries via a `register(&mut Registry)` function; see
//! `commands/mod.rs::all()` for the assembly order (which reproduces the
//! historical `schema.rs` table order byte-for-byte — `schema --json` and the
//! MCP tool list must never reorder).
//!
//! The types (`Arg`, `Flag`, `Command`, `Schema`, `Registry`, `JSON_FLAG`,
//! the version consts) moved to `aoide-protocol` (Phase 2 restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) and are re-exported here so every
//! existing `crate::registry::*` caller is untouched. The `cmd!`/`arg!`/`flag!`
//! macros moved there too (Phase 9 restructure) so every domain crate's
//! `commands::register()` can describe its own commands — they expand to
//! `$crate::registry::Command { .. }` literals resolved inside the protocol
//! crate, and are re-exported here under the same names.

pub use aoide_protocol::registry::*;

#[cfg(test)]
mod tests {
    /// The pair door is closed: no command answers a far call a pair
    /// partner made.
    #[test]
    fn no_command_opens_the_pair_door() {
        let r = crate::commands::all();
        for c in r.commands() {
            assert!(!c.opens(aoide_protocol::audit::Door::Pair), "`{}` opens the pair door", c.path.join(" "));
        }
    }

    #[test]
    fn every_command_carries_the_json_flag_and_exit_codes() {
        let r = crate::commands::all();
        for c in r.commands() {
            assert!(
                c.flags.iter().any(|f| f.name == "json"),
                "{} missing --json",
                c.dotted()
            );
            let v = serde_json::to_value(c).unwrap();
            assert_eq!(v["exitCodes"]["0"], "ok");
            assert_eq!(v["exitCodes"]["64"], "not-implemented");
        }
    }

    /// `examples` is additive (CONTRACTS.md §3): a command WITHOUT examples
    /// serializes byte-identical to before the field existed — no `examples`
    /// key at all — while one WITH examples carries it.
    #[test]
    fn examples_key_is_absent_unless_populated() {
        let r = crate::commands::all();
        let with = r
            .commands()
            .find(|c| c.dotted() == "project.add")
            .expect("project add carries examples");
        let v = serde_json::to_value(with).unwrap();
        assert_eq!(v["examples"][0], "project add aoide ~/Aoide");

        let without = r
            .commands()
            .find(|c| c.dotted() == "session.prune")
            .expect("session prune carries none");
        let v = serde_json::to_value(without).unwrap();
        assert!(
            v.get("examples").is_none(),
            "no examples → no key (byte-identical schema): {v}"
        );
    }

    /// `internal` is additive (CONTRACTS.md §3), same discipline `examples`
    /// carries above: a non-internal command's schema stays byte-identical
    /// to before the field existed (no `internal` key at all), while the
    /// hook-plumbing family (task #101 R1) carries `"internal":true`.
    #[test]
    fn internal_key_is_absent_unless_the_command_is_hook_plumbing() {
        let r = crate::commands::all();
        let hook_cmd = r
            .commands()
            .find(|c| c.dotted() == "session.start")
            .expect("session start is the hook-plumbing family");
        assert!(hook_cmd.internal, "session.start is marked internal");
        let v = serde_json::to_value(hook_cmd).unwrap();
        assert_eq!(v["internal"], true);

        let operator_cmd = r
            .commands()
            .find(|c| c.dotted() == "graph")
            .expect("bare graph is an ordinary operator command");
        assert!(!operator_cmd.internal);
        let v = serde_json::to_value(operator_cmd).unwrap();
        assert!(
            v.get("internal").is_none(),
            "non-internal command carries no `internal` key at all: {v}"
        );
    }

    /// `approves` and `launches` are additive (CONTRACTS.md §3), the same
    /// discipline `internal` carries above: `schema --json` shows each key on
    /// the entries that set it, as `true`, and on no other entry.
    #[test]
    fn approves_and_launches_keys_are_on_the_marked_entries_only() {
        let r = crate::commands::all();
        let find = |p: &str| serde_json::to_value(r.commands().find(|c| c.dotted() == p).unwrap()).unwrap();
        assert_eq!(find("session.start")["approves"], true);
        assert!(find("session.start").get("launches").is_none());
        assert_eq!(find("job.remove")["approves"], true);
        assert!(find("job.remove").get("launches").is_none());
        assert_eq!(find("project.add")["launches"], true);
        assert!(find("project.add").get("approves").is_none());
        assert_eq!(find("resurrect")["approves"], true);
        assert_eq!(find("resurrect")["launches"], true);
        assert_eq!(find("spawn")["approves"], true);
        assert_eq!(find("spawn")["launches"], true);
        assert_eq!(find("node.spawn")["approves"], true);
        assert_eq!(find("node.spawn")["launches"], true);
        assert_eq!(find("conduct")["approves"], true);
        assert_eq!(find("conduct")["launches"], true);
        assert_eq!(find("session.pending.approve")["approves"], true);
        assert_eq!(find("session.pending.approve")["launches"], true);
        assert_eq!(find("a2a.serve")["launches"], true);
        assert!(find("a2a.serve").get("approves").is_none());
        for plain in ["graph", "session.prune"] {
            let v = find(plain);
            assert!(v.get("approves").is_none() && v.get("launches").is_none(), "{plain} carries neither key: {v}");
        }
        let doc = serde_json::to_value(r.schema("aoide")).unwrap();
        let entries = doc["commands"].as_array().unwrap();
        let marks = [
            ("approves", r.commands().filter(|c| c.approves).count()),
            ("launches", r.commands().filter(|c| c.launches).count()),
        ];
        for (key, marked) in marks {
            let shown: Vec<&serde_json::Value> = entries.iter().filter_map(|v| v.get(key)).collect();
            assert!(shown.iter().all(|v| **v == serde_json::json!(true)), "`{key}` is only ever true: {shown:?}");
            assert_eq!(shown.len(), marked, "`{key}` shows on exactly the marked entries");
        }
    }

    /// `external` is additive too (task #138, CONTRACTS.md §3), same
    /// discipline `examples`/`internal` carry above, but at the SCHEMA level
    /// rather than per-command: a host with zero `aoide-*` plugins on `PATH`
    /// gets a `schema --json` byte-identical to before the field existed —
    /// no `external` key at all — while dropping one on a scoped `PATH`
    /// surfaces it by name, resolved command spelling, and path.
    /// GATED on Unix with its reason: the fixture installs a `#!/bin/sh` shim
    /// on `PATH` and makes it executable by a mode (`CreateProcess` understands
    /// neither a shebang nor an extension-less name), so external-subcommand
    /// discovery cannot be driven on native Windows without a real `.exe`.
    /// Same gate and reason as the rest of this tree's shim groups; no native
    /// twin.
    #[cfg(unix)]
    #[test]
    fn external_key_is_absent_unless_a_plugin_is_on_path() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_cli_external_schema_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("PATH", &dir);

        let r = crate::commands::all();
        let v = serde_json::to_value(r.schema("aoide")).unwrap();
        assert!(v.get("external").is_none(), "no plugins on PATH -> no key at all: {v}");

        let plugin = dir.join("aoide-deploy");
        std::fs::write(&plugin, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&plugin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&plugin, perms).unwrap();
        }

        let v = serde_json::to_value(r.schema("aoide")).unwrap();
        assert_eq!(v["external"][0]["name"], "deploy");
        assert_eq!(v["external"][0]["command"], "aoide-deploy");
        assert_eq!(v["external"][0]["path"], plugin.to_string_lossy().to_string());

        match saved {
            Some(val) => std::env::set_var("PATH", val),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_paths_are_unique() {
        let r = crate::commands::all();
        let mut seen = std::collections::HashSet::new();
        for c in r.commands() {
            assert!(seen.insert(c.dotted()), "duplicate command {}", c.dotted());
        }
    }

    /// Golden snapshot: the sorted list of every command path. Adding,
    /// removing, or renaming a command is a reviewable diff here — this
    /// replaces the old magic `len() >= 33` assertion.
    #[test]
    fn command_paths_match_the_golden_snapshot() {
        let r = crate::commands::all();
        let mut got: Vec<String> = r.commands().map(|c| c.dotted()).collect();
        got.sort();

        // The R1 graph-prefix cutover (task #101, Lane R) renamed 18 of the 19
        // former `graph.*` spellings in place — same 73-count, no aliases:
        // `graph.view` -> bare `graph` (the render; `graph.link` alone
        // survives the family), `graph.send`/`graph.spawn`/`graph.resurrect`
        // -> bare `send`/`spawn`/`resurrect`, `graph.session.*`/`graph.permit`/
        // `graph.pending.*`/`graph.reap`/`graph.prune` -> `session.*`,
        // `graph.project.*` -> `project.*`. See `conduct/src/commands/
        // graph.rs`'s module doc for the full table. Bumped by 1 for bare
        // `session` — the undying picker (U3, command-defrag lane U) — a
        // parent command alongside `session.*` the same way bare `graph`
        // sits alongside `graph.link`: reached 74. Bumped by 1 more for
        // `node.pair.watch` (P-P5): reached 75. Bumped by 3 for `melete
        // status`/`melete graph`/`melete call` (M2, task #14) — the Melete
        // MCP client, `aoide_client::mcp_client` — reached 78. Bumped by 1
        // for `node.advertise` (task #120) — the discovery advertise
        // switch — reached 79. Bumped by 1 for `node.list` (task #120 P2)
        // — the one-glance mesh roster (this host + registered nodes +
        // advertising instances, sessions under each), registered from
        // `aoide-conduct` because it folds `who`'s probe core — reached 80.
        // Bumped by 1 for bare `pair` (task #120 P3) — the interactive
        // pairing picker, one sweep + a select menu driving the SAME
        // `run_pair_request` ceremony core `node invite` uses — reached 81.
        // Bumped by 1 for `secrets allow-remote-origin` (LANE IDENTITY
        // P-ID4) — the per-secret remote-origin admission bit the broker's
        // origin gate enforces (deny by default; the first real consumer
        // of the sealed session credential) — reached 82. Bumped by 1 for
        // `session window attach`: the desktop's window sweep binds a session's
        // window through core — reached 83.
        //
        // Session-surface redesign (command-defrag lane X, 2026-08-28) —
        // three movements in the SAME commit, net 82 -> 81:
        //   - `session.undying` RETIRED (-1, 82 -> 81): absorbed into
        //     `session.grant`'s positional `<kind>` grammar below — the
        //     standalone scripted spelling is now unknown, same as a typo.
        //   - `session.grant` ADDED (+1, 81 -> 82): the grant family — one
        //     kind today, `undying` (bare = interactive picker, relocated
        //     verbatim from the old bare `session`; `on|off` = the scripted
        //     mark `session.undying` used to be).
        //   - `who` RETIRED (-1, 82 -> 81): folded entirely into bare
        //     `session` — `session --hosts` now renders exactly what `who`
        //     used to (byte-identical), and plain bare `session` groups by
        //     PROJECT instead of by host. `session` itself keeps its
        //     existing path (no count change from it — only its meaning
        //     changed, from the U3 undying picker to the roster).
        // Net: 82 - 1 + 1 - 1 = 81.
        //
        // P-PV2 (the User's locked spec, three grill rounds) collapses the
        // pairing command surface, net 81 -> 80: `node.invite` (-1) and
        // `node.pair.request` (-1) DIE outright, hard cutover, no aliases —
        // folded into ONE `node.pair` (+1, positional `<target>`, SMART
        // TARGET dispatch: a URL dials directly, anything else resolves by
        // discovery sweep — reuses the same `run_pair_request` core both
        // dead commands called). `node.pair.pending` RENAMES to
        // `node.pending` (net 0, path change only) — its rows drop the SAS/
        // confirmation code (never shown outside the out-of-band compare
        // the approve step preserves). `node.pair.approve`'s `<id>` becomes
        // optional when exactly one request is pending (no new path).
        // Net: 81 - 1 - 1 + 1 = 80.
        //
        // Bumped by 2 for `config` and `config.set` (task #135 P-C) — the
        // portable runtime config file (`$AOIDE_ROOT/config.toml`,
        // `aoide_storage::config`): core is cargo-buildable on any host, so
        // a core command's configuration cannot live in a NixOS option —
        // reached 82.
        //
        // Task #135 P3' collapses the pairing surface AGAIN, net 82 -> 79
        // (the User: "the command set can just be aoide pair"): `node.pair`,
        // `node.pair.approve`, `node.pair.reject`, `node.pair.watch` and
        // `node.pending` all DIE — hard cutover, no aliases, same as
        // `node.invite` before them. Bare `pair` (already registered)
        // becomes the ONE command, routed by what already exists (approve an
        // inbound match, resume an outbound one, else request), plus
        // `pair.reject` (+1) and `pair.watch` (+1). The `node` family keeps
        // the ROSTER (add/list/allow/hub/spawn/pull/status/discover/
        // advertise); `pair` mints the verified records those operate on.
        // Net: 82 - 5 + 2 = 79.
        //
        // Task #135 P4 adds `mesh` (+1) — a read-only drift report
        // comparing every declared `[mesh.<name>]` in config.toml against
        // the live node registry (`aoide_client::mesh`); writes neither
        // side. Net: 79 + 1 = 80.
        //
        // Task #135 P5 adds `mesh.pair` (+1) — the converge: the same
        // `mesh::drift` selects the declared nodes with no verified record
        // (missing/unverified) and drives `commands::run_pair_request` over
        // each, one ordinary pairwise ceremony apiece. A verified node is
        // never modified, so a second run is all-skipped. Net: 80 + 1 = 81.
        //
        // The peer -> node vocabulary rename (User ruling, 2026-09-07)
        // renames all ten `peer.*` paths to `node.*` in place — hard
        // cutover, no aliases, same shape `node.invite`'s own retirement
        // set. Net: 81 (unchanged).
        //
        // Messaging plan P-M1 (docs/architecture/MAIL.md) retires
        // `inbox.clear`/`inbox.list`/`inbox.read` (-3) and adds the
        // addressed, signed, append-only mailbase's six commands: bare
        // `mail`, `mail.mark`, `mail.read`, `mail.rm`, `mail.send`,
        // `mail.show` (+6) — same alphabetical slot the retired `inbox.*`
        // trio held, between `identity` and `make`. Net: 81 - 3 + 6 = 84.
        //
        // P-M2 adds the outbox spool's own two commands, `mail.outbox` and
        // `mail.outbox.rm` (+2) — sorting between `mail.mark` and
        // `mail.read`, same mailbase family. Net: 84 + 2 = 86.
        //
        // The outbox un-park adds `mail.outbox.retry` (+1) — a policy
        // refusal is a PARKED state, not a kill-list: the `allows` set that
        // refused a letter is the RECEIVING node's own record of the
        // sender, so granting it there (`node allow <sender> message on`)
        // must let the already-spooled letters move. Sorts between
        // `mail.outbox` and `mail.outbox.rm` (`retry` > `rm`). Net: 87.
        //
        // Mail export (register §30) adds `mail.export` (+1) — the mailbase's
        // one OUTBOUND projection, one Markdown note per thread under
        // `state/mail-export/` (`aoide_client::mail_export`). Sorts between
        // `mail` and `mail.mark`, the same family, giving this list 98 paths.
        //
        // Mail poll (P-M3) adds `mail.poll` (+1) — the receive trigger a node
        // with nothing to send needs, and the only caller of
        // `aoide_client::mail_wire::poll_node` outside the drain. Same
        // family, so it sorts between `mail.outbox.rm` and `mail.read`; no
        // other entry moves.
        //
        // H1 adds `mail.serve` (+1) — the mail adapter, the second listener:
        // a long-running server registering beside `a2a.serve` (its own
        // sibling in `server/commands.rs`, the crate that owns both serve
        // loops), sorting in the mail family between `mail.send` and
        // `mail.show`.
        //
        // `node address` (+1) — the one command that moves an existing node
        // record to another address; sorts after `node.add`.
        //
        // Bumped by 1 for `project.edit` (multi-root projects) — the
        // exact-replacement editor for a project's root list, the
        // `project edit` that `records.rs`'s `Project.auto_resume` doc
        // used to say did not exist.
        //
        // Bumped by 1 for `session.trace` (P-EIDOLON slice E5) — the read
        // surface over a harness's own TRACE file
        // (`docs/architecture/EIDOLON-TRACE.md`): `aoide session trace <id>
        // [--tail N] [--follow] [--json]` renders the run step by step.
        // Registered from `aoide-conduct` beside `session.pending.list`, its
        // own neighbourhood, so the sort slot is between `session.reap` and
        // `session.start`. Read-only — no stage write, no daemon, no lock —
        // so it sits outside the L4 dual-writer family entirely. Net: 81 + 1
        // = 82.
        //
        // `secrets status` — the secrets broker's value-free inventory seam
        // (`aoide_secrets::commands::handle_secrets_status`, wire op
        // `status`): one row per registered policy, policy METADATA only,
        // read by the broker over the socket. Sort slot: between
        // `secrets.set-totp` and `secrets.watch`.
        //
        // Bumped by 3 for the workspace ↔ project binding
        // (`workspace.set`/`workspace.clear`/`workspace.list`, core-seams
        // design §B) — the binding that makes a workspace carry a project.
        // Registered from `aoide-conduct`'s `commands/graph.rs`, appended at
        // that `register()`'s tail; the sort slot falls after `usage`.
        // Bumped by 1 more for `workspace root` (W-P4 of that slice lane) —
        // the launcher's bare-path read of a bound project's first folder, the
        // second special-cased command here after the `secrets` value-printers
        // (`cli/README.md`'s named-seam list).
        //
        // Bumped by 4 for the P-CHARTER charter family
        // (`mesh.charter.init`/`sign`/`accept`/`reroot`, registered from
        // `aoide_client::charter`): one operator's machines, rooted once and
        // signed by that operator's own key. They sort inside the existing
        // `mesh.*` family, between bare `mesh` and `mesh.pair`. Net: 82 + 4
        // = 86. Bumped by 1 more for `mesh.charter.show` (the same slice's
        // read side: the charter in force and its status, which a charter
        // applied by an unattended poll needs an operator to be able to see)
        // — reached 87. Bumped by 1 for `mesh.join` (the trust-entry step every
        // machine but the operator's own performs: one operator key, or one
        // LAN ceremony) — reached 88.
        //
        // **The running total above is the CHAIN's history, not this list's
        // **The vec below holds 124 paths** (counted, review F7). This line is
        // the authority for how many; the chain above is the history of how it
        // got there and is never the answer.
        let mut expected: Vec<&str> = vec![
            "a2a.serve",
            "adapter.melete",
            "conduct",
            "conductor",
            "config",
            "config.set",
            "content.approve",
            "content.ingest",
            "content.propose",
            "content.query",
            "content.register",
            "daemon",
            "events.tail",
            "graph",
            "graph.link",
            "guide",
            "hooks.install",
            "identity",
            "job.add",
            "job.list",
            "job.remove",
            "job.runs",
            "mail",
            "mail.box.add",
            "mail.box.rm",
            "mail.export",
            "mail.mark",
            "mail.outbox",
            "mail.outbox.retry",
            "mail.outbox.rm",
            "mail.poll",
            "mail.read",
            "mail.ring",
            "mail.rm",
            "mail.route",
            "mail.send",
            "mail.serve",
            "mail.show",
            "make",
            "mcp.serve",
            "melete.call",
            "melete.graph",
            "melete.status",
            "mesh",
            "mesh.charter.accept",
            "mesh.charter.init",
            "mesh.charter.reroot",
            "mesh.charter.show",
            "mesh.charter.sign",
            "mesh.join",
            "mesh.pair",
            "node.add",
            "node.address",
            "node.advertise",
            "node.allow",
            "node.discover",
            "node.hub",
            "node.list",
            "node.pull",
            "node.remove",
            "node.spawn",
            "node.status",
            "onboard",
            "pair",
            "pair.reject",
            "pair.watch",
            "project.add",
            "project.edit",
            "project.lead",
            "project.list",
            "project.remove",
            "resurrect",
            "schema",
            "secrets.add",
            "secrets.allow-remote-origin",
            "secrets.approve",
            "secrets.automate",
            "secrets.dismiss",
            "secrets.enroll",
            "secrets.exec",
            "secrets.expose",
            "secrets.grant",
            "secrets.migrate",
            "secrets.pending",
            "secrets.put",
            "secrets.revoke",
            "secrets.rm",
            "secrets.serve",
            "secrets.set-totp",
            "secrets.status",
            "secrets.watch",
            "send",
            "session",
            "session.bind",
            "session.codex.sync",
            "session.kill",
            "session.project",
            "session.end",
            "session.find",
            "session.grant",
            "session.hook",
            "session.pending.approve",
            "session.pending.deny",
            "session.pending.list",
            "session.permit",
            "session.phase",
            "session.prune",
            "session.reap",
            "session.show",
            "session.start",
            "session.trace",
            "session.watch",
            "session.window.attach",
            "session.window.closed",
            "session.window.roster",
            "context",
            "soundcheck",
            "spawn",
            "update",
            "usage",
            "workspace.clear",
            "workspace.root",
            "workspace.list",
            "workspace.set",
        ];
        expected.sort();

        assert_eq!(got, expected, "command path set drifted from the golden snapshot");
    }

    /// Every MCP-open command passes the door gate when given nothing: any
    /// refusal it returns is about its own arguments, never the door.
    #[test]
    fn every_mcp_open_command_passes_the_door_gate() {
        use aoide_protocol::audit::Door;
        let r = crate::commands::all();
        for c in r.commands().filter(|c| c.opens(Door::Mcp)) {
            let mut inv = aoide_protocol::Invocation {
                path: c.path.iter().map(|s| s.to_string()).collect(),
                args: vec![],
                flags: Default::default(),
                door: Door::Mcp,
            };
            if let Err(refusal) = c.check("aoide", &mut inv) {
                assert!(!refusal.what.contains("is not open to the mcp door"), "{}: {}", c.dotted(), refusal.what);
            }
        }
    }

    /// The state-changing commands an MCP caller must never reach are refused
    /// by the registry (the handler never runs), and the whole state root is
    /// byte-identical afterwards. The audit line is the one thing that moves.
    #[test]
    fn the_mcp_door_refuses_the_state_changing_commands_and_writes_nothing() {
        use aoide_protocol::audit::Door;
        use std::collections::BTreeMap;
        use std::path::{Path, PathBuf};

        fn snapshot(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    snapshot(&p, out);
                } else if let Ok(bytes) = std::fs::read(&p) {
                    out.insert(p, bytes);
                }
            }
        }

        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = aoide_test_support::short_tmp("mcp-door-state");
        std::fs::create_dir_all(root.join("home")).unwrap();
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "STRILLIX_ROOT",
            "AOIDE_AUDIT_LOG",
            "AOIDE_STAGE_DIR",
            "AOIDE_SECRETS_HOME",
            "XDG_RUNTIME_DIR",
            "HOME",
            "AOIDE_SESSION_ID",
        ]);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_SECRETS_HOME", root.join("secrets"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
        std::env::set_var("HOME", root.join("home"));
        std::env::remove_var("AOIDE_SESSION_ID");

        let cases: [(&[&str], &[&str], &[(&str, &str)]); 7] = [
            (&["mesh", "join"], &["home"], &[("operator", "ed25519:00"), ("replace", "true")]),
            (&["node", "allow"], &["peer"], &[]),
            (&["session", "end"], &[], &[("id", "x")]),
            (&["session", "pending", "approve"], &["0"], &[]),
            (&["config", "set"], &["upkeep.verifyCommand", "true"], &[]),
            (&["mail", "ring"], &["worker"], &[]),
            (&["send"], &["ship it"], &[("id", "x"), ("yes", "true")]),
        ];
        for (path, args, flags) in cases {
            let inv = aoide_protocol::Invocation {
                path: path.iter().map(|s| s.to_string()).collect(),
                args: args.iter().map(|s| s.to_string()).collect(),
                flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                door: Door::Mcp,
            };
            let log = crate::dispatch::audit_log_path(&inv);
            let mut before = BTreeMap::new();
            snapshot(&root, &mut before);
            before.remove(&log);

            let outcome = crate::dispatch::dispatch(&inv);
            assert_eq!(outcome.status, aoide_protocol::output::Status::Usage, "{path:?}: {}", outcome.message);
            assert!(outcome.message.contains("is not open to the mcp door"), "{path:?}: {}", outcome.message);

            let mut after = BTreeMap::new();
            snapshot(&root, &mut after);
            after.remove(&log);
            assert_eq!(before, after, "{path:?} changed state over the mcp door");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The MCP door reads mail through the attested subtree only: the four
    /// read commands answer it, `--all-names` never does, and the mail-send
    /// family stays CLI-only (MAIL.md "Reading over MCP").
    #[test]
    fn the_mcp_door_offers_the_mail_reads_and_closes_all_names() {
        let r = crate::commands::all();
        let tools = aoide_server::mcp::tool_list(&r);
        let list = tools["tools"].as_array().unwrap();
        let names: Vec<&str> = list.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for want in ["mail", "mail.read", "mail.show", "mail.mark"] {
            assert!(names.contains(&want), "{want} must be an MCP tool: {names:?}");
        }
        assert!(!names.contains(&"mail.send"), "mail.send stays CLI-only");
        let read = list.iter().find(|t| t["name"] == "mail.read").unwrap();
        assert!(read["inputSchema"]["properties"].get("all-names").is_none(), "--all-names is not an MCP argument");
    }

    /// An MCP mail read with no attested session is refused with the taught
    /// reason and reads nothing: the letter's text never reaches the caller.
    #[test]
    fn an_unattested_mcp_mail_read_is_taught_and_reads_nothing() {
        use aoide_protocol::audit::Door;
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = aoide_test_support::short_tmp("mcp-mail-unattested");
        std::fs::create_dir_all(root.join("home")).unwrap();
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_AUDIT_LOG", "AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "HOME", "AOIDE_SESSION_ID"]);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
        std::env::set_var("HOME", root.join("home"));
        std::env::remove_var("AOIDE_SESSION_ID");
        aoide_storage::mail::file_letter("alice", "victim", "the victim's secret words").unwrap();

        let inv = aoide_protocol::Invocation {
            path: vec!["mail".into(), "read".into()],
            args: vec![],
            flags: [("for".to_string(), "victim".to_string())].into_iter().collect(),
            door: Door::Mcp,
        };
        let outcome = crate::dispatch::dispatch(&inv);
        assert!(outcome.message.contains("attested session"), "unattested MCP read must be taught: {}", outcome.message);
        assert!(!outcome.message.contains("secret words"), "{}", outcome.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--all-names` is a CLI-only flag: over MCP its flag door refuses it
    /// before the handler runs.
    #[test]
    fn mcp_all_names_is_refused_by_its_flag_door() {
        use aoide_protocol::audit::Door;
        let inv = aoide_protocol::Invocation {
            path: vec!["mail".into(), "read".into()],
            args: vec![],
            flags: [("all-names".to_string(), "true".to_string())].into_iter().collect(),
            door: Door::Mcp,
        };
        let outcome = crate::dispatch::dispatch(&inv);
        assert!(outcome.message.contains("`--all-names` is not open to the mcp door"), "{}", outcome.message);
    }

    /// The MCP tool list is the MCP-open set, name for name (CONTRACTS.md §3).
    #[test]
    fn the_mcp_tool_list_is_exactly_the_mcp_open_set() {
        let r = crate::commands::all();
        let tools = aoide_server::mcp::tool_list(&r);
        let mut names: Vec<String> = tools["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        names.sort();
        let mut expected = vec!["graph", "guide", "mail", "mail.mark", "mail.read", "mail.show", "schema", "send", "session", "session.find", "session.watch", "soundcheck", "spawn"];
        expected.sort();
        assert_eq!(names, expected);
    }

    /// The A2A card lists the commands that declare the A2A door, and only
    /// those: `send` and `spawn` (U5). A2A still routes both by hand.
    #[test]
    fn the_a2a_card_lists_the_commands_that_declare_a2a() {
        let r = crate::commands::all();
        let card = aoide_server::a2a::agent_card(&r, "127.0.0.1", 8710);
        let mut skills: Vec<&str> = card["skills"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
        skills.sort();
        assert_eq!(skills, ["send", "spawn"]);
    }

    /// `mcp serve` reports the same count the tool list shows.
    #[test]
    fn mcp_serve_counts_only_the_mcp_open_commands() {
        let inv = aoide_protocol::Invocation {
            path: vec!["mcp".into(), "serve".into()],
            args: vec![],
            flags: Default::default(),
            door: aoide_protocol::audit::Door::Cli,
        };
        let outcome = crate::dispatch::dispatch(&inv);
        assert_eq!(outcome.data.as_ref().unwrap()["toolCount"], 13, "{outcome:?}");
    }

    /// Golden snapshot: the commands an MCP caller may list and call. Every
    /// `doors` change updates this list in the same commit (crates/AGENTS.md).
    #[test]
    fn mcp_open_paths_match_the_golden_snapshot() {
        let r = crate::commands::all();
        let mut got: Vec<String> = r.commands().filter(|c| c.opens(aoide_protocol::audit::Door::Mcp)).map(|c| c.dotted()).collect();
        got.sort();
        let mut expected = vec!["graph", "guide", "mail", "mail.mark", "mail.read", "mail.show", "schema", "send", "session", "session.find", "session.watch", "soundcheck", "spawn"];
        expected.sort();
        assert_eq!(got, expected, "MCP-open set drifted from the golden snapshot");
    }

    /// Golden snapshot: the verbs that approve or deny a hold, re-parent,
    /// answer for a person or type into a session, or reach a check that
    /// refuses a door that cannot approve (`approves` in
    /// protocol/src/registry.rs). The desktop app's floor refuses a grant
    /// that names one of them.
    #[test]
    fn approves_paths_match_the_golden_snapshot() {
        let r = crate::commands::all();
        let mut got: Vec<String> = r.commands().filter(|c| c.approves).map(|c| c.dotted()).collect();
        got.sort();
        let mut expected = vec![
            "conduct",
            "graph.link",
            "job.add",
            "job.remove",
            "mail.ring",
            "mail.send",
            "node.spawn",
            "resurrect",
            "send",
            "session.hook",
            "session.pending.approve",
            "session.pending.deny",
            "session.permit",
            "session.start",
            "spawn",
        ];
        expected.sort();
        assert_eq!(got, expected, "approves set drifted from the golden snapshot");
    }

    /// Golden snapshot: the verbs that start an agent, or arm a timer, a sweep
    /// or a listener that starts a program later (`launches`,
    /// protocol/src/registry.rs). The desktop app's floor refuses a grant that
    /// names one of them.
    #[test]
    fn launches_paths_match_the_golden_snapshot() {
        let r = crate::commands::all();
        let mut got: Vec<String> = r.commands().filter(|c| c.launches).map(|c| c.dotted()).collect();
        got.sort();
        let mut expected = vec!["a2a.serve", "conduct", "job.add", "node.spawn", "project.add", "resurrect", "session.pending.approve", "spawn"];
        expected.sort();
        assert_eq!(got, expected, "launches set drifted from the golden snapshot");
    }

    /// Findings 2 and 10: A2A opens no registry command, because its card
    /// describes `send` and `spawn` itself. The daemon hop keeps the serve
    /// commands it had before the door policy, so the Daemon door still reaches
    /// them, with the same Ok answer the handler always gave there.
    #[test]
    fn a2a_opens_no_registry_command_and_the_daemon_keeps_its_serve_commands() {
        let r = crate::commands::all();
        assert!(r.commands().all(|c| !c.opens(aoide_protocol::audit::Door::A2a)), "A2A declares no registry command");
        for path in [["a2a", "serve"], ["mail", "serve"]] {
            let path: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            let c = r.get(&path).expect("the serve command is registered");
            assert!(c.opens(aoide_protocol::audit::Door::Daemon), "{path:?} keeps the Daemon door");
            assert!(!c.opens(aoide_protocol::audit::Door::Mcp), "{path:?} stays closed to MCP");
        }
    }

    #[test]
    fn every_example_parses() {
        aoide_test_support::registry_walk::every_example_parses("aoide", &crate::commands::all());
    }

    #[test]
    fn required_is_enforced() {
        aoide_test_support::registry_walk::required_is_enforced("aoide", &crate::commands::all());
    }

    #[test]
    fn defaults_applied() {
        aoide_test_support::registry_walk::defaults_applied("aoide", &crate::commands::all());
    }

    #[test]
    fn brief_fits() {
        aoide_test_support::registry_walk::brief_fits(&crate::commands::all());
    }

    #[test]
    fn every_head_is_sectioned() {
        aoide_test_support::registry_walk::every_head_is_sectioned(&crate::commands::all());
    }

    #[test]
    fn listings_fit_and_hang() {
        aoide_test_support::registry_walk::listings_fit_and_hang("aoide", &crate::commands::all());
    }

    #[test]
    fn suggestion_or_list() {
        aoide_test_support::registry_walk::suggestion_or_list("aoide", &crate::commands::all());
    }

    /// The cross-binary hint reads a static head list (`door::*_HEADS`); this
    /// pins this binary's half of it to the registry, so a head moving
    /// between binaries fails here instead of misdirecting a typo.
    #[test]
    fn cross_binary_heads_match_the_registry() {
        use aoide_protocol::door::{AOIDE_ONLY_HEADS, SHARED_HEADS};
        let r = crate::commands::all();
        let mut heads: Vec<&str> = r.commands().map(|c| c.path[0]).collect();
        heads.sort();
        heads.dedup();
        let mut expected: Vec<&str> = AOIDE_ONLY_HEADS.iter().chain(SHARED_HEADS).copied().collect();
        expected.sort();
        assert_eq!(heads, expected, "AOIDE_ONLY_HEADS + SHARED_HEADS must be exactly this binary's heads");
    }

    /// The declarative fields are additive (CONTRACTS.md §3): a command that
    /// declares none serializes without their keys, and the three migrated
    /// commands carry exactly what they declare.
    #[test]
    fn migrated_declarations_serialize_and_the_rest_stay_byte_identical() {
        let r = crate::commands::all();
        let find = |p: &str| serde_json::to_value(r.commands().find(|c| c.dotted() == p).unwrap()).unwrap();

        let add = find("secrets.add");
        let flags = add["flags"].as_array().unwrap();
        let key = flags.iter().find(|f| f["name"] == "key").unwrap();
        assert!(key.get("required").is_none(), "age defaults the key, so the registry cannot require it");
        assert_eq!(key["value"], "key");
        assert!(key["description"].as_str().unwrap().contains("(default for age: the secret's name)"));
        let exec = find("secrets.exec");
        let required: Vec<&str> =
            exec["flags"].as_array().unwrap().iter().filter(|f| f["required"] == true).map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(required, ["as", "secret"]);
        let automate = find("secrets.automate");
        assert_eq!(automate["args"][1]["values"], serde_json::json!(["on", "off", "grant", "revoke"]));
        assert_eq!(automate["args"][2]["requiredAfter"], serde_json::json!(["grant", "revoke"]));
        let backend = flags.iter().find(|f| f["name"] == "backend").unwrap();
        assert_eq!(backend["default"], "age");
        assert!(backend.get("required").is_none());

        let send = find("send");
        assert_eq!(send["oneOf"], serde_json::json!([["id", "to"]]));
        let to = send["flags"].as_array().unwrap().iter().find(|f| f["name"] == "to").unwrap();
        assert_eq!(to["conflicts"], serde_json::json!(["id"]));

        let rm = find("project.list");
        for k in ["oneOf", "brief"] {
            assert!(rm.get(k).is_none(), "an unmigrated command grows no `{k}` key: {rm}");
        }
        for f in rm["flags"].as_array().unwrap() {
            for k in ["value", "required", "default", "values", "conflicts"] {
                assert!(f.get(k).is_none(), "an unmigrated flag grows no `{k}` key: {f}");
            }
        }
    }
}
