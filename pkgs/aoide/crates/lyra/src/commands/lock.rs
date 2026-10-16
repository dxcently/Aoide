//! `lyra lock` — the one way to lock the screen, from a key bind, the power
//! menu or a terminal.
//!
//! - A staged song that dresses the `lockscreen` slot (its own entry in
//!   `run/qml/songs/manifest.json`, no baseline fallback) is locked by the
//!   running shell: `quickshell -p <run/qml/shell.qml> ipc call lock lock`
//!   reaches `LockHost.qml`, which holds the Wayland session lock and paints
//!   the song's face. The call counts only once the shell answers
//!   `ipc call lock secure` with `true` — the compositor has confirmed the
//!   lock — inside [`SHELL_ANSWER`]. A shell that is not running, does not
//!   answer, or never confirms falls through to hyprlock, so the command
//!   never reports a lock it did not get.
//! - Every other song, and that fall-through, runs `hyprlock`, dressed by
//!   `song/declared/hyprlock.conf` and `song/stage/hyprlock.conf` through the
//!   host's hyprlock config. hyprlock is spawned in its own process group and
//!   left running; a hyprlock that exits inside [`HYPRLOCK_SETTLE`] (another
//!   lock holds the session, no config) is a failure, never a lock.
//!
//! `lyra lock hint yes|no` sets logind's `LockedHint` on the session a lock
//! covers ([`aoide_protocol::dialog::lock_session_id`]). LockHost calls it
//! when it locks and unlocks, so every locked-state reader — the secrets and
//! pairing popup watchers — sees a shell lock the way they see hyprlock's
//! process.
//!
//! Spawned by the compositor (`exec`), never by a NoNewPrivileges unit:
//! hyprlock's PAM check needs the setuid `unix_chkpwd`, which a
//! NoNewPrivileges ancestor disables (shellbridge's `PowerAction::Lock`).

use crate::dispatch::Invocation;
use crate::output::{Fix, Kind, Outcome};
use crate::registry::{arg, cmd, Registry};
use aoide_protocol::Door;
use serde_json::{json, Value};
use std::io::{Read, Seek};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The slot a song dresses to have the shell lock the screen.
pub const FACE_SLOT: &str = "lockscreen";

/// How long the shell has to take the lock and the compositor to confirm it.
const SHELL_ANSWER: Duration = Duration::from_secs(5);

/// How long a spawned hyprlock must stay up to count as holding the lock.
const HYPRLOCK_SETTLE: Duration = Duration::from_millis(500);

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["lock"],
        summary: "Lock the screen. A staged song that dresses the `lockscreen` slot is locked by the running shell (its own face, confirmed by the compositor within 5 s); every other song, and a shell that does not answer, runs hyprlock dressed by song/{declared,stage}/hyprlock.conf.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_lock,
        examples: ["lock", "lock --json"],
        brief: "Lock the screen in the staged song's face.",
        doors: &[Door::Cli],
    ));
    r.insert(cmd!(
        path: ["lock", "hint"],
        summary: "Set logind's LockedHint on the session a lock covers ($XDG_SESSION_ID, else the user's display session). The shell's lock host calls it on lock and unlock, so locked-state readers see a shell lock.",
        args: [arg!("state", "string", true, "`yes` (locked) or `no` (unlocked).", values: &["yes", "no"])],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_hint,
        examples: ["lock hint yes", "lock hint no --json"],
        brief: "Set logind's LockedHint for this session.",
        doors: &[Door::Cli],
    ));
}

