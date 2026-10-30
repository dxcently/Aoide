use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aoide_desktop_shell::exec::{check_pin, PinError};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aoide-desk-pins-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    dir
}

fn program(path: &Path, mode: u32) {
    fs::create_dir_all(path.parent().expect("a file has a parent")).expect("its parent is made");
    fs::write(path, "#!/bin/sh\n").expect("the file is written");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("the mode is set");
}

#[test]
fn a_relative_path_is_refused() {
    let home = scratch("relative");
    let err = check_pin(Path::new("bin/eidolon"), &home).expect_err("a relative path is refused");
    assert!(matches!(err, PinError::Relative(_)), "{err:?}");
}

#[test]
fn a_missing_path_is_refused() {
    let home = scratch("missing");
    let err = check_pin(&home.join("eidolon"), &home).expect_err("a missing path is refused");
    assert!(matches!(err, PinError::Missing { .. }), "{err:?}");
}

#[test]
fn a_file_without_an_execute_bit_is_refused() {
    let dir = scratch("noexec");
    let file = dir.join("aoide");
    program(&file, 0o644);
    let err = check_pin(&file, &dir.join("home")).expect_err("a non-executable file is refused");
    assert!(matches!(err, PinError::NotExecutable(_)), "{err:?}");
}

#[test]
fn a_directory_is_not_a_program() {
    let dir = scratch("dir");
    let tool = dir.join("tool");
    fs::create_dir_all(&tool).expect("the directory is made");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("the mode is set");
    let err = check_pin(&tool, &dir.join("home")).expect_err("a directory is refused");
    assert!(matches!(err, PinError::NotExecutable(_)), "{err:?}");
}

#[test]
fn a_file_under_local_bin_is_refused() {
    let home = scratch("localbin");
    let file = home.join(".local/bin/eidolon");
    program(&file, 0o755);
    let err = check_pin(&file, &home).expect_err("a file in local bin is refused");
    assert!(matches!(err, PinError::InLocalBin(_)), "{err:?}");
}

#[test]
fn a_symlink_into_local_bin_is_refused() {
    let home = scratch("link-home");
    let target = home.join(".local/bin/eidolon");
    program(&target, 0o755);
    let elsewhere = scratch("link-elsewhere");
    let link = elsewhere.join("eidolon");
    symlink(&target, &link).expect("the link is made");
    let err = check_pin(&link, &home).expect_err("a link into local bin is refused");
    assert!(matches!(err, PinError::InLocalBin(_)), "{err:?}");
}

#[test]
fn local_bin_is_refused_behind_a_linked_home() {
    let real = scratch("real-home");
    let target = real.join(".local/bin/eidolon");
    program(&target, 0o755);
    let linked = scratch("linked-home").join("home");
    symlink(&real, &linked).expect("the home link is made");
    let err = check_pin(&target, &linked).expect_err("local bin behind a link is refused");
    assert!(matches!(err, PinError::InLocalBin(_)), "{err:?}");
}

#[test]
fn a_good_file_pins_to_its_canonical_path() {
    let home = scratch("good-home");
    let dir = scratch("good");
    let file = dir.join("aoide");
    program(&file, 0o755);
    let link = dir.join("aoide-link");
    symlink(&file, &link).expect("the link is made");
    let pinned = check_pin(&link, &home).expect("a good file pins");
    assert_eq!(pinned.path(), fs::canonicalize(&file).expect("the file canonicalizes").as_path());
}

#[test]
fn a_program_in_a_group_writable_directory_is_refused() {
    let home = scratch("group-dir");
    let bin = home.join("opt");
    program(&bin.join("eidolon"), 0o755);
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o775)).expect("the directory is group-writable");
    let err = check_pin(&bin.join("eidolon"), &home).expect_err("another user can write the directory");
    assert!(matches!(err, PinError::Writable(_)), "{err:?}");
}

#[test]
fn a_program_in_an_other_writable_directory_is_refused() {
    let home = scratch("other-dir");
    let bin = home.join("opt");
    program(&bin.join("eidolon"), 0o755);
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o777)).expect("the directory is other-writable");
    let err = check_pin(&bin.join("eidolon"), &home).expect_err("anyone can write the directory");
    assert!(matches!(err, PinError::Writable(_)), "{err:?}");
}

#[test]
fn a_group_writable_program_is_refused() {
    let home = scratch("group-file");
    program(&home.join("eidolon"), 0o775);
    let err = check_pin(&home.join("eidolon"), &home).expect_err("another user can write the program");
    assert!(matches!(err, PinError::Writable(_)), "{err:?}");
}
