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
//!
//! Each control word applied, each request served and each signal received is
//! appended to `DIR/.fake-log` in order. On SIGTERM it waits, at most 20 s, until
//! no events stream is open, removes its token file and exits 0. Past 20 s it
//! exits 1 and leaves its token.
#![allow(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aoide_desktop_door::fake::FakeDoor;
use serde_json::json;

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
}

impl Core {
    fn open(dir: PathBuf) -> Core {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(".fake-log"))
            .expect("the log opens");
        Core { dir, log, served: 0, seen: String::new(), turn: Turn::Idle, pending: None }
    }

    fn line(&mut self, text: &str) {
        writeln!(self.log, "{text}").expect("the log is written");
    }

    /// Logs the requests served since the last call, and turns the door idle
    /// when a cancel is served while it runs.
    fn sync(&mut self, door: &FakeDoor) {
        let requests = door.requests();
        for request in &requests[self.served..] {
            self.line(&format!("{} {}", request.method, request.path));
            if request.method == "POST" && request.path == "/api/cancel" && self.turn == Turn::Running {
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
            _ => return,
        }
        self.seen = word.clone();
        self.line(&format!("control {word}"));
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
    })
    .to_string();
    door.set_head(&[hello.as_str()]);
    let core = Arc::new(Mutex::new(Core::open(dir)));
    println!("listening on http://{}/", door.addr());
    println!("token file: {}", door.token_file().display());
    {
        let (door, core) = (door.clone(), core.clone());
        thread::spawn(move || await_term(&door, &core));
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
/// once no events stream is open.
fn await_term(door: &FakeDoor, core: &Mutex<Core>) {
    let set = term_set();
    let mut signal: libc::c_int = 0;
    // SAFETY: `set` is a valid signal set, and `signal` is a valid out pointer.
    unsafe {
        libc::sigwait(&set, &mut signal);
    }
    {
        let mut core = core.lock().expect("the core lock");
        core.sync(door);
        core.line("SIGTERM");
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
