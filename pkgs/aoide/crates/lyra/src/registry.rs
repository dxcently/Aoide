//! The self-registering command registry — re-exported from `aoide-protocol`
//! (same seam `aoide-cli`'s own `registry.rs` uses) so every
//! `crate::registry::*` caller here reads exactly like the core crate's.
//!
//! Golden snapshot: the sorted list of every command path lyra registers —
//! the sole authority for lyra's command set, no count tracked elsewhere.
//! Mirrors `aoide-cli`'s `registry.rs` test module — same invariant checks.
//! P-A4 minted the base bundle; P-I3 added `onboard`; P3 added `secrets
//! ask`; L-E1 added `element seed`; P-PV3 added `pair ask`, then its own
//! revert added `pair confirm`; `quickshell healthcheck` — the
//! placeholder-screen watchdog — followed; R2 (the mutual-code redesign's
//! popup phase) repurposed `pair confirm` into `pair show`, the ceremony's
//! reply-code display dialog (net zero — one path dies, one lands). `lyra
//! reload` (design settled 2026-08-31) repurposed `quickshell.reload` into
//! `reload`, the one mode-aware iteration command — the SAME net-zero swap
//! shape. P1 added `preview`, `preview.set` (an isolated quickshell canvas
//! for one widget); a same-lane follow-up added `preview.declare` (the
//! canvas's counterpart of `rice declare`). P6 added `preview.shot`,
//! `preview.tree`, `preview.notes` — shell-first agent tools over that same
//! canvas (a screenshot, the live/static-joined item tree, and a
//! scaffolding notes store). I1 added `icon.collections`, `icon.list`,
//! `icon.resolve` (the pinned icon collections, resolved into the lane's own
//! SVG tree). See `commands/mod.rs::all()` for the assembly
//! order.

pub use aoide_protocol::registry::*;

#[cfg(test)]
mod tests {
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
            .find(|c| c.dotted() == "rice.compose")
            .expect("rice compose carries examples");
        let v = serde_json::to_value(with).unwrap();
        assert_eq!(v["examples"][0], "rice compose moonlight --from sonata");

