//! A stand-in for `eidolon web`, for the door tests (feature `fakes`). It takes
//! exactly `web --bind 127.0.0.1:0 --cwd DIR`, serves the door crate's fake
//! door on it, and writes its banner and token file as the door does.
//!
//! The test drives it through `DIR/.fake-turn`, one control word at a time:
//!
//! - `running` and `idle` change the turn. Each change pushes one `turn-state`
//!   frame while an events stream is open. A change made while none is open is
//!   pushed when the next stream opens.
//! - `end` ends the open events stream. With none open it is dropped.
//! - `cancel-409` queues one `409` for the next `POST /api/cancel`, the answer
//!   an idle door gives. A cancel served while running answers `204`, turns the
//!   door idle and pushes `running=false`.
//! - `ask-approval ID JSON` raises an approval under that id with that input,
//!   and `settle ID LABEL` settles it. Either one rewrites the head, so a stream
//!   opened later replays the asks still pending, and is pushed at once while a
//!   stream is open.
//! - `answer-409`, `answer-422` and `answer-401` queue one answer of that status
//!   for the next `POST /api/answer`. An answer served as `204` leaves its ask
//!   pending until a `settle` says otherwise.
//! - `flood N` pushes N `text-delta` frames while a stream is open.
//! - `asks N` raises approvals under ids 1 to N, with the same input.
//! - `hello-bad` makes the first frame of every head something other than a
//!   `hello`; `hello-good` puts the hello back.
//! - `session NAME` names the session in each hello from now on.
//!
//! `DIR/.fake-mode`, read once at start, changes the start-up:
//!
//! - `exit-first` exits 3 before it prints anything.
//! - `no-banner` prints no banner, and runs until it is signalled.
//! - `remote-address` names a non-loopback address in its banner.
//! - `token-outside` names a token path outside the runtime dir.
//! - `token-space` names a token path with a trailing space.
//! - `banner-128k` prints 128 KiB before its banner.
//! - `ignore-sigterm` ignores the first SIGTERM.
//! - `cancel-stuck` answers a cancel `204` and keeps its turn running.
//! - `hello-bad` makes the first frame of every head something other than a
//!   `hello`, from the start.
//!
//! Each control word applied, each request served (an answer with its body) and
//! each signal received is appended to `DIR/.fake-log` in order. On SIGTERM it
//! waits, at most 20 s, until no events stream is open, removes its token file
//! and exits 0. Past 20 s it exits 1 and leaves its token.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aoide_desktop_door::fake::FakeDoor;
use serde_json::{json, Value};

const TOKEN: &str = "fake-door-token";
const POLL: Duration = Duration::from_millis(20);
const DRAIN: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Turn {
    Running,
    Idle,
}

/// What the main loop and the signal thread share, under one lock.
struct Core {
    dir: PathBuf,
    log: fs::File,
    served: usize,
    seen: String,
    turn: Turn,
    pending: Option<Turn>,
    hello: Value,
    hello_bad: bool,
    stuck: bool,
    /// The asks pending on this door, each as the frame it was raised with, by id.
    asks: BTreeMap<u64, Value>,
}

