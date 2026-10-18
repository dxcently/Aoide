//! Metadata-only registration for strillix's one remaining walking-skeleton stub
//! command, `rice transpose`, plus `rice declare`'s real handler — the two
//! sat together as `register_rice_late` (P-A4 plan, same relative position
//! core's `commands/mod.rs::all()` gave them) before `declare` graduated
//! (L-C2, task #107): its arg parsing/schema were already real, only the
//! live-system action was missing, and that action is now just the
//! composed-song copy step below. `dispatch()` still short-circuits any
//! `implemented == false` command before reaching a handler, so
//! `rice transpose` alone still shares the placeholder.
//!
//! Only `rice declare`/`rice transpose` sit in strillix (P-A4 plan) — the
//! `content`/`make`/`update`/`onboard` stub groups in `aoide-cli`'s own
//! `commands/stubs.rs` are core identity and stay there untouched.

use crate::dispatch::Invocation;
use crate::output::{Fix, Kind, Outcome};
use crate::registry::{arg, cmd, flag, Registry};
use serde_json::json;
use aoide_protocol::Door;

/// Never invoked — `dispatch()` returns the not-implemented envelope itself
/// for any command with `implemented: false`, without calling `handler`.
fn unimplemented(_inv: &Invocation) -> Outcome {
    unreachable!("dispatch() never calls the handler of a not-implemented command")
}

/// `rice declare` / `rice transpose` — sit after `cover set`/`livery` in the
/// historical order (same relative position core's `commands/mod.rs::all()`
/// gives them).
pub fn register_rice_late(r: &mut Registry) {
    r.insert(cmd!(
        path: ["rice", "declare"],
        summary: "Copy a composed song from $AOIDE_ROOT/rices/<name> into the checkout's rices/<name>, so it can be committed, less the machine's top-level history/ and drafts/. Refuses while git shows uncommitted changes in that folder, when the checkout is not a git repository (git history is the archive), or when the staged rice.nix is the old function-module shape (`strillix rice refresh <name>`). With --host, instead rewrite the one `habit.song.declared = \"…\";` line in this host's config (aoide.hostConfig) to <name>, a song this host's build carries; it takes effect on the next rebuild, and warns when the checkout's song folder is dirty. The gated rebuild itself is the user's own git commit + nix switch, never this command.",
        args: [arg!("name", "string", true, "Rice/song name to declare.")],
        flags: [flag!("host", "bool", "Rewrite this host's declared-song line to <name> instead of copying the song into the checkout. CLI-only: it changes what the next rebuild performs.", doors: &[Door::Cli])],
        gated: true,
        implemented: true,
        handler: handle_rice_declare,
        examples: ["rice declare moonlight", "rice declare barcarolle --host"],
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "transpose"],
        summary: "Replay a song in another key (palette) from the song's rices/<song>/palette/.",
        args: [
            arg!("rice", "string", true, "Source song name."),
            arg!("palette", "string", true, "Key/palette name to transpose into."),
        ],
        flags: [],
        gated: false,
        implemented: false,
        handler: unimplemented,
    ));
}