fn handle_lock(_inv: &Invocation) -> Outcome {
    let cmd = "lock";
    let song = active_song();
    let face = song.as_deref().filter(|s| dresses_face(&manifest_path(), s));
    let mut shell_refused = None;
    if let Some(song) = face {
        match shell_lock(SHELL_ANSWER) {
            Ok(()) => {
                return Outcome::ok(cmd, format!("locked by the shell in `{song}`'s face"))
                    .with_data(json!({ "locker": "shell", "song": song }));
            }
            Err(why) => shell_refused = Some(why),
        }
    }
    match spawn_hyprlock() {
        Ok(pid) => {
            let message = match &shell_refused {
                Some(_) => "the shell did not take the lock; locked by hyprlock".to_string(),
                None => "locked by hyprlock".to_string(),
            };
            Outcome::ok(cmd, message).with_data(json!({
                "locker": "hyprlock",
                "pid": pid,
                "song": song,
                "shellRefused": shell_refused,
            }))
        }
        Err(detail) => Outcome::refuse(
            cmd,
            Kind::Failed,
            "the screen is not locked",
            "hyprlock did not stay up",
            Fix::Run("hyprlock".to_string()),
        )
        .with_fields(json!({ "reason": "hyprlock-failed", "shellRefused": shell_refused }))
        .with_detail(detail),
    }
}

fn handle_hint(inv: &Invocation) -> Outcome {
    let cmd = "lock.hint";
    let locked = inv.args.first().is_some_and(|s| s == "yes");
    let Some(session) = aoide_protocol::dialog::lock_session_id() else {
        return Outcome::refuse(
            cmd,
            Kind::Refused,
            "no session to mark",
            "neither $XDG_SESSION_ID nor logind's display session for this user names a session",
            Fix::None("LockedHint lives on a logind session; run inside a graphical login"),
        )
        .with_fields(json!({ "reason": "no-session" }));
    };
    let path = format!("/org/freedesktop/login1/session/{}", bus_label(&session));
    let out = Command::new("busctl")
        .args(["call", "org.freedesktop.login1", &path, "org.freedesktop.login1.Session", "SetLockedHint", "b"])
        .arg(if locked { "true" } else { "false" })
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => Outcome::ok(cmd, format!("LockedHint={} on session {session}", if locked { "yes" } else { "no" }))
            .with_data(json!({ "session": session, "lockedHint": locked })),
        Ok(o) => hint_failed(cmd, String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => hint_failed(cmd, format!("spawn busctl: {e}")),
    }
}

fn hint_failed(cmd: &str, detail: String) -> Outcome {
    Outcome::refuse(cmd, Kind::Failed, "LockedHint was not set", "logind refused the call; its words are in `detail`", Fix::None("logind lets only the session's own user set its LockedHint"))
        .with_fields(json!({ "reason": "logind-refused" }))
        .with_detail(detail)
}

/// The staged song: `stage/livery.json`'s `song`, the name the shell itself
/// paints (`LiveryState.songName`).
fn active_song() -> Option<String> {
    let raw = std::fs::read_to_string(aoide_storage::fs::stage_dir().join("livery.json")).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    doc.get("song")?.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn manifest_path() -> std::path::PathBuf {
    aoide_storage::fs::run_qml_dir().join("songs").join("manifest.json")
}

/// Does `song`'s OWN manifest entry carry the face slot? The baseline song is
/// never consulted: a song that ships no face locks with hyprlock.
fn dresses_face(manifest: &Path, song: &str) -> bool {
    std::fs::read_to_string(manifest)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .is_some_and(|doc| doc.get(song).and_then(|s| s.get(FACE_SLOT)).is_some_and(Value::is_object))
}

/// Ask the running shell to lock, then wait for the compositor's word.
fn shell_lock(budget: Duration) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    let said = qs_call(&["lock"], deadline)?;
    if !said.is_empty() {
        return Err(format!("the shell has no lock host: {said}"));
    }
    let unconfirmed = || format!("the compositor did not confirm the shell's lock within {} s", budget.as_secs());
    loop {
        if qs_call(&["secure"], deadline).map_err(|_| unconfirmed())? == "true" {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(unconfirmed());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `quickshell -p <run/qml/shell.qml> ipc call lock <function>`, killed at
/// `deadline`. Quickshell exits 0 for a call that reached no handler, and
/// prints why, so the caller judges the words, not the status.
fn qs_call(function: &[&str], deadline: Instant) -> Result<String, String> {
    let mut child = Command::new("quickshell")
        .arg("-p")
        .arg(aoide_storage::fs::run_qml_dir().join("shell.qml"))
        .args(["ipc", "call", "lock"])
        .args(function)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn quickshell: {e}"))?;
    let status = wait_until(&mut child, deadline).ok_or_else(|| "the shell did not answer in time".to_string())?;
    let mut out = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut out);
    }
    if out.trim().is_empty() {
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut out);
        }
    }
    let out = out.trim().to_string();
    if status.success() {
        Ok(out)
    } else if out.is_empty() {
        Err(format!("quickshell ipc exited {status}"))
    } else {
        Err(out)
    }
}

