#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aoide_desktop_shell::exec::{check_pin, test_doors, Exec, ExecError};

const FAKE_AOIDE: &str = env!("CARGO_BIN_EXE_fake-aoide");

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aoide-desk-detach-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("home")).expect("a scratch dir is made");
    fs::create_dir_all(dir.join("runtime")).expect("the runtime dir is made");
    fs::set_permissions(dir.join("runtime"), fs::Permissions::from_mode(0o700)).expect("the runtime dir is 0700");
    dir
}

fn load(dir: &Path, base: &[(&str, &str)]) -> Exec {
    let mut env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), dir.join("record.jsonl").into_os_string()),
    ];
    env.extend(base.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    Exec::load(Path::new(FAKE_AOIDE), &dir.join("home"), env).expect("the fake aoide loads")
}

fn log_file(dir: &Path) -> fs::File {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("door.log"))
        .expect("the log opens")
}

#[test]
fn an_exec_failure_is_reported_as_an_error() {
    let dir = scratch("missing");
    let exec = load(&dir, &[]);
    let program = dir.join("door");
    fs::write(&program, "#!/bin/sh\n").expect("the door is written");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("the door is executable");
    let pinned = check_pin(&program, &dir.join("home")).expect("the door pins");
    fs::remove_file(&program).expect("the door is removed after it pins");
    let args = [OsString::from("web")];
    let failed = test_doors::spawn(&exec, &pinned, &args, &dir.join("runtime"), log_file(&dir));
    assert!(
        matches!(&failed, Err(ExecError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound),
        "the unlinked path is a spawn error of kind NotFound: {failed:?}"
    );
}

#[test]
fn the_door_env_points_the_door_at_its_runtime_and_drops_the_session() {
    let dir = scratch("env");
    let exec = load(&dir, &[("XDG_RUNTIME_DIR", "/old"), ("AOIDE_SESSION_ID", "s1")]);
    let env = exec.door_env(&dir.join("runtime")).expect("the env is made");
    let get = |name: &str| env.iter().find(|(k, _)| k.as_os_str() == name).map(|(_, v)| v.clone());
    assert_eq!(get("XDG_RUNTIME_DIR"), Some(dir.join("runtime").into_os_string()));
    assert_eq!(get("AOIDE_SESSION_ID"), None, "a door never inherits the session id");
}

#[test]
fn terminate_refuses_pid_zero_and_pids_past_i32() {
    assert!(test_doors::terminate(0).is_err(), "pid 0 would signal this process group");
    assert!(test_doors::terminate(u32::MAX).is_err(), "a pid past i32::MAX is refused");
}