        let without = r
            .commands()
            .find(|c| c.dotted() == "rice.lint")
            .expect("rice lint carries none");
        let v = serde_json::to_value(without).unwrap();
        assert!(
            v.get("examples").is_none(),
            "no examples → no key (byte-identical schema): {v}"
        );
    }

    /// `external` is additive too (task #138, CONTRACTS.md §3), mirroring
    /// `aoide-cli`'s own `registry.rs` twin: a host with zero `lyra-*`
    /// plugins on `PATH` gets a `schema --json` byte-identical to before the
    /// field existed — no `external` key at all — while dropping one on a
    /// scoped `PATH` surfaces it by name, resolved command spelling, and
    /// path.
    #[test]
    fn external_key_is_absent_unless_a_plugin_is_on_path() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_lyra_external_schema_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("PATH", &dir);

        let r = crate::commands::all();
        let v = serde_json::to_value(r.schema("lyra")).unwrap();
        assert!(v.get("external").is_none(), "no plugins on PATH -> no key at all: {v}");

        let plugin = dir.join("lyra-clipboard-copy");
        std::fs::write(&plugin, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&plugin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&plugin, perms).unwrap();
        }

        let v = serde_json::to_value(r.schema("lyra")).unwrap();
        assert_eq!(v["external"][0]["name"], "clipboard-copy");
        assert_eq!(v["external"][0]["command"], "lyra-clipboard-copy");
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
    /// removing, or renaming a command is a reviewable diff here — this is
    /// lyra's own golden, independent of the core crate's.
    #[test]
    fn command_paths_match_the_golden_snapshot() {
        let r = crate::commands::all();
        let mut got: Vec<String> = r.commands().map(|c| c.dotted()).collect();
        got.sort();

        let mut expected: Vec<&str> = vec![
            "apps.list",
            "apps.publish",
            "apps.show",
            "cover.set",
            "cover.sync",
            "element.seed",
            "guide",
            "herald.push",
            "icon.collections",
            "icon.list",
            "icon.resolve",
            "livery.emit",
            "livery.lint",
            "livery.resolve",
            "mcp.serve",
            "onboard",
            "pair.ask",
            "pair.show",
            "preview",
            "preview.declare",
            "preview.notes",
            "preview.set",
            "preview.shot",
            "preview.tree",
            "quickshell.healthcheck",
            "reload",
            "rice.back",
            "rice.compose",
            "rice.declare",
            "rice.draft.drop",
            "rice.draft.list",
            "rice.draft.save",
            "rice.lint",
            "rice.mode.declarative",
            "rice.mode.draft",
            "rice.mode.stage",
            "rice.mode.status",
            "rice.stage",
            "rice.take",
            "rice.take.diff",
            "rice.take.list",
            "rice.take.mark",
            "rice.take.prune",
            "rice.transpose",
            "schema",
            "screen.diff",
            "screen.info",
            "screen.ocr",
            "screen.point.click",
            "screen.point.drag",
            "screen.point.hover",
            "screen.point.idle",
            "screen.point.move",
            "screen.point.restore",
            "screen.point.save",
            "screen.point.scroll",
            "screen.point.text",
            "screen.send",
            "screen.shot",
            "secrets.ask",
            "shellbridge",
        ];
        expected.sort();

        assert_eq!(got, expected, "command path set drifted from lyra's golden snapshot");
    }

    #[test]
    fn every_example_parses() {
        aoide_test_support::registry_walk::every_example_parses("lyra", &crate::commands::all());
    }

    #[test]
    fn required_is_enforced() {
        aoide_test_support::registry_walk::required_is_enforced("lyra", &crate::commands::all());
    }

    #[test]
    fn defaults_applied() {
        aoide_test_support::registry_walk::defaults_applied("lyra", &crate::commands::all());
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
        aoide_test_support::registry_walk::listings_fit_and_hang("lyra", &crate::commands::all());
    }

    #[test]
    fn suggestion_or_list() {
        aoide_test_support::registry_walk::suggestion_or_list("lyra", &crate::commands::all());
    }

    /// The cross-binary hint reads a static head list (`door::*_HEADS`); this
    /// pins this binary's half of it to the registry, so a head moving
    /// between binaries fails here instead of misdirecting a typo.
    #[test]
    fn cross_binary_heads_match_the_registry() {
        use aoide_protocol::door::{LYRA_ONLY_HEADS, AOIDE_ONLY_HEADS, SHARED_HEADS};
        let r = crate::commands::all();
        let mut heads: Vec<&str> = r.commands().map(|c| c.path[0]).collect();
        heads.sort();
        heads.dedup();
        let mut expected: Vec<&str> = LYRA_ONLY_HEADS.iter().chain(SHARED_HEADS).copied().collect();
        expected.sort();
        assert_eq!(heads, expected, "LYRA_ONLY_HEADS + SHARED_HEADS must be exactly this binary's heads");
        assert!(AOIDE_ONLY_HEADS.iter().all(|h| !heads.contains(h)), "AOIDE_ONLY_HEADS must not appear here");
    }

    /// The declarative fields are additive (CONTRACTS.md §3): the migrated
    /// `preview shot --what` carries its closed value set and default, and an
    /// unmigrated flag grows no key.
    #[test]
    fn migrated_declarations_serialize_and_the_rest_stay_byte_identical() {
        let r = crate::commands::all();
        let find = |p: &str| serde_json::to_value(r.commands().find(|c| c.dotted() == p).unwrap()).unwrap();

        let shot = find("preview.shot");
        let what = shot["flags"].as_array().unwrap().iter().find(|f| f["name"] == "what").unwrap();
        assert_eq!(what["values"], serde_json::json!(["screen", "canvas", "widget", "element"]));
        assert_eq!(what["default"], "widget");

        let tree = find("preview.tree");
        assert!(tree.get("brief").is_none());
        for f in tree["flags"].as_array().unwrap() {
            for k in ["value", "required", "default", "values", "conflicts"] {
                assert!(f.get(k).is_none(), "an unmigrated flag grows no `{k}` key: {f}");
            }
        }
    }
}