/// The child's exit status, or `None` once `deadline` passes — the child is
/// killed and reaped then, so no caller leaves a stray behind.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Start hyprlock in its own process group and leave it holding the lock.
///
/// Its stderr goes to an unlinked scratch file, never a pipe: hyprlock
/// outlives this process, and a pipe whose reader has exited turns its next
/// log line into SIGPIPE — the lock would die under the user.
fn spawn_hyprlock() -> Result<u32, String> {
    let mut said = scratch_file().map_err(|e| format!("hyprlock stderr: {e}"))?;
    let stderr = said.try_clone().map_err(|e| format!("hyprlock stderr: {e}"))?;
    let mut child = Command::new("hyprlock")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("spawn hyprlock: {e}"))?;
    let settle = Instant::now() + HYPRLOCK_SETTLE;
    while Instant::now() < settle {
        if let Ok(Some(status)) = child.try_wait() {
            let mut text = String::new();
            let _ = said.rewind();
            let _ = said.read_to_string(&mut text);
            return Err(format!("hyprlock exited at once ({status}): {}", text.trim()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(child.id())
}

/// A private read-write file with no name left on disk.
fn scratch_file() -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("lyra-lock-{}-{nanos}", std::process::id()));
    let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&path)?;
    std::fs::remove_file(&path)?;
    Ok(file)
}

