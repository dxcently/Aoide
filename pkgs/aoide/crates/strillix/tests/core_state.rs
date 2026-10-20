//! The strillix binary through its real entry point, against a legacy `~/Aoide`
//! tree that holds core's state. A plain start moves none of it. With an aoide
//! binary installed, the rice flag waits for core: it refuses, moving nothing,
//! until core's legacy `state/` has moved into the root. With none installed, the
//! flag moves the rice tree and leaves the legacy state where it is. Each child
//! runs under a private HOME with every root pinned inside it.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("strillix-core-state-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn strillix(sb: &Path, migrate: bool, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_strillix"));
    c.args(args)
        .env("HOME", sb.join("home"))
        .env("AOIDE_ROOT", sb.join("aoide"))
        .env("STRILLIX_ROOT", sb.join("strillix"))
        .env_remove("AOIDE_USER")
        .env_remove("AOIDE_STATE_DIR")
        .env_remove("AOIDE_AUDIT_LOG")
        .env("AOIDE_CORE_BIN", sb.join("bin").join("aoide"))
        .env("XDG_RUNTIME_DIR", sb.join("run"))
        .env("XDG_STATE_HOME", sb.join("xdg-state"))
        .env("XDG_CONFIG_HOME", sb.join("xdg-config"))
        .env("XDG_DATA_HOME", sb.join("xdg-data"))
        .env("COLUMNS", "105");
    if migrate {
        c.env("STRILLIX_MIGRATE", "1");
    } else {
        c.env_remove("STRILLIX_MIGRATE");
    }
    c.output().expect("the strillix binary runs")
}

/// Core's legacy tree as a pre-split host has it: a `state/` and the audit log.
fn legacy_core_tree(sb: &Path) -> PathBuf {
    let legacy = sb.join("home").join("Aoide");
    let state = legacy.join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(state.join("usage.json"), "{}").unwrap();
    std::fs::write(legacy.join("log"), "audit\n").unwrap();
    legacy
}

/// The old rice tree under `$AOIDE_ROOT/song`, and the lyra link Hyprland sources.
fn old_rice_tree(sb: &Path) {
    let stage = sb.join("aoide").join("song").join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
    let lyra = sb.join("home").join(".local").join("state").join("lyra");
    std::fs::create_dir_all(&lyra).unwrap();
    std::fs::write(lyra.join("rice.conf"), "source\n").unwrap();
}

/// Plants the `aoide` that every child's `AOIDE_CORE_BIN` names: aoide is
/// installed on this host. Without it that path is missing, and the children
/// see no aoide.
fn plant_aoide(sb: &Path) {
    std::fs::create_dir_all(sb.join("bin")).unwrap();
    std::fs::write(sb.join("bin").join("aoide"), "").unwrap();
}

/// Every entry under `dir` with its mode and bytes, sorted: a tree the child
/// touched in any way reads back different.
fn snapshot(dir: &Path) -> Vec<(PathBuf, u32, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, u32, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let mode = meta.permissions().mode() & 0o7777;
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if meta.is_dir() {
                out.push((rel, mode, Vec::new()));
                walk(root, &path, out);
            } else {
                out.push((rel, mode, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn a_plain_start_leaves_core_legacy_tree_untouched() {
    let sb = sandbox("plain");
    let legacy = legacy_core_tree(&sb);
    old_rice_tree(&sb);
    let before = snapshot(&legacy);

    let out = strillix(&sb, false, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(snapshot(&legacy), before, "the start changed core's legacy tree");
    assert!(!sb.join("aoide").join("state").exists(), "the start moved core's state into the root");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn the_rice_flag_waits_for_core_and_moves_nothing_until_then() {
    let sb = sandbox("wait");
    let legacy = legacy_core_tree(&sb);
    old_rice_tree(&sb);
    plant_aoide(&sb);
    let before = snapshot(&legacy);

    let refused = strillix(&sb, true, &["--help"]);
    assert!(!refused.status.success(), "the flag ran over core's legacy state");
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(err.contains("core has not moved its old root yet"), "{err}");
    assert_eq!(snapshot(&legacy), before, "the refused run changed core's legacy tree");
    assert!(sb.join("aoide").join("song").join("stage").join("mode.json").is_file(), "the refused run moved the rice tree");
    assert!(sb.join("home/.local/state/lyra/rice.conf").is_file(), "the refused run moved the lyra link");
    assert!(!sb.join("strillix").exists(), "the refused run created the strillix root");

    // What core's `migrate_root_once` does to a legacy `state/`: a rename into the root.
    std::fs::rename(legacy.join("state"), sb.join("aoide").join("state")).unwrap();
    let retried = strillix(&sb, true, &["--help"]);
    assert!(retried.status.success(), "{}", String::from_utf8_lossy(&retried.stderr));
    assert!(sb.join("strillix").join("stage").join("mode.json").is_file(), "the retry did not move the rice tree");
    assert!(sb.join("home/.local/state/strillix/rice.conf").is_file(), "the retry did not move the lyra link");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn the_rice_flag_moves_the_rice_tree_without_aoide_and_leaves_core_legacy_state() {
    let sb = sandbox("no-aoide");
    let legacy = legacy_core_tree(&sb);
    old_rice_tree(&sb);
    let before = snapshot(&legacy);

    let out = strillix(&sb, true, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(sb.join("strillix").join("stage").join("mode.json").is_file(), "the rice tree did not move");
    assert!(sb.join("home/.local/state/strillix/rice.conf").is_file(), "the lyra link did not move");
    assert_eq!(snapshot(&legacy), before, "the rice move changed core's legacy tree");
    assert!(!sb.join("aoide").join("state").exists(), "the rice move moved core's state into the root");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn a_dev_checkout_at_the_legacy_path_holds_nothing_back() {
    let sb = sandbox("checkout");
    // A dev checkout at `~/Aoide` (the flake root's default) carries no legacy state.
    std::fs::create_dir_all(sb.join("home").join("Aoide").join("pkgs")).unwrap();
    old_rice_tree(&sb);

    let out = strillix(&sb, true, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(sb.join("strillix").join("stage").join("mode.json").is_file(), "the rice tree did not move");
    let _ = std::fs::remove_dir_all(&sb);
}
