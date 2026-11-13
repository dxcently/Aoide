//! `strx migrate` through the REAL `strillix` binary: Strillix's trees move out
//! of `$AOIDE_ROOT` into the strillix root, and no other verb moves anything.
//! Every child runs under a private HOME with every root pinned inside it.

#![cfg(unix)]

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// One file in each tree that moves, as a box before the move holds them.
const FILES: [&str; 5] = [
    "run/qml/shell.qml",
    "run/elements/waybar/config",
    "state/skwd-wall/library/sea.png",
    "state/captures/shot.png",
    "state/pointer-pos.json",
];

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("strillix-migrate-gate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// Core's root inside the sandbox.
fn core(sb: &Path) -> PathBuf {
    sb.join("aoide")
}

fn put(path: &Path, bytes: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Strillix's trees under core's root, and the `song` marker an earlier move left.
fn old_trees(sb: &Path) {
    for file in FILES {
        put(&core(sb).join(file), file);
    }
    put(&core(sb).join("song"), "");
}

fn strillix(sb: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strillix"))
        .env("HOME", sb.join("home"))
        .env("AOIDE_ROOT", core(sb))
        .env("STRILLIX_ROOT", sb.join("strillix"))
        .env("AOIDE_STATE_DIR", sb.join("state"))
        .env("AOIDE_AUDIT_LOG", sb.join("log"))
        .env("XDG_RUNTIME_DIR", sb.join("run"))
        .env("XDG_STATE_HOME", sb.join("xdg-state"))
        .env("XDG_CONFIG_HOME", sb.join("xdg-config"))
        .env("XDG_DATA_HOME", sb.join("xdg-data"))
        .env("COLUMNS", "105")
        .args(args)
        .output()
        .expect("the strillix binary runs")
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout))
    })
}

#[test]
fn migrate_check_lists_the_old_paths_and_moves_nothing() {
    let sb = sandbox("check");
    old_trees(&sb);
    let out = strillix(&sb, &["migrate", "--check", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let data = &json(&out)["data"];
    assert_eq!(data["check"], true);
    assert_eq!(data["due"].as_array().map(Vec::len), Some(6), "five trees and the marker: {data}");
    assert!(core(&sb).join("state/captures/shot.png").is_file(), "a check moves nothing");
    assert!(!sb.join("strillix").join("state").exists());
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn migrate_moves_every_tree_and_a_second_run_finds_nothing() {
    let sb = sandbox("verb");
    old_trees(&sb);
    let out = strillix(&sb, &["migrate", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for file in FILES {
        assert_eq!(std::fs::read_to_string(sb.join("strillix").join(file)).unwrap(), file, "{file} moved");
        assert!(!core(&sb).join(file).exists(), "{file} left nothing behind");
    }
    assert!(!core(&sb).join("run").exists(), "the emptied run/ goes too");
    assert!(!core(&sb).join("song").exists(), "the marker goes");
    let again = strillix(&sb, &["migrate", "--check", "--json"]);
    assert_eq!(json(&again)["data"]["due"], json!([]), "nothing is left to move");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn a_clash_stays_where_it_is_and_the_verb_fails() {
    let sb = sandbox("clash");
    old_trees(&sb);
    let theirs = sb.join("strillix").join("state/pointer-pos.json");
    put(&theirs, "theirs");
    let out = strillix(&sb, &["migrate"]);
    assert!(!out.status.success(), "a path that stays fails the verb");
    let stayed = core(&sb).join("state/pointer-pos.json");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&format!("strillix: {} stays where it is", stayed.display())), "{err}");
    assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "theirs", "the strillix root's copy is kept");
    assert!(stayed.is_file(), "the old copy stays");
    assert!(sb.join("strillix").join("state/captures/shot.png").is_file(), "the rest still moves");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn no_other_verb_moves_anything() {
    let sb = sandbox("plain");
    old_trees(&sb);
    let out = strillix(&sb, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for file in FILES {
        assert!(core(&sb).join(file).is_file(), "{file} stays until migrate runs");
    }
    assert!(core(&sb).join("song").is_file());
    assert!(!sb.join("strillix").join("state").exists());
    let _ = std::fs::remove_dir_all(&sb);
}
