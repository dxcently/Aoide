//! The window's state and its calls. Each method is synchronous; the commands
//! run it on a blocking thread. No method holds a lock across a confirm or a
//! child: the chat map lock is taken only to insert or remove an entry, and the
//! asks record's lock only to read or change the record.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use aoide_desktop_shell::asks::{self, Change, Held};
use aoide_desktop_shell::chat::record::spell;
use aoide_desktop_shell::chat;
use aoide_desktop_shell::exec::untrusted::visible;
use aoide_desktop_shell::exec::{Exec, Flag, Output};
use aoide_desktop_shell::palette::{self, Refusal, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::confirm::Confirm;
use crate::wire::{ask_wire, wire, AskSink, Sink};

/// How long one poll of a chat's stream waits before the reader checks its stop flag.
const POLL: Duration = Duration::from_millis(100);

pub struct Window {
    exec: Exec,
    confirm: Box<dyn Confirm>,
    busy: AtomicBool,
    chats: Mutex<HashMap<String, Reader>>,
    harnesses: Mutex<HashMap<String, Option<String>>>,
    asks: Mutex<Option<Reader>>,
    held: Arc<Mutex<Vec<Held>>>,
    live: Arc<AtomicUsize>,
}

/// One reader thread: it owns a `Chat`, or the `Watch`, until `stop` is set.
struct Reader {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

/// A flag as the palette sends it: a name, and a value when it takes one.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagIn {
    pub name: String,
    pub value: Option<String>,
}

/// A finished call: its exit code, its stdout as JSON with every string spelled
/// (or spelled text when it is not JSON), and its stderr spelled.
#[derive(Debug, Serialize)]
pub struct Reply {
    pub code: Option<i32>,
    pub stdout: Value,
    pub stderr: String,
}

enum Ask {
    Yes,
    No,
    Busy,
}

impl Window {
    pub fn new(exec: Exec, confirm: Box<dyn Confirm>) -> Window {
        Window {
            exec,
            confirm,
            busy: AtomicBool::new(false),
            chats: Mutex::new(HashMap::new()),
            harnesses: Mutex::new(HashMap::new()),
            asks: Mutex::new(None),
            held: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// The held children the window runs now: one per open chat and one for the
    /// asks watch, each until its reader ends.
    pub fn live_children(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    /// The commands the palette offers the main window.
    pub fn palette(&self) -> Vec<Row> {
        let schema = self.exec.schema();
        palette::palette(schema, &palette::main_grants(schema))
    }

    /// Runs one command: the palette's check first, then the confirm for an act
    /// or none, then the call. A refused path, or a confirm that is not answered
    /// yes, starts no child.
    pub fn run(&self, path: &[String], flags: &[FlagIn], positionals: &[String]) -> Result<Reply, String> {
        let schema = self.exec.schema();
        let words: Vec<&str> = path.iter().map(String::as_str).collect();
        let flags_ref: Vec<Flag<'_>> =
            flags.iter().map(|f| Flag { name: &f.name, value: f.value.as_deref() }).collect();
        let positionals_ref: Vec<&str> = positionals.iter().map(String::as_str).collect();
        let grants = palette::main_grants(schema);
        palette::check(schema, &grants, &words, &flags_ref).map_err(|r| refusal_text(&r))?;
        let verb = schema.command(&words).and_then(|spec| spec.verb.clone()).unwrap_or_default();
        if matches!(verb.as_str(), "act" | "none") {
            match self.ask("Run", &argv_text(path, flags, positionals)) {
                Ask::Yes => {}
                Ask::No => return Err("cancelled: the confirm was not answered yes".to_owned()),
                Ask::Busy => return Err("refused: another confirm is open".to_owned()),
            }
        }
        let out = self
            .exec
            .call(&words, &flags_ref, &positionals_ref, None)
            .map_err(|e| visible(&e.to_string()))?;
        Ok(reply(out))
    }

    /// The sessions the person has, as `session find --all` prints them. The
    /// harness each one runs is kept for the sends that name it.
    pub fn sessions(&self) -> Result<Reply, String> {
        let listed =
            self.run(&["session".to_owned(), "find".to_owned()], &[FlagIn { name: "all".to_owned(), value: None }], &[])?;
        *self.harnesses.lock().expect("the harness map is not poisoned") = harnesses_in(&listed.stdout);
        Ok(listed)
    }

    /// Opens chat `id`: follows its stream and pushes each message to `sink` from
    /// one reader thread. A second open of the same id ends the first.
    pub fn chat_open(&self, id: &str, sink: Sink) -> Result<(), String> {
        self.chat_close(id)?;
        let mut chat = chat::follow(&self.exec, id).map_err(|e| visible(&e.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in = stop.clone();
        let live = self.live.clone();
        live.fetch_add(1, Ordering::SeqCst);
        let thread = std::thread::spawn(move || {
            while !stop_in.load(Ordering::SeqCst) {
                if let Some(push) = chat.next(POLL) {
                    let end = matches!(push, chat::record::Push::End);
                    sink(wire(push));
                    if end {
                        break;
                    }
                }
            }
            if let Ok(rest) = chat.close() {
                for push in rest {
                    sink(wire(push));
                }
            }
            live.fetch_sub(1, Ordering::SeqCst);
        });
        self.chats.lock().expect("the chat map is not poisoned").insert(id.to_owned(), Reader { stop, thread });
        Ok(())
    }

    /// Sends `text` to chat `id` through `aoide send`, naming the harness the
    /// session was listed with, so core refuses a session that no longer runs
    /// it. A session that is not listed is refused before any child starts. A
    /// send core refuses is an error, never a reply.
    pub fn chat_say(&self, id: &str, text: &str) -> Result<Reply, String> {
        let harness = self.harness_of(id)?;
        let sent = reply(chat::say(&self.exec, id, harness.as_deref(), text).map_err(|e| visible(&e.to_string()))?);
        match sent.code {
            Some(0) => Ok(sent),
            code => Err(refusal_of_send(&sent, code)),
        }
    }

    /// The harness session `id` was listed with, listing the sessions once when
    /// it is not known yet.
    fn harness_of(&self, id: &str) -> Result<Option<String>, String> {
        let known = |window: &Self| window.harnesses.lock().expect("the harness map is not poisoned").get(id).cloned();
        if let Some(harness) = known(self) {
            return Ok(harness);
        }
        self.sessions()?;
        known(self).ok_or_else(|| format!("refused: no session `{}` is listed, so nothing was sent", visible(id)))
    }

    /// Ends chat `id`: its reader interrupts the child and reaps it before this returns.
    pub fn chat_close(&self, id: &str) -> Result<(), String> {
        let reader = self.chats.lock().expect("the chat map is not poisoned").remove(id);
        if let Some(reader) = reader {
            reader.stop.store(true, Ordering::SeqCst);
            reader.thread.join().map_err(|_| "the chat's reader panicked".to_owned())?;
        }
        Ok(())
    }

    /// Follows core's `ask watch` and pushes each change to `sink` from one reader
    /// thread. The window keeps each ask it pushes, keyed by session and ask id;
    /// an answer is built from that record, never from what the page says. A
    /// second open ends the first.
    pub fn asks_open(&self, sink: AskSink) -> Result<(), String> {
        self.asks_close()?;
        let mut watch = asks::watch(&self.exec)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in = stop.clone();
        let held = self.held.clone();
        let live = self.live.clone();
        live.fetch_add(1, Ordering::SeqCst);
        let thread = std::thread::spawn(move || {
            while !stop_in.load(Ordering::SeqCst) {
                if let Some(change) = watch.next(POLL) {
                    let end = matches!(change, Change::End);
                    keep(&held, &change);
                    sink(ask_wire(change));
                    if end {
                        break;
                    }
                }
            }
            if let Ok(rest) = watch.close() {
                for change in rest {
                    sink(ask_wire(change));
                }
            }
            held.lock().expect("the asks record is not poisoned").clear();
            live.fetch_sub(1, Ordering::SeqCst);
        });
        *self.asks.lock().expect("the asks reader is not poisoned") = Some(Reader { stop, thread });
        Ok(())
    }

    /// Ends the asks watch: its reader interrupts the child and reaps it before
    /// this returns.
    pub fn asks_close(&self) -> Result<(), String> {
        let reader = self.asks.lock().expect("the asks reader is not poisoned").take();
        if let Some(reader) = reader {
            reader.stop.store(true, Ordering::SeqCst);
            reader.thread.join().map_err(|_| "the asks reader panicked".to_owned())?;
        }
        Ok(())
    }

    /// One confirm at a time: a confirm asked while another is open is refused.
    fn ask(&self, verb: &str, text: &str) -> Ask {
        if self.busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Ask::Busy;
        }
        let yes = self.confirm.confirm(verb, text);
        self.busy.store(false, Ordering::SeqCst);
        if yes {
            Ask::Yes
        } else {
            Ask::No
        }
    }
}

/// Applies one change to the record of the open asks. An ask is keyed by its
/// session and ask id: one added again replaces the record, and a removal drops
/// it only from the door that reported it.
fn keep(held: &Mutex<Vec<Held>>, change: &Change) {
    let mut record = held.lock().expect("the asks record is not poisoned");
    match change {
        Change::Added(ask) => {
            record.retain(|h| !(h.session == ask.session && h.ask_id == ask.ask_id));
            record.push((**ask).clone());
        }
        Change::Removed { session, port, ask_id } => {
            record.retain(|h| !(h.session == *session && h.ask_id == *ask_id && h.port == *port));
        }
        _ => {}
    }
}

fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Blocking(path) => format!("refused: `{}` runs until it is ended, so the palette does not start it", visible(path)),
        Refusal::Unknown(path) => format!("refused: no command `{}` is declared", visible(path)),
        Refusal::Unverified(path) => format!("refused: the schema names no verb for `{}`", visible(path)),
        Refusal::Floored { path, why } => format!("refused: `{}` {}", visible(path), why),
        Refusal::NotGranted(path) => format!("refused: the main window is not granted `{}`", visible(path)),
    }
}

/// The argv the confirm shows: one word per line, each spelled.
fn argv_text(path: &[String], flags: &[FlagIn], positionals: &[String]) -> String {
    let mut lines = vec!["aoide".to_owned()];
    lines.extend(path.iter().cloned());
    for flag in flags {
        lines.push(format!("--{}", flag.name));
        if let Some(value) = &flag.value {
            lines.push(value.clone());
        }
    }
    if !positionals.is_empty() {
        lines.push("--".to_owned());
        lines.extend(positionals.iter().cloned());
    }
    lines.iter().map(|line| visible(line)).collect::<Vec<_>>().join("\n")
}

/// The harness each listed session runs, by session id: none for a shell or a
/// row that names no agent.
fn harnesses_in(listing: &Value) -> HashMap<String, Option<String>> {
    let rows = listing["data"]["sessions"].as_array().into_iter().flatten();
    rows.filter_map(|row| {
        let id = row["sessionId"].as_str()?;
        let carries = &row["carries"];
        let agent = carries["agent"].as_str().filter(|agent| !agent.is_empty());
        let shell = carries["kind"].as_str() == Some("shell");
        Some((id.to_owned(), agent.filter(|_| !shell).map(str::to_owned)))
    })
    .collect()
}

/// Why a send that did not exit zero failed: core's message, else its stderr.
fn refusal_of_send(sent: &Reply, code: Option<i32>) -> String {
    let message = sent.stdout["message"].as_str().filter(|m| !m.is_empty());
    let why = message.map(str::to_owned).unwrap_or_else(|| sent.stderr.trim().to_owned());
    match why.as_str() {
        "" => format!("the send exited with {code:?}"),
        _ => why,
    }
}

fn reply(out: Output) -> Reply {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let stdout = match serde_json::from_str::<Value>(&text) {
        Ok(value) => spell(value),
        Err(_) => Value::String(visible(&text)),
    };
    Reply {
        code: out.status.code(),
        stdout,
        stderr: visible(&String::from_utf8_lossy(&out.stderr)),
    }
}
