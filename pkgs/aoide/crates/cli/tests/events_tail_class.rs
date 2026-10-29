//! `aoide events tail --class state --json`, run as the built binary over a feed
//! it follows. The library's `poll_once` is what the other tests call; this one
//! proves the CLI's own `--class` plumbing reaches the filter.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The one child this test started. It is killed and reaped by its own handle
/// when the test ends, however it ends; no other process is signalled.
struct Tail(Child);

impl Drop for Tail {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One feed line of `class`, with `files` as its payload when there are any.
fn feed_line(class: &str, kind: &str, files: &[&str]) -> Value {
    let payload = if files.is_empty() { json!({}) } else { json!({ "files": files }) };
    json!({"v": 0, "ts": 1, "class": class, "kind": kind, "source": "aoided", "payload": payload})
}

fn append(events: &Path, line: &Value) {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(events).unwrap();
    writeln!(f, "{line}").unwrap();
}

#[test]
fn events_tail_with_class_state_prints_only_the_state_lines() {
    let dir = aoide_test_support::short_tmp("tail-state");
    for sub in ["root", "stage"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let events = dir.join("events.jsonl");
    std::fs::write(&events, b"").unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_aoide"))
        .args(["events", "tail", "--class", "state", "--json"])
        .env("AOIDE_DAEMON_EVENTS", &events)
        .env("AOIDE_ROOT", dir.join("root"))
        .env("AOIDE_STAGE_DIR", dir.join("stage"))
        .env("XDG_RUNTIME_DIR", &dir)
        .env("HOME", &dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut tail = Tail(child);
    let stdout = tail.0.stdout.take().unwrap();
    let printed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&printed);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            sink.lock().unwrap().push(line);
        }
    });
    let seen = |name: &str| printed.lock().unwrap().iter().any(|l| l.contains(name));

    // `tail` follows only what lands after it opened the file, so attach first:
    // probe lines (a `state` line for a file the test never uses) until one comes back.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen("probe.json") {
        assert!(Instant::now() < deadline, "the tail never attached to {events:?}");
        append(&events, &feed_line("state", "changed", &["probe.json"]));
        std::thread::sleep(Duration::from_millis(100));
    }

    for line in [
        feed_line("audit", "started", &[]),
        feed_line("gate", "proposed", &[]),
        feed_line("content", "released", &[]),
        feed_line("notification", "forwarded", &[]),
        feed_line("secret", "released", &[]),
        feed_line("state", "changed", &["sessions.json"]),
        feed_line("state", "changed", &["graph.json"]),
    ] {
        append(&events, &line);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while !(seen("sessions.json") && seen("graph.json")) {
        assert!(Instant::now() < deadline, "the state lines never came back");
        std::thread::sleep(Duration::from_millis(50));
    }
    // A full window past the last line: anything else the filter let through would show by now.
    std::thread::sleep(Duration::from_millis(1500));
    drop(tail);

    let got: Vec<Value> = printed
        .lock()
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("non-JSON line `{l}`: {e}")))
        .filter(|v: &Value| v["payload"]["files"] != json!(["probe.json"]))
        .collect();
    assert_eq!(got.len(), 2, "exactly the two state lines: {got:?}");
    assert!(got.iter().all(|v| v["class"] == "state"), "{got:?}");
    assert_eq!(got[0]["payload"]["files"], json!(["sessions.json"]), "{got:?}");
    assert_eq!(got[1]["payload"]["files"], json!(["graph.json"]), "{got:?}");
}
