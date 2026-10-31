//! `aoide ask watch` ends on a real SIGINT: its closing line is the next line
//! after the rows, then the command's outcome prints, and it exits 0.
//! The child is the binary this package builds; the test signals it by pid.
//! Every child runs under an isolated root, so no real `~/.aoide` is read.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGINT: i32 = 2;

fn root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("aoide-ask-watch-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for dir in ["stage", "state", "run"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("run"), fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(
        root.join("stage").join("asks.json"),
        r#"{"written":1,"asks":[{"sessionId":"s0","port":9,"askId":7,"firstSeen":1,
            "ask":{"kind":"question","ask_id":7,"prompt":"pick one","options":[{"label":"a"}]}}]}"#,
    )
    .unwrap();
    root
}

fn watch(root: &PathBuf, args: &[&str]) -> (Child, Receiver<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aoide"))
        .args(["ask", "watch"])
        .args(args)
        .env("AOIDE_ROOT", root)
        .env("STRILLIX_ROOT", root)
        .env("AOIDE_STAGE_DIR", root.join("stage"))
        .env("AOIDE_STATE_DIR", root.join("state"))
        .env("AOIDE_AUDIT_LOG", root.join("log"))
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("aoide ask watch starts");
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (child, rx)
}

/// Sends SIGINT, waits for the child to exit, and returns what it printed after.
fn interrupt(mut child: Child, rx: &Receiver<String>) -> (std::process::ExitStatus, Vec<String>) {
    assert_eq!(unsafe { kill(child.id() as i32, SIGINT) }, 0, "the signal is sent");
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the child is waited on") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("ask watch did not exit after SIGINT");
        }
        thread::sleep(Duration::from_millis(50));
    };
    (status, rx.iter().collect())
}

#[test]
fn a_sigint_to_ask_watch_json_prints_the_closing_line_and_exits() {
    let root = root("json");
    let (child, rx) = watch(&root, &["--json"]);
    let first = rx.recv_timeout(Duration::from_secs(20)).expect("the open ask is printed");
    assert!(first.contains("\"added\""), "{first}");
    let (status, rest) = interrupt(child, &rx);
    assert!(status.success(), "ask watch exits 0 after SIGINT: {status:?}");
    assert_eq!(rest.first().map(String::as_str), Some(r#"{"reason":"interrupted","type":"end"}"#), "{rest:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_sigint_to_ask_watch_text_prints_the_closing_line_and_exits() {
    let root = root("text");
    let (child, rx) = watch(&root, &[]);
    let first = rx.recv_timeout(Duration::from_secs(20)).expect("the open ask is printed");
    assert_eq!(first, "added s0 port 9 ask 7");
    let (status, rest) = interrupt(child, &rx);
    assert!(status.success(), "ask watch exits 0 after SIGINT: {status:?}");
    assert_eq!(rest.first().map(String::as_str), Some("watch ended: interrupted"), "{rest:?}");
    let _ = fs::remove_dir_all(&root);
}
