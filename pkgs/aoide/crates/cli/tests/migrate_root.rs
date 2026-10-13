//! `aoide`'s start-up migration, through the REAL binary: a pre-L-C2 `state/`
//! that moves but cannot then be locked owner-only stops the run before any
//! command reads it. Every child runs under an isolated `HOME`, with the stage
//! dir, audit log and runtime dir pinned inside it, so only the `state/` piece
//! of `migrate_root_once` has anything to do.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aoide-migrate-root-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The legacy `~/Aoide/state` is a symlink, so the moved `state/` is one too
/// and the lock refuses it. Bare `aoide` would print its overview and exit 0;
/// here it exits 1 naming the path, prints nothing on stdout, and the link's
/// target keeps its mode.
#[test]
fn aoide_exits_when_the_state_it_migrated_cannot_be_locked() {
    let home = sandbox("unlockable");
    let target = home.join("elsewhere");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(home.join("Aoide")).unwrap();
    std::os::unix::fs::symlink(&target, home.join("Aoide").join("state")).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_aoide"))
        .env("HOME", &home)
        .env_remove("AOIDE_USER")
        .env_remove("AOIDE_ROOT")
        .env_remove("AOIDE_STATE_DIR")
        .env("AOIDE_STAGE_DIR", home.join("stage"))
        .env("AOIDE_AUDIT_LOG", home.join("log"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("COLUMNS", "105")
        .output()
        .expect("the aoide binary runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let state = home.join(".aoide").join("state");
    assert_eq!(out.status.code(), Some(1), "aoide ran on a state/ it could not lock: {stderr}");
    assert!(out.stdout.is_empty(), "nothing may run past the refusal: {}", String::from_utf8_lossy(&out.stdout));
    assert!(stderr.contains(&*state.to_string_lossy()), "the refusal must name the path: {stderr}");
    assert!(stderr.contains("symlink"), "the refusal must say why: {stderr}");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "the link's target was chmodded");
    let _ = std::fs::remove_dir_all(&home);
}
