//! The smoke run's state, built only under the `smoke` feature. A smoke run
//! takes `--smoke <dir>` and writes into that directory: `ready` once the page
//! has loaded, and `record.jsonl` one line per command. Its confirm answers
//! from `scenario.json` there, so no input is injected.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::confirm::Confirm;

/// The most of a note the record keeps, in bytes.
const NOTE_CAP: usize = 4096;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static LAUNCHES: AtomicU64 = AtomicU64::new(0);

/// Starts the smoke run in `dir`, which the harness has made.
pub fn begin(dir: PathBuf) -> Result<(), String> {
    DIR.set(dir).map_err(|_| "the smoke run began twice".to_owned())
}

fn dir() -> Result<&'static Path, String> {
    DIR.get().map(PathBuf::as_path).ok_or_else(|| "not a smoke run: no --smoke directory".to_owned())
}

fn append(line: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir()?.join("record.jsonl"))
        .map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

/// Records one command the window ran: its name and its launch number, never its text.
pub fn record(command: &str) {
    let launch = LAUNCHES.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = append(&json!({"command": command, "launch": launch}));
}

/// Appends `text` as data, cut to `NOTE_CAP` bytes on a character boundary.
pub fn note(text: &str) -> Result<(), String> {
    let mut end = text.len().min(NOTE_CAP);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    append(&json!({"note": &text[..end]}))
}

/// Writes `ready`, once the main page has finished loading.
pub fn ready() -> Result<(), String> {
    fs::write(dir()?.join("ready"), b"").map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Answer {
    confirm: bool,
}

/// Answers each confirm from the scenario file. A file that does not read, or
/// does not parse, answers no.
pub struct Scenario;

impl Confirm for Scenario {
    fn confirm(&self, _verb: &str, _text: &str) -> bool {
        let Ok(dir) = dir() else { return false };
        fs::read_to_string(dir.join("scenario.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Answer>(&text).ok())
            .is_some_and(|answer| answer.confirm)
    }
}