/// `rice declare <name>` — the copy half of the stage-vs-commit split (house
/// rule 2: the user gates the rebuild, never this command). Copies the
/// composed song `$AOIDE_ROOT/rices/<name>/` into the checkout's
/// `rices/<name>/` (`aoide_storage::fs::flake_root()`,
/// `$AOIDE_FLAKE_ROOT` override, default `<home>/Aoide`) — composing lives
/// in the runtime root, committing lives in the checkout git tracks, and
/// this is the seam between them. The machine's own top-level `history/` and
/// `drafts/` (`strillix_rice::widgets::MACHINE_RUNTIME_DIRS`) stay behind: the
/// checkout carries the song, not the machine's undo history and scratch.
/// Nothing beyond the copy: no `git add`, no rebuild proposal, no `nix eval`
/// — those stay the user's own git/rebuild-gate steps. Overwrites only files
/// whose bytes actually differ (never touches a byte-identical destination
/// file), so a repeat declare with nothing new is a true no-op.
///
/// The copy refuses while git shows uncommitted work in the folder, and names
/// it (`strillix_rice::checkout_gate`), and refuses a staged `rice.nix` that is
/// still the old function-module shape (`old-song-shape`). `--host` is the other declare step,
/// `strillix_rice::host_declare`: it points the host's `habit.song.declared` line
/// at `<name>` instead of copying any song, never refuses for git, and warns
/// when the folder is dirty. It too stops short of git and the rebuild.
fn handle_rice_declare(inv: &Invocation) -> Outcome {
    let name = match inv.args.first() {
        Some(n) => n.clone(),
        None => {
            return Outcome::usage("rice.declare", "usage: aoide rice declare <name> [--json]")
                .with_data(json!({ "reason": "missing-name" }));
        }
    };

    if inv.flag_present("host") {
        return strillix_rice::host_declare::declare_on_host(&name);
    }

    // `name` is joined unsanitized into both `rice_dir` (the runtime
    // root) and the checkout's `rices/` below — without this check a
    // `..`-shaped name escapes both roots (arbitrary read from `src`,
    // arbitrary write to `dst`). Same validator `rice compose`/`rice mode`
    // already gate on (`strillix_rice::compose::valid_song_name`,
    // `^[a-z0-9][a-z0-9-]*$` — rejects `..`/`/` by construction), checked
    // BEFORE either path is built, not after.
    if !strillix_rice::compose::valid_song_name(&name) {
        return Outcome::error(
            "rice.declare",
            format!(
                "`{name}` is not a valid song name: must match `^[a-z0-9][a-z0-9-]*$` \
                 (lowercase letters, digits, hyphens; no leading hyphen, no `/`, no `..`)"
            ),
        )
        .with_data(json!({ "reason": "invalid-name", "name": name }));
    }

    let src = aoide_storage::fs::rice_dir(&name);
    if !src.is_dir() {
        return Outcome::error(
            "rice.declare",
            format!(
                "no composed song named `{name}` at {} — run `strillix rice compose {name}` first",
                src.display()
            ),
        )
        .with_data(json!({ "reason": "no-composed-song", "name": name, "expected": src.to_string_lossy() }));
    }

    let checkout = aoide_storage::fs::flake_root();
    if !checkout.is_dir() {
        return Outcome::error(
            "rice.declare",
            format!(
                "no Aoide checkout at {} ($AOIDE_FLAKE_ROOT override, default `<home>/Aoide`) — \
                 `rice declare` commits into the checkout's `rices/`, so a checkout must exist first",
                checkout.display()
            ),
        )
        .with_data(json!({ "reason": "no-checkout", "expected": checkout.to_string_lossy() }));
    }

    // The no-archive gate: the copy below overwrites the folder's bytes, so
    // git must show nothing uncommitted there first (checkout_gate's header).
    let folder = format!("rices/{name}");
    if let Err(blocked) = strillix_rice::checkout_gate::check_folder(&checkout, &folder) {
        return blocked.into_outcome("rice.declare", &checkout, &folder);
    }

    // The staged body must be a song shape v1 body: habit reads it as data, so
    // an old function-module `rice.nix` copied into the checkout breaks the build
    // (CONTRACTS.md §5). A file that cannot be read is left to the copy's own error.
    let rice = src.join("rice.nix");
    if std::fs::read_to_string(&rice).is_ok_and(|text| strillix_rice::checkout_gate::is_old_song_shape(&text)) {
        return Outcome::refuse(
            "rice.declare",
            Kind::Refused,
            format!(
                "`{}` is the old song shape (a function module), not a v1 body; run `strillix rice refresh {name}` to bring the repo's rice.nix in",
                rice.display()
            ),
            "habit reads a song body as data, so copying this file into the checkout would break the build; if the machine edited the file, rewrite it as a body (CONTRACTS.md §5)".to_string(),
            Fix::Run(format!("strillix rice refresh {name}")),
        )
        .with_fields(json!({ "reason": "old-song-shape", "name": name, "file": rice.to_string_lossy() }));
    }

    let dst = checkout.join("rices").join(&name);
    let mut changed = Vec::new();
    if let Err(e) = strillix_rice::widgets::copy_tree_atomic(
        &src,
        &dst,
        strillix_rice::widgets::MACHINE_RUNTIME_DIRS,
        &mut changed,
    ) {
        return Outcome::error(
            "rice.declare",
            format!(
                "copying {} into {} failed at {}: {}",
                src.display(),
                dst.display(),
                e.target,
                e.error
            ),
        )
        .with_data(json!({ "reason": "copy-failed", "name": name }));
    }

    let message = if changed.is_empty() {
        format!("`{name}` already matches {} — nothing to copy", dst.display())
    } else {
        format!("copied {} file(s) into {}", changed.len(), dst.display())
    };
    Outcome::ok("rice.declare", message)
        .gated(true)
        .changed(changed)
        .with_data(json!({ "name": name, "checkout": dst.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    fn inv(args: &[&str]) -> Invocation {
        Invocation {
            path: vec!["rice".into(), "declare".into()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: Default::default(),
            door: aoide_protocol::Door::Cli,
        }
    }

    /// `--host` takes the other road: the host file is rewritten and the
    /// checkout's rices is never touched (the engine's own cases live in
    /// `strillix_rice::host_declare`).
    #[test]
    fn host_flag_rewrites_the_host_line_and_copies_nothing() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT", "AOIDE_HOST_CONFIG", "STRILLIX_TEMPLATES"]);
        let root = unique_tmp("declare-host");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("song").join("stage"));
        let checkout = root.join("checkout");
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);
        let host = root.join("default.nix");
        std::fs::write(&host, "{\n  habit.song.declared = \"sonata\";\n}\n").unwrap();
        std::env::set_var("AOIDE_HOST_CONFIG", &host);
        let templates = root.join("templates");
        std::fs::create_dir_all(templates.join("dusk")).unwrap();
        std::fs::write(templates.join("dusk").join("rice.nix"), "{ ... }: { }\n").unwrap();
        std::fs::write(templates.join("builtin.json"), r#"{"declared":"sonata","songs":["sonata","dusk"],"packages":[]}"#).unwrap();
        std::env::set_var("STRILLIX_TEMPLATES", &templates);

        let mut i = inv(&["dusk"]);
        i.flags.insert("host".into(), "true".into());
        let out = handle_rice_declare(&i);
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        assert_eq!(std::fs::read_to_string(&host).unwrap(), "{\n  habit.song.declared = \"dusk\";\n}\n");
        assert!(!checkout.exists(), "--host copies no song into the checkout");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--host` declares a dirty song folder anyway and says so in a warning
    /// field, naming the folder: it never refuses for git.
    #[test]
    fn host_flag_warns_about_a_dirty_song_folder_and_still_declares() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT", "AOIDE_HOST_CONFIG", "STRILLIX_TEMPLATES"]);
        let root = unique_tmp("declare-host-warn");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("song").join("stage"));
        let checkout = root.join("checkout");
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);
        let folder = checkout.join("rices").join("dusk");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("livery.json"), "{}").unwrap();
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        aoide_test_support::git_in(&checkout, &["add", "-A"]);
        aoide_test_support::git_in(&checkout, &["commit", "-qm", "dusk"]);
        std::fs::write(folder.join("livery.json"), "{\"a\":1}").unwrap();

        let host = root.join("default.nix");
        std::fs::write(&host, "{\n  habit.song.declared = \"sonata\";\n}\n").unwrap();
        std::env::set_var("AOIDE_HOST_CONFIG", &host);
        let templates = root.join("templates");
        std::fs::create_dir_all(templates.join("dusk")).unwrap();
        std::fs::write(templates.join("dusk").join("rice.nix"), "{ ... }: { }\n").unwrap();
        std::fs::write(templates.join("builtin.json"), r#"{"declared":"sonata","songs":["sonata","dusk"],"packages":[]}"#).unwrap();
        std::env::set_var("STRILLIX_TEMPLATES", &templates);

        let mut i = inv(&["dusk"]);
        i.flags.insert("host".into(), "true".into());
        let out = handle_rice_declare(&i);
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        assert_eq!(std::fs::read_to_string(&host).unwrap(), "{\n  habit.song.declared = \"dusk\";\n}\n");
        let data = out.data.expect("the declare carries data");
        assert!(data["warning"].as_str().unwrap().contains("rices/dusk"));
        assert_eq!(data["uncommitted"][0], " M rices/dusk/livery.json");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The copy refuses while git shows uncommitted work in the folder, names
    /// the folder, and leaves the bytes it would have overwritten alone.
    #[test]
    fn copy_refuses_a_dirty_song_folder_and_leaves_it_alone() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-dirty");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(&rices).unwrap();
        std::fs::write(rices.join("livery.json"), "{\"runtime\":true}").unwrap();

        let checkout = unique_tmp("declare-dirty-checkout");
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);
        let folder = checkout.join("rices").join("dusk");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("livery.json"), "{\"committed\":true}").unwrap();
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        aoide_test_support::git_in(&checkout, &["add", "-A"]);
        aoide_test_support::git_in(&checkout, &["commit", "-qm", "dusk"]);
        std::fs::write(folder.join("livery.json"), "{\"hand-edit\":true}").unwrap();

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Error, "{out:?}");
        let data = out.data.expect("a refusal carries data");
        assert_eq!(data["reason"], "dirty-folder");
        assert!(out.message.contains("rices/dusk"), "{}", out.message);
        assert_eq!(std::fs::read_to_string(folder.join("livery.json")).unwrap(), "{\"hand-edit\":true}");

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }

    #[test]
    fn the_host_flag_is_cli_only() {
        let mut r = Registry::new();
        register_rice_late(&mut r);
        let declare = r.commands().find(|c| c.path == ["rice", "declare"]).expect("rice declare");
        let host = declare.flags.iter().find(|f| f.name == "host").expect("--host");
        assert_eq!(host.doors, &[Door::Cli]);
    }

    #[test]
    fn missing_name_is_usage() {
        let out = handle_rice_declare(&inv(&[]));
        assert_eq!(out.status, crate::output::Status::Usage);
    }

    /// A traversal-shaped name must be refused BEFORE it ever reaches
    /// `rice_dir`/the checkout join — same pattern
    /// `song/src/commands/mode.rs`'s
    /// `current_staged_song_rejects_a_hand_edited_path_traversal_song_field`
    /// pins for the sibling stage-file case. No env override needed: the
    /// name check runs before any path is even built.
    #[test]
    fn path_traversal_shaped_name_is_refused_before_any_path_is_built() {
        let out = handle_rice_declare(&inv(&["../../evil"]));
        assert_eq!(out.status, crate::output::Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "invalid-name");
    }

    #[test]
    fn no_composed_song_is_a_taught_error() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-nosong");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        std::env::set_var("AOIDE_FLAKE_ROOT", unique_tmp("declare-nosong-checkout"));

        let out = handle_rice_declare(&inv(&["nope"]));
        assert_eq!(out.status, crate::output::Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-composed-song");

        let _ = std::fs::remove_dir_all(&stage_root);
    }

    #[test]
    fn no_checkout_is_a_taught_error() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-nocheckout");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(&rices).unwrap();
        std::fs::write(rices.join("livery.json"), "{}").unwrap();
        let missing_checkout = unique_tmp("declare-nocheckout-missing");
        std::fs::remove_dir_all(&missing_checkout).unwrap(); // unique_tmp creates it; this test wants it ABSENT
        std::env::set_var("AOIDE_FLAKE_ROOT", &missing_checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-checkout");

        let _ = std::fs::remove_dir_all(&stage_root);
    }

    /// An old function-module `rice.nix` is refused before any byte is copied:
    /// the checkout's song folder is never created.
    #[test]
    fn an_old_shape_rice_nix_is_refused_and_nothing_is_copied() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-old-shape");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(&rices).unwrap();
        std::fs::write(rices.join("livery.json"), "{}").unwrap();
        std::fs::write(rices.join("rice.nix"), "{ lib, config, ... }:\n{ config = { }; }\n").unwrap();

        let checkout = unique_tmp("declare-old-shape-checkout");
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Error, "{out:?}");
        let data = out.data.expect("a refusal carries data");
        assert_eq!(data["refusal"]["kind"], "refused");
        assert_eq!(data["reason"], "old-song-shape");
        assert_eq!(data["refusal"]["fix"]["run"], "strillix rice refresh dusk");
        assert!(out.message.contains("rice.nix"), "{}", out.message);
        assert!(!checkout.join("rices").join("dusk").exists(), "nothing is copied");

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }

    /// A v1 body copies like any song.
    #[test]
    fn a_v1_body_passes_the_shape_check_and_is_copied() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-v1-body");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(&rices).unwrap();
        std::fs::write(rices.join("livery.json"), "{}").unwrap();
        std::fs::write(rices.join("rice.nix"), "{\n  description = \"dusk\";\n  home = { members = [ \"strillix\" ]; };\n}\n").unwrap();

        let checkout = unique_tmp("declare-v1-body-checkout");
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        assert!(checkout.join("rices").join("dusk").join("rice.nix").exists());

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }

    #[test]
    fn copies_the_composed_song_into_the_checkout_and_is_idempotent() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-copy");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(rices.join("widgets")).unwrap();
        std::fs::write(rices.join("livery.json"), "{\"a\":1}").unwrap();
        std::fs::write(rices.join("widgets").join(".gitkeep"), "").unwrap();

        let checkout = unique_tmp("declare-copy-checkout");
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        assert!(out.gated, "rice.declare stays gated metadata");
        let dst = checkout.join("rices").join("dusk");
        assert_eq!(std::fs::read_to_string(dst.join("livery.json")).unwrap(), "{\"a\":1}");
        assert!(dst.join("widgets").join(".gitkeep").exists());
        assert_eq!(out.changed.len(), 2);

        // Re-declaring copies nothing once the copy is committed: byte-identical
        // files are left untouched, and nothing uncommitted stands in the way.
        aoide_test_support::git_in(&checkout, &["add", "-A"]);
        aoide_test_support::git_in(&checkout, &["commit", "-qm", "dusk"]);
        let out2 = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out2.status, crate::output::Status::Ok);
        assert!(out2.changed.is_empty(), "byte-identical re-declare copies nothing");

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }

    #[test]
    fn the_machines_takes_and_drafts_never_reach_the_checkout() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-scratch");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        std::fs::create_dir_all(rices.join("history")).unwrap();
        std::fs::create_dir_all(rices.join("drafts").join("neon")).unwrap();
        std::fs::create_dir_all(rices.join("widgets").join("history")).unwrap();
        std::fs::create_dir_all(rices.join("elements").join("x")).unwrap();
        std::fs::write(rices.join("livery.json"), "{}").unwrap();
        std::fs::write(rices.join("history").join("head.json"), "{}").unwrap();
        std::fs::write(rices.join("drafts").join("neon").join("livery.json"), "{}").unwrap();
        std::fs::write(rices.join("widgets").join("history").join("Bar.qml"), "").unwrap();
        std::fs::write(rices.join("elements").join("x").join("element.json"), "{}").unwrap();

        let checkout = unique_tmp("declare-scratch-checkout");
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        let dst = checkout.join("rices").join("dusk");
        assert!(dst.join("livery.json").exists());
        assert!(!dst.join("history").exists(), "history/ is the machine's undo history");
        assert!(!dst.join("drafts").exists(), "drafts/ is the machine's scratch");
        assert!(
            dst.join("widgets").join("history").join("Bar.qml").exists(),
            "only the song folder's top-level scratch names are skipped"
        );
        assert!(
            dst.join("elements").join("x").join("element.json").exists(),
            "elements/ is song-authored input and is carried"
        );

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }

    #[test]
    fn a_takes_symlink_is_skipped_and_does_not_fail_the_declare() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_FLAKE_ROOT"]);
        let stage_root = unique_tmp("declare-takes-link");
        std::env::set_var("AOIDE_STAGE_DIR", stage_root.join("song").join("stage"));
        let rices = stage_root.join("song").join("rices").join("dusk");
        let elsewhere = stage_root.join("elsewhere");
        std::fs::create_dir_all(&rices).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(rices.join("livery.json"), "{}").unwrap();
        std::fs::write(elsewhere.join("head.json"), "{}").unwrap();
        std::os::unix::fs::symlink(&elsewhere, rices.join("history")).unwrap();

        let checkout = unique_tmp("declare-takes-link-checkout");
        aoide_test_support::git_in(&checkout, &["init", "-q"]);
        std::env::set_var("AOIDE_FLAKE_ROOT", &checkout);

        let out = handle_rice_declare(&inv(&["dusk"]));
        assert_eq!(out.status, crate::output::Status::Ok, "{out:?}");
        let dst = checkout.join("rices").join("dusk");
        assert!(dst.join("livery.json").exists());
        assert!(
            std::fs::symlink_metadata(dst.join("history")).is_err(),
            "a takes symlink is skipped by name, never followed or copied"
        );

        let _ = std::fs::remove_dir_all(&stage_root);
        let _ = std::fs::remove_dir_all(&checkout);
    }
}