impl Core {
    fn open(dir: PathBuf, hello: Value, stuck: bool) -> Core {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(".fake-log"))
            .expect("the log opens");
        Core {
            dir,
            log,
            served: 0,
            seen: String::new(),
            turn: Turn::Idle,
            pending: None,
            hello,
            hello_bad: false,
            stuck,
            asks: BTreeMap::new(),
        }
    }

    fn line(&mut self, text: &str) {
        writeln!(self.log, "{text}").expect("the log is written");
    }

    /// Logs the requests served since the last call, an answer with its body,
    /// and turns the door idle when a cancel is served while it runs.
    fn sync(&mut self, door: &FakeDoor) {
        let requests = door.requests();
        for request in &requests[self.served..] {
            let body = if request.path == "/api/answer" {
                format!(" {}", String::from_utf8_lossy(&request.body))
            } else {
                String::new()
            };
            self.line(&format!("{} {}{body}", request.method, request.path));
            if request.method == "POST" && request.path == "/api/cancel" && self.turn == Turn::Running && !self.stuck {
                self.turn = Turn::Idle;
                self.pending = Some(Turn::Idle);
            }
        }
        self.served = requests.len();
    }

    /// Applies a changed control word.
    fn control(&mut self, door: &FakeDoor) {
        let word = fs::read_to_string(self.dir.join(".fake-turn")).unwrap_or_default().trim().to_owned();
        if word.is_empty() || word == self.seen {
            return;
        }
        match word.as_str() {
            "running" => {
                self.turn = Turn::Running;
                self.pending = Some(Turn::Running);
            }
            "idle" => {
                self.turn = Turn::Idle;
                self.pending = Some(Turn::Idle);
            }
            "end" => {
                if door.streaming() {
                    door.end();
                }
            }
            "cancel-409" => door.reply("/api/cancel", 409, ""),
            "answer-409" => door.reply("/api/answer", 409, ""),
            "answer-422" => door.reply("/api/answer", 422, &json!({"error": "the ask is not taken"}).to_string()),
            "answer-401" => door.reply("/api/answer", 401, ""),
            "hello-bad" => {
                self.hello_bad = true;
                self.head(door);
            }
            "hello-good" => {
                self.hello_bad = false;
                self.head(door);
            }
            _ => {
                if let Some((id, input)) = word.strip_prefix("ask-approval ").and_then(|rest| rest.split_once(' ')) {
                    let input: Value = serde_json::from_str(input).expect("the ask's input is JSON");
                    self.raise(door, id.parse().expect("the ask id is a number"), input);
                } else if let Some((id, label)) = word.strip_prefix("settle ").and_then(|rest| rest.split_once(' ')) {
                    self.settle(door, id.parse().expect("the ask id is a number"), label);
                } else if let Some(count) = word.strip_prefix("asks ") {
                    for id in 1..=count.parse::<u64>().expect("asks takes a count") {
                        self.raise(door, id, json!({"path": "a.txt"}));
                    }
                } else if let Some(count) = word.strip_prefix("flood ") {
                    if door.streaming() {
                        for _ in 0..count.parse::<usize>().expect("flood takes a count") {
                            door.push(&json!({"type": "text-delta", "text": "x"}).to_string());
                        }
                    }
                } else if let Some(name) = word.strip_prefix("session ") {
                    self.hello["session"] = json!(name);
                    self.head(door);
                } else {
                    return;
                }
            }
        }
        self.seen = word.clone();
        self.line(&format!("control {word}"));
    }

    /// Puts the head in place: the hello, with its count of pending asks, then
    /// each pending ask. A stream opened from now on replays it.
    fn head(&self, door: &FakeDoor) {
        let mut hello = self.hello.clone();
        hello["pending"] = json!(self.asks.len());
        let first = if self.hello_bad {
            json!({"type": "turn-state", "running": false}).to_string()
        } else {
            hello.to_string()
        };
        let mut frames = vec![first];
        frames.extend(self.asks.values().map(Value::to_string));
        let frames: Vec<&str> = frames.iter().map(String::as_str).collect();
        door.set_head(&frames);
    }

    /// Raises an approval under `id`, replacing any ask already there, and pushes
    /// it while a stream is open.
    fn raise(&mut self, door: &FakeDoor, id: u64, input: Value) {
        let frame = json!({
            "type": "ask",
            "kind": "approval",
            "ask_id": id,
            "call_id": format!("tu{id}"),
            "tool": "write",
            "input": input,
            "prompt": "write?",
            "reason": null,
            "structural": false,
            "judged": null,
            "yolo": false,
            "answers": ["yes", "no"],
        });
        self.asks.insert(id, frame.clone());
        self.head(door);
        if door.streaming() {
            door.push(&frame.to_string());
        }
    }

    /// Settles the ask under `id`: it leaves the head, and its settle is pushed
    /// while a stream is open.
    fn settle(&mut self, door: &FakeDoor, id: u64, label: &str) {
        self.asks.remove(&id);
        self.head(door);
        if door.streaming() {
            let settled = json!({"type": "ask-settled", "ask_id": id, "how": "answered", "answer": label});
            door.push(&settled.to_string());
        }
    }

    /// Pushes a pending turn-state change once a stream is open to take it.
    fn push(&mut self, door: &FakeDoor) {
        let Some(turn) = self.pending else {
            return;
        };
        if door.streaming() {
            door.push(&json!({ "type": "turn-state", "running": turn == Turn::Running }).to_string());
            self.pending = None;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = match args.as_slice() {
        [web, bind, addr, flag, dir] if web == "web" && bind == "--bind" && addr == "127.0.0.1:0" && flag == "--cwd" => {
            PathBuf::from(dir)
        }
        _ => {
            eprintln!("fake-eidolon takes: web --bind 127.0.0.1:0 --cwd DIR");
            std::process::exit(2);
        }
    };
    let mode = fs::read_to_string(dir.join(".fake-mode")).unwrap_or_default().trim().to_owned();
    if mode == "exit-first" {
        std::process::exit(3);
    }
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_default());
    // Blocked before any thread starts, so every thread inherits the block and
    // the signal is taken by sigwait.
    let set = term_set();
    // SAFETY: `set` is a valid signal set; the block is set before any thread starts.
    unsafe {
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    let door = Arc::new(FakeDoor::start(&runtime, TOKEN));
    let hello = json!({
        "type": "hello",
        "session": dir.join("session.eid").display().to_string(),
        "cwd": dir.display().to_string(),
        "model": "fake",
        "yolo": false,
        "pending": 0,
        "protocol": 1,
    });
    let core = Arc::new(Mutex::new(Core::open(dir.clone(), hello, mode == "cancel-stuck")));
    core.lock().expect("the core lock").hello_bad = mode == "hello-bad";
    core.lock().expect("the core lock").head(&door);
    if mode == "banner-128k" {
        for _ in 0..65_536 {
            println!("x");
        }
    }
    if mode != "no-banner" {
        let port = door.addr().port();
        let addr = if mode == "remote-address" { format!("192.0.2.1:{port}") } else { door.addr().to_string() };
        let token = match mode.as_str() {
            "token-outside" => dir.join("elsewhere.token"),
            "token-space" => PathBuf::from(format!("{} ", door.token_file().display())),
            _ => door.token_file(),
        };
        println!("listening on http://{addr}/");
        println!("token file: {}", token.display());
    }
    {
        let (door, core) = (door.clone(), core.clone());
        let ignore_first = mode == "ignore-sigterm";
        thread::spawn(move || await_term(&door, &core, ignore_first));
    }
    loop {
        thread::sleep(POLL);
        let mut core = core.lock().expect("the core lock");
        core.sync(&door);
        core.control(&door);
        core.push(&door);
    }
}

/// Waits for SIGTERM, logs it after the requests served before it, and exits
/// once no events stream is open. With `ignore_first`, the first SIGTERM is
/// logged and ignored, and the second one is the one that exits.
fn await_term(door: &FakeDoor, core: &Mutex<Core>, ignore_first: bool) {
    let set = term_set();
    let mut signal: libc::c_int = 0;
    let mut ignore = ignore_first;
    loop {
        // SAFETY: `set` is a valid signal set, and `signal` is a valid out pointer.
        unsafe {
            libc::sigwait(&set, &mut signal);
        }
        {
            let mut core = core.lock().expect("the core lock");
            core.sync(door);
            core.line("SIGTERM");
        }
        if !ignore {
            break;
        }
        ignore = false;
    }
    let end = Instant::now() + DRAIN;
    while door.streaming() && Instant::now() < end {
        thread::sleep(POLL);
    }
    if door.streaming() {
        std::process::exit(1);
    }
    let _ = fs::remove_file(door.token_file());
    std::process::exit(0);
}

fn term_set() -> libc::sigset_t {
    // SAFETY: a zeroed sigset_t is valid, and sigemptyset initializes it before sigaddset uses it.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        set
    }
}
