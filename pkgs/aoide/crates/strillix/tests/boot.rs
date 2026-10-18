//! The paint binary through its real entry point. `strx` is a symlink to
//! `strillix` in the same package: one binary, so argv[0] must never change
//! what it prints. Each child runs under a private `HOME` with every aoide
//! path pinned inside it, so the boot-time `migrate_root_once` touches only
//! the temp tree.

#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn private_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("strillix-boot-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn strillix(home: &Path, argv0: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strillix"))
        .arg0(argv0)
        .args(args)
        .env("HOME", home)
        .env_remove("AOIDE_USER")
        .env_remove("AOIDE_STATE_DIR")
        .env("AOIDE_ROOT", home.join("aoide"))
        .env("STRILLIX_ROOT", home.join("aoide"))
        .env("AOIDE_STAGE_DIR", home.join("stage"))
        .env("AOIDE_AUDIT_LOG", home.join("log"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("COLUMNS", "105")
        .output()
        .expect("the strillix binary runs")
}

#[test]
fn argv0_strx_prints_exactly_what_strillix_prints() {
    let home = private_home("argv0");
    let as_strx = strillix(&home, "strx", &["rice", "--help"]);
    let as_strillix = strillix(&home, "strillix", &["rice", "--help"]);
    assert_eq!(as_strx.status.code(), as_strillix.status.code());
    assert_eq!(as_strx.stdout, as_strillix.stdout, "argv[0] leaked into the help text");
    let text = String::from_utf8_lossy(&as_strx.stdout);
    assert!(text.contains("strillix"), "help does not name strillix: {text}");
    assert!(!text.contains("strx"), "help names the shorthand, not the binary: {text}");
}

#[test]
fn a_boot_moves_the_strillix_state_link_once() {
    let home = private_home("boot");
    let target = home.join("declared-rice.conf");
    std::fs::write(&target, "source\n").unwrap();
    let old = home.join(".local/state/lyra");
    std::fs::create_dir_all(&old).unwrap();
    symlink(&target, old.join("rice.conf")).unwrap();

    strillix(&home, "strillix", &["rice", "--help"]);
    let new = home.join(".local/state/strillix/rice.conf");
    assert!(!old.exists(), "the old state tree survived the boot");
    assert_eq!(std::fs::read_link(&new).unwrap(), target);

    strillix(&home, "strillix", &["rice", "--help"]);
    assert_eq!(std::fs::read_link(&new).unwrap(), target, "the second boot moved it again");
}
