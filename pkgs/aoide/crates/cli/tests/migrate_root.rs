//! `aoide`'s start-up migration, through the REAL binary: a `state/` that
//! cannot be locked owner-only stops EVERY run before any command reads it,
//! the run that moves it and each run after. Every child runs under an
//! isolated `HOME`, with the stage dir, audit log and runtime dir pinned inside
//! it, so only the `state/` piece of `migrate_root_once` has anything to do.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aoide-migrate-root-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn aoide(home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aoide"))
        .env("HOME", home)
        .env_remove("AOIDE_USER")
        .env_remove("AOIDE_ROOT")
        .env_remove("AOIDE_STATE_DIR")
        .env("AOIDE_STAGE_DIR", home.join("stage"))
        .env("AOIDE_AUDIT_LOG", home.join("log"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("COLUMNS", "105")
        .output()
        .expect("the aoide binary runs")
}

/// Bare `aoide` would print its overview and exit 0; a refusal exits 1 naming
/// the path and why, prints nothing on stdout, and leaves the link's target
/// alone.
fn assert_refused(out: &Output, state: &Path, target: &Path, run: u32) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "run {run}: aoide ran on a state/ it could not lock: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.stdout.is_empty(), "run {run}: nothing may run past the refusal: {stdout}");
    assert!(stderr.contains(&*state.to_string_lossy()), "run {run}: the refusal must name the path: {stderr}");
    assert!(stderr.contains("symlink"), "run {run}: the refusal must say why: {stderr}");
    let mode = std::fs::metadata(target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "run {run}: the link's target was chmodded");
}

/// The legacy `~/Aoide/state` is a symlink, so the moved `state/` is one too
/// and the lock refuses it. The first run moves it and exits 1. The second
/// finds it already in place with nothing left to move, and exits 1 just the
/// same.
#[test]
fn aoide_exits_on_every_run_while_its_state_cannot_be_locked() {
    let home = sandbox("unlockable");
    let target = home.join("elsewhere");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(home.join("Aoide")).unwrap();
    std::os::unix::fs::symlink(&target, home.join("Aoide").join("state")).unwrap();
    let state = home.join(".aoide").join("state");

    assert_refused(&aoide(&home), &state, &target, 1);
    assert!(std::fs::symlink_metadata(&state).is_ok(), "run 1 moved the link, which is what run 2 finds");
    assert!(std::fs::symlink_metadata(home.join("Aoide").join("state")).is_err(), "nothing is left to move");

    assert_refused(&aoide(&home), &state, &target, 2);
    let _ = std::fs::remove_dir_all(&home);
}