/// A logind session id as a D-Bus object-path label (`sd_bus_path_encode`):
/// every byte that is not an ASCII alphanumeric, and a leading digit, becomes
/// `_` and two lowercase hex digits.
fn bus_label(id: &str) -> String {
    id.bytes()
        .enumerate()
        .map(|(i, b)| {
            if b.is_ascii_alphabetic() || (b.is_ascii_digit() && i > 0) {
                (b as char).to_string()
            } else {
                format!("_{b:02x}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use aoide_test_support::{env_lock, inv, unique_tmp, EnvSaver};
    use std::path::PathBuf;

    /// A scratch runtime root and a PATH holding only stand-ins for
    /// quickshell, hyprlock, busctl and loginctl, so no test can reach the
    /// live shell, a real locker or the real logind. Each stand-in logs its
    /// argv to `<name>.log`; `quickshell`'s answers come from files the test
    /// writes (`qs-lock`, `qs-secure`, `qs-hang`). Builtins only.
    struct Rig {
        root: PathBuf,
        _env: EnvSaver,
    }

    const QS: &str = r#"#!/bin/sh
here="${0%/bin/quickshell}"
echo "$*" >> "$here/quickshell.log"
if [ -e "$here/qs-hang" ]; then while :; do :; done; fi
if [ -e "$here/qs-dead" ]; then echo "No running instances for $2"; exit 255; fi
fn=""; for a in "$@"; do fn="$a"; done
case "$fn" in
lock) [ -e "$here/qs-nohost" ] && echo "Not ready to accept queries yet."; exit 0 ;;
secure) if [ -e "$here/qs-secure" ]; then echo true; else echo false; fi ;;
esac
"#;

    const HYPRLOCK: &str = r#"#!/bin/sh
here="${0%/bin/hyprlock}"
echo "run" >> "$here/hyprlock.log"
if [ -e "$here/hl-fail" ]; then echo "session lock already held" >&2; exit 1; fi
if [ -e "$here/hl-late" ]; then while [ ! -e "$here/hl-speak" ]; do :; done; echo "still locked" >&2 && echo "wrote" >> "$here/hyprlock.log"; fi
while :; do read -r _ 2>/dev/null || :; [ -e "$here/hl-stop" ] && exit 0; done
"#;

    const BUSCTL: &str = r#"#!/bin/sh
here="${0%/bin/busctl}"
echo "$*" >> "$here/busctl.log"
"#;

    const LOGINCTL: &str = r#"#!/bin/sh
here="${0%/bin/loginctl}"
echo "$*" >> "$here/loginctl.log"
case "$1" in show-user) echo 3 ;; esac
"#;

    impl Rig {
        fn new(tag: &str) -> Rig {
            let env = EnvSaver::capture(&["PATH", "AOIDE_STAGE_DIR", "XDG_SESSION_ID"]);
            let root = unique_tmp(tag);
            let bin = root.join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            for (name, body) in [("quickshell", QS), ("hyprlock", HYPRLOCK), ("busctl", BUSCTL), ("loginctl", LOGINCTL)] {
                let p = bin.join(name);
                std::fs::write(&p, body).unwrap();
                std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            }
            let stage = root.join("aoide/song/stage");
            std::fs::create_dir_all(&stage).unwrap();
            std::fs::create_dir_all(root.join("aoide/run/qml/songs")).unwrap();
            std::env::set_var("PATH", &bin);
            std::env::set_var("AOIDE_STAGE_DIR", &stage);
            std::env::remove_var("XDG_SESSION_ID");
            Rig { root, _env: env }
        }

        fn stage(&self, song: &str, manifest: &str) {
            std::fs::write(self.root.join("aoide/song/stage/livery.json"), format!(r#"{{"song":"{song}"}}"#)).unwrap();
            std::fs::write(self.root.join("aoide/run/qml/songs/manifest.json"), manifest).unwrap();
        }

        fn touch(&self, name: &str) {
            std::fs::write(self.root.join(name), b"").unwrap();
        }

        fn log(&self, name: &str) -> String {
            std::fs::read_to_string(self.root.join(format!("{name}.log"))).unwrap_or_default()
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            // Every hyprlock stand-in still spinning ends here.
            self.touch("hl-stop");
            std::thread::sleep(Duration::from_millis(50));
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const FACE: &str = r#"{"barcarolle":{"lockscreen":{"owner":"barcarolle","file":"lockscreen.qml"}},"sonata":{"bar":{"owner":"sonata","file":"bar.qml"}}}"#;

    fn data(o: &Outcome) -> Value {
        o.data.clone().unwrap_or_default()
    }

    #[test]
    fn a_song_with_a_face_is_locked_by_the_shell_once_the_compositor_confirms() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-shell");
        rig.stage("barcarolle", FACE);
        rig.touch("qs-secure");
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(data(&o)["locker"], "shell");
        let calls = rig.log("quickshell");
        assert!(calls.lines().next().unwrap().ends_with("ipc call lock lock"), "{calls}");
        assert!(calls.contains("ipc call lock secure"), "{calls}");
        assert!(calls.contains("run/qml/shell.qml"), "{calls}");
        assert_eq!(rig.log("hyprlock"), "", "hyprlock never ran");
    }

    #[test]
    fn a_song_without_a_face_runs_hyprlock_and_never_asks_the_shell() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-noface");
        // sonata dresses no lockscreen; the baseline is never borrowed.
        rig.stage("sonata", FACE);
        rig.touch("qs-secure");
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(data(&o)["locker"], "hyprlock");
        assert!(data(&o)["shellRefused"].is_null());
        assert_eq!(rig.log("quickshell"), "");
        assert_eq!(rig.log("hyprlock"), "run\n");
    }

    #[test]
    fn nothing_staged_runs_hyprlock() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-unstaged");
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(data(&o)["locker"], "hyprlock");
        assert_eq!(rig.log("quickshell"), "");
    }

    #[test]
    fn a_shell_that_is_not_running_or_has_no_lock_host_falls_through_to_hyprlock() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        for (tag, marker, said) in [("lock-dead", "qs-dead", "No running instances"), ("lock-nohost", "qs-nohost", "no lock host")] {
            let rig = Rig::new(tag);
            rig.stage("barcarolle", FACE);
            rig.touch(marker);
            let o = handle_lock(&inv(&["lock"], &[]));
            assert_eq!(o.status, Status::Ok, "{o:?}");
            assert_eq!(data(&o)["locker"], "hyprlock");
            assert!(data(&o)["shellRefused"].as_str().unwrap().contains(said), "{o:?}");
            assert_eq!(rig.log("hyprlock"), "run\n");
        }
    }

    #[test]
    fn a_lock_the_compositor_never_confirms_is_not_reported_and_hyprlock_takes_over() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-unconfirmed");
        rig.stage("barcarolle", FACE);
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(data(&o)["locker"], "hyprlock");
        assert!(data(&o)["shellRefused"].as_str().unwrap().contains("did not confirm"), "{o:?}");
    }

    #[test]
    fn a_shell_that_hangs_is_cut_off_inside_the_budget() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-hang");
        rig.stage("barcarolle", FACE);
        rig.touch("qs-hang");
        let started = Instant::now();
        let o = handle_lock(&inv(&["lock"], &[]));
        let took = started.elapsed();
        assert!(took < SHELL_ANSWER + Duration::from_secs(2), "took {took:?}");
        assert_eq!(data(&o)["locker"], "hyprlock", "{o:?}");
        assert!(data(&o)["shellRefused"].as_str().unwrap().contains("did not answer"), "{o:?}");
    }

    #[test]
    fn a_hyprlock_that_dies_at_once_is_a_failure_not_a_lock() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-hlfail");
        rig.touch("hl-fail");
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Error, "{o:?}");
        assert_eq!(o.status.exit_code(), 1);
        assert_eq!(data(&o)["reason"], "hyprlock-failed");
        assert!(data(&o)["detail"].as_str().unwrap().contains("session lock already held"), "{o:?}");
    }

    #[test]
    fn hyprlock_keeps_logging_after_lyra_returns() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-hllate");
        rig.touch("hl-late");
        let o = handle_lock(&inv(&["lock"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        rig.touch("hl-speak");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(rig.log("hyprlock"), "run\nwrote\n", "a late stderr line must not kill the locker");
    }

    #[test]
    fn hint_marks_the_display_session_when_no_session_id_is_set() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("lock-hint");
        let o = handle_hint(&inv(&["lock", "hint"], &["yes"]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(data(&o)["session"], "3");
        assert!(rig.log("loginctl").starts_with("show-user "), "{}", rig.log("loginctl"));
        assert_eq!(
            rig.log("busctl"),
            "call org.freedesktop.login1 /org/freedesktop/login1/session/_33 org.freedesktop.login1.Session SetLockedHint b true\n"
        );
        std::env::set_var("XDG_SESSION_ID", "c2");
        let o = handle_hint(&inv(&["lock", "hint"], &["no"]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert!(rig.log("busctl").ends_with("/org/freedesktop/login1/session/c2 org.freedesktop.login1.Session SetLockedHint b false\n"));
    }

    #[test]
    fn bus_label_escapes_a_leading_digit_and_every_non_alphanumeric() {
        assert_eq!(bus_label("3"), "_33");
        assert_eq!(bus_label("12"), "_312");
        assert_eq!(bus_label("c2"), "c2");
        assert_eq!(bus_label("a-b"), "a_2db");
    }
}
