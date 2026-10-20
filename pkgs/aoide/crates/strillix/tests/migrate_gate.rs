//! The rice-tree migration through the REAL `strillix` binary. It moves only
//! under `STRILLIX_MIGRATE=1` (the home activation sets it) or through
//! `strx migrate`; a plain run narrates the old state and moves nothing.
//! Every child runs under a private HOME with every root pinned inside it.

#![cfg(unix)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("strillix-migrate-gate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// The old rice tree as an un-migrated box has it under `$AOIDE_ROOT/song`.
fn old_rice_tree(sb: &Path) {
    let song = sb.join("aoide").join("song");
    std::fs::create_dir_all(song.join("stage")).unwrap();
    std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
    std::fs::create_dir_all(song.join("songbook").join("sonata")).unwrap();
    std::fs::write(song.join("songbook").join("sonata").join("rice.nix"), "{}").unwrap();
}

fn strillix(sb: &Path, migrate: bool, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_strillix"));
    c.env("HOME", sb.join("home"))
        .env("AOIDE_ROOT", sb.join("aoide"))
        .env("STRILLIX_ROOT", sb.join("strillix"))
        .env("AOIDE_STATE_DIR", sb.join("state"))
        .env("AOIDE_AUDIT_LOG", sb.join("log"))
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
    c.args(args).output().expect("the strillix binary runs")
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout))
    })
}

#[test]
fn a_plain_run_moves_nothing_and_names_the_verb() {
    let sb = sandbox("plain");
    old_rice_tree(&sb);
    let out = strillix(&sb, false, &["--help"]);
    assert!(out.status.success(), "help still runs: {}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().filter(|l| l.contains("strx migrate")).count(), 1, "one line names the verb: {err}");
    assert!(sb.join("aoide").join("song").join("stage").join("mode.json").is_file(), "the old tree stays put");
    assert!(!sb.join("strillix").join("stage").exists(), "nothing moved into the strillix root");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn the_flag_runs_the_move_from_the_start_up_hook() {
    let sb = sandbox("flag");
    old_rice_tree(&sb);
    let out = strillix(&sb, true, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let moved = sb.join("strillix").join("stage").join("mode.json");
    let mode: Value = serde_json::from_str(&std::fs::read_to_string(&moved).unwrap()).unwrap();
    assert_eq!(mode["rice"], "sonata", "mode.json's song key becomes rice");
    assert!(sb.join("strillix").join("rices").join("sonata").join("rice.nix").is_file());
    assert!(sb.join("aoide").join("song").is_file(), "the finished move leaves the marker");
    let _ = std::fs::remove_dir_all(&sb);
}

/// The paint link Hyprland sources until the activation rewrites its config.
fn old_lyra_link(sb: &Path) -> PathBuf {
    let lyra = sb.join("home").join(".local").join("state").join("lyra");
    std::fs::create_dir_all(&lyra).unwrap();
    std::fs::write(lyra.join("rice.conf"), "source\n").unwrap();
    lyra.join("rice.conf")
}

#[test]
fn the_lyra_link_moves_only_under_the_flag() {
    let sb = sandbox("lyra");
    let link = old_lyra_link(&sb);
    let plain = strillix(&sb, false, &["--help"]);
    assert!(plain.status.success(), "{}", String::from_utf8_lossy(&plain.stderr));
    assert!(link.is_file(), "a plain start leaves the lyra link where Hyprland sources it");
    assert!(!sb.join("home").join(".local/state/strillix").exists());
    let flagged = strillix(&sb, true, &["--help"]);
    assert!(flagged.status.success(), "{}", String::from_utf8_lossy(&flagged.stderr));
    assert!(sb.join("home/.local/state/strillix/rice.conf").is_file(), "the flag moves the lyra link");
    assert!(!link.exists());
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn migrate_check_reports_due_and_moves_nothing() {
    let sb = sandbox("check");
    old_rice_tree(&sb);
    let out = strillix(&sb, false, &["migrate", "--check", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let body = json(&out);
    let data = &body["data"];
    assert_eq!(data["check"], true);
    assert_eq!(data["before"]["state"], "due");
    assert_eq!(data["after"]["state"], "due", "a check leaves the state as it found it");
    assert!(sb.join("aoide").join("song").join("stage").join("mode.json").is_file());
    assert!(!sb.join("strillix").join("stage").exists());
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn migrate_moves_once_then_reports_settled() {
    let sb = sandbox("verb");
    old_rice_tree(&sb);
    let first = json(&strillix(&sb, false, &["migrate", "--json"]));
    assert_eq!(first["data"]["before"]["state"], "due");
    assert_eq!(first["data"]["after"]["state"], "settled");
    assert!(sb.join("strillix").join("rices").join("sonata").join("rice.nix").is_file());
    let second = json(&strillix(&sb, false, &["migrate", "--json"]));
    assert_eq!(second["data"]["before"]["state"], "settled", "a second run finds nothing to move");
    let _ = std::fs::remove_dir_all(&sb);
}

#[test]
fn a_fresh_box_grows_no_song_tree_under_the_flag() {
    let sb = sandbox("fresh");
    let out = strillix(&sb, true, &["--help"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!sb.join("aoide").join("song").exists(), "a fresh box gets no marker and no song tree");
    let _ = std::fs::remove_dir_all(&sb);
}
