//! The `window` verbs through the real binary, with no Hyprland session: `list` and `watch --json`
//! answer the no-compositor refusal on stderr, and `watch` writes nothing to stdout. A `watch` whose
//! stdout reader has gone ends with its own `stdout-closed` refusal, not by a signal. When stdout and
//! stderr share that pipe, the refusal's print to stderr is what ends the process, by SIGPIPE, after
//! the audit line is written.

#![cfg(unix)]

use aoide_test_support::{accept_one, short_tmp, DELIVERY_BUDGET};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A private root for one test. `short_tmp` keeps the event socket's path inside `sun_path`.
fn private_home(tag: &str) -> PathBuf {
    let dir = short_tmp(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The binary with every root it reads inside `home`, and no Hyprland session named.
fn command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_strillix"));
    command
        .args(args)
        .env("HOME", home)
        .env("AOIDE_ROOT", home.join("aoide"))
        .env("STRILLIX_ROOT", home.join("strillix"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("STRILLIX_MIGRATE");
    command
}

fn strillix(home: &Path, args: &[&str]) -> Output {
    command(home, args).output().expect("the strillix binary runs")
}

/// A watch under test. std's `Child` does not kill on drop, so this does: a panic at an accept, a
/// read or an assertion leaves no watch running, and the child is reaped either way.
struct Watch(Child);

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The fake event socket Hyprland would listen on, bound under `home`'s runtime dir.
fn bind_event_socket(home: &Path) -> UnixListener {
    let event = home.join("run").join("hypr").join("test").join(".socket2.sock");
    std::fs::create_dir_all(event.parent().unwrap()).unwrap();
    UnixListener::bind(&event).unwrap()
}

/// The watch's first line, read on a thread within the budget. The reader comes back with it, so
/// the caller decides when the read end closes.
fn first_line<R: Read + Send + 'static>(reader: R) -> (String, BufReader<R>) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        let read = reader.read_line(&mut line);
        let _ = tx.send((read, line, reader));
    });
    let (read, line, reader) =
        rx.recv_timeout(DELIVERY_BUDGET).expect("the watch wrote no first line within the budget");
    read.expect("the first line reads");
    (line, reader)
}

/// The watch's exit status, polled within the budget. A watch still running at the deadline fails
/// the test, and the guard kills it on the way out.
fn exit_within_budget(child: &mut Watch) -> ExitStatus {
    let deadline = Instant::now() + DELIVERY_BUDGET;
    loop {
        if let Some(status) = child.0.try_wait().expect("the watch can be polled") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the watch was still running {:?} after its reader left",
            DELIVERY_BUDGET
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn watch_json_with_no_signature_is_the_error_envelope_on_stderr() {
    let home = private_home("watch");
    let out = strillix(&home, &["window", "watch", "--json"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let outcome: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(outcome["command"], "window.watch");
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["data"]["reason"], "no-compositor");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn list_json_with_no_signature_is_the_error_envelope() {
    let home = private_home("list");
    let out = strillix(&home, &["window", "list", "--json"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let outcome: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(outcome["command"], "window.list");
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["data"]["reason"], "no-compositor");
    let _ = std::fs::remove_dir_all(&home);
}

/// `watch` with its stdout a real pipe whose reader has gone. The test reads `connected`, closes the
/// read end, and sends one event: the binary must end with its own `stdout-closed` refusal, exit 1,
/// and no signal. Every wait is bounded, and the watch is killed and reaped on every way out of the
/// test, a panic included.
#[test]
fn watch_whose_stdout_reader_has_gone_ends_with_stdout_closed_not_a_signal() {
    let home = private_home("sigpipe");
    let listener = bind_event_socket(&home);
    let mut child = Watch(
        command(&home, &["window", "watch", "--json"])
            .env("HYPRLAND_INSTANCE_SIGNATURE", "test")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the strillix binary runs"),
    );
    let mut peer = accept_one(&listener, "the watch's event connection");

    let (first, stdout) = first_line(child.0.stdout.take().expect("stdout is piped"));
    assert_eq!(first, "{\"type\":\"connected\"}\n");
    drop(stdout);

    peer.write_all(b"openwindow>>55aabb,1,kitty,t\n").expect("the event line is sent");
    let status = exit_within_budget(&mut child);
    let mut stderr = String::new();
    child.0.stderr.take().expect("stderr is piped").read_to_string(&mut stderr).unwrap();

    assert_eq!((status.code(), status.signal()), (Some(1), None), "stderr: {stderr}");
    let outcome: Value = serde_json::from_str(&stderr).expect("stderr is the refusal envelope");
    assert_eq!(outcome["command"], "window.watch");
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["data"]["reason"], "stdout-closed");
    let log = std::fs::read_to_string(home.join("strillix").join("log")).expect("the audit log exists");
    assert!(log.contains("window.watch"), "audit log: {log}");
    let _ = std::fs::remove_dir_all(&home);
}

/// `watch` with stdout and stderr on one pipe, whose reader has gone. The audit line is written when
/// the verb returns; then the door prints the refusal to stderr, the same closed pipe, and the process
/// ends by SIGPIPE. A print that panicked in `eprintln!` would exit 101 instead. The Command is
/// dropped after spawn, so the child holds the only write end.
#[test]
fn watch_on_a_shared_pipe_whose_reader_has_gone_ends_by_sigpipe_after_its_audit_line() {
    let home = private_home("sigpipe-shared");
    let listener = bind_event_socket(&home);
    let (reader, writer) = std::io::pipe().expect("the pipe is made");
    let mut cmd = command(&home, &["window", "watch", "--json"]);
    cmd.env("HYPRLAND_INSTANCE_SIGNATURE", "test")
        .stdout(writer.try_clone().expect("the pipe's writer is cloned"))
        .stderr(writer);
    let mut child = Watch(cmd.spawn().expect("the strillix binary runs"));
    drop(cmd);
    let mut peer = accept_one(&listener, "the watch's event connection");

    let (first, reader) = first_line(reader);
    assert_eq!(first, "{\"type\":\"connected\"}\n");
    drop(reader);

    peer.write_all(b"openwindow>>55aabb,1,kitty,t\n").expect("the event line is sent");
    let status = exit_within_budget(&mut child);

    assert_eq!(status.signal(), Some(libc::SIGPIPE), "exit {:?}", status.code());
    let log = std::fs::read_to_string(home.join("strillix").join("log")).expect("the audit log exists");
    assert!(log.contains("window.watch"), "audit log: {log}");
    let _ = std::fs::remove_dir_all(&home);
}
