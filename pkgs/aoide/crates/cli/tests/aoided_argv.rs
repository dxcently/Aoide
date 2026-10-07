//! `aoided`'s argv, through the REAL binary: a probe (`--version`, `--help`, a
//! typo) answers and exits without ever becoming a daemon. Every child runs
//! under an isolated root, runtime dir and socket path, and the tests assert
//! those stay EMPTY — no socket, pid, log or state file means nothing started.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(20);

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("aoided-argv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("root")).unwrap();
        std::fs::create_dir_all(dir.join("run")).unwrap();
        Sandbox { dir }
    }

    fn aoided(&self, args: &[&str]) -> (String, String, i32) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aoided"))
            .args(args)
            .env("AOIDE_ROOT", self.dir.join("root"))
            .env("XDG_RUNTIME_DIR", self.dir.join("run"))
            .env("AOIDE_DAEMON_SOCKET", self.dir.join("run").join("aoided.sock"))
            .env("AOIDE_STAGE_DIR", self.dir.join("root").join("stage"))
            .env("AOIDE_STATE_DIR", self.dir.join("root").join("state"))
            .env("AOIDE_AUDIT_LOG", self.dir.join("root").join("log"))
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the aoided binary runs");
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > DEADLINE {
                let _ = child.kill();
                panic!("aoided {args:?} did not exit within {DEADLINE:?}: it started a daemon");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )
    }

    fn files(&self) -> Vec<PathBuf> {
        fn walk(p: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                out.push(e.path());
                if e.path().is_dir() {
                    walk(&e.path(), out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.dir.join("root"), &mut out);
        walk(&self.dir.join("run"), &mut out);
        out
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn version_prints_the_version_and_starts_nothing() {
    let sb = Sandbox::new("version");
    let (out, err, code) = sb.aoided(&["--version"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(out.trim(), format!("aoided {}", env!("CARGO_PKG_VERSION")));
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

#[test]
fn help_prints_usage_on_stdout() {
    let sb = Sandbox::new("help");
    let (out, err, code) = sb.aoided(&["--help"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.contains("usage: aoided") && out.contains("--audit-log"), "{out}");
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

#[test]
fn an_unknown_flag_is_refused_and_starts_nothing() {
    let sb = Sandbox::new("unknown");
    let (out, err, code) = sb.aoided(&["--verison"]);
    assert_eq!((code, out.as_str()), (2, ""));
    assert!(err.contains("`--verison` is not an aoided argument"), "{err}");
    assert!(err.contains("aoided --version"), "{err}");
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

#[test]
fn a_bare_word_is_refused_and_starts_nothing() {
    let sb = Sandbox::new("bare");
    let (_, err, code) = sb.aoided(&["start"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("aoided --help"), "{err}");
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

#[test]
fn audit_log_without_a_path_is_usage() {
    let sb = Sandbox::new("nopath");
    let (_, err, code) = sb.aoided(&["--audit-log"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("needs a path"), "{err}");
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

#[test]
fn the_equals_form_is_taught_the_spaced_one() {
    let sb = Sandbox::new("equals");
    let (_, err, code) = sb.aoided(&["--audit-log=/x"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("aoided --audit-log /x"), "{err}");
    assert!(sb.files().is_empty(), "{:?}", sb.files());
}

/// Run `bin args` with its stdout reader already gone; return (code, stderr).
fn with_stdout_closed(bin: &str, args: &[&str]) -> (i32, String) {
    let sb = Sandbox::new("closed");
    let mut child = Command::new(bin)
        .args(args)
        .env("AOIDE_ROOT", sb.dir.join("root"))
        .env("XDG_RUNTIME_DIR", sb.dir.join("run"))
        .env("AOIDE_AUDIT_LOG", sb.dir.join("root").join("log"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < DEADLINE, "{bin} {args:?} hung");
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn aoide_with_a_closed_stdout_exits_quietly() {
    let (code, err) = with_stdout_closed(env!("CARGO_BIN_EXE_aoide"), &["schema", "--json"]);
    assert_eq!((code, err.as_str()), (141, ""));
}

#[test]
fn aoided_help_with_a_closed_stdout_exits_quietly() {
    let (code, err) = with_stdout_closed(env!("CARGO_BIN_EXE_aoided"), &["--help"]);
    assert_eq!((code, err.as_str()), (141, ""));
}

#[test]
fn aoide_version_prints_the_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_aoide")).arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("aoide {}", env!("CARGO_PKG_VERSION")));
}
