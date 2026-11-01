#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aoide_desktop_shell::asks::{change, watch, Change, Held};
use aoide_desktop_shell::exec::untrusted::is_unsafe;
use aoide_desktop_shell::exec::Exec;
use serde_json::Value;
use sha2::{Digest, Sha256};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");
const FIXTURE: &str = include_str!("fixtures/ask-watch.jsonl");
const WAIT: Duration = Duration::from_secs(5);

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ask-watch.jsonl")
}

/// An exec over the fake aoide, its asks file `asks` and the record of every call.
fn load(tag: &str, asks: &Path, extra: &[(&str, &str)]) -> (Exec, PathBuf) {
    let dir = std::env::temp_dir().join(format!("aoide-desk-asks-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    let record = dir.join("record.jsonl");
    let mut env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
        (OsString::from("FAKE_AOIDE_ASKS_FILE"), asks.as_os_str().to_owned()),
    ];
    env.extend(extra.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    let exec = Exec::load(Path::new(FAKE), &dir.join("home"), env).expect("the fake loads");
    (exec, record)
}

fn argv_of(record: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(record)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let call: Value = serde_json::from_str(line).expect("a record line is JSON");
            call["args"].as_array().expect("args").iter().map(|a| a.as_str().unwrap().to_owned()).collect()
        })
        .collect()
}

fn held(text: &str) -> Held {
    match change(text) {
        Change::Added(held) => *held,
        other => panic!("an added ask is expected, not {other:?}"),
    }
}

fn fixture_rows() -> Vec<&'static str> {
    FIXTURE.lines().collect()
}

#[test]
fn an_added_row_is_a_held_ask_with_the_fingerprint_core_derived() {
    let rows = fixture_rows();
    let approval = held(rows[0]);
    assert_eq!((approval.session.as_str(), approval.port, approval.ask_id), ("s-1", 4471, 1));
    assert_eq!(approval.fingerprint, "3f9a2c4be1d07a55");
    let view = approval.view();
    assert_eq!((view.kind, view.prompt.as_str(), view.answers.as_slice()), ("approval", "write?", ["yes".to_owned(), "no".to_owned()].as_slice()));
    let question = held(rows[1]).view();
    assert_eq!((question.kind, question.fingerprint.as_str()), ("question", "c01d5eed7ab1e002"));
    assert_eq!(question.options.len(), 2);
    assert_eq!((question.options[0].label.as_str(), question.options[0].description.as_str()), ("sunrise", "warm palette"));
    assert_eq!(question.options[1].description, "", "an absent description is empty");
    assert!(question.call.is_none() && question.digest.is_none(), "a question has no dialog");
}

#[test]
fn an_approvals_view_holds_the_whole_call_and_the_digest_of_it() {
    let view = held(fixture_rows()[0]).view();
    let call = view.call.expect("an approval shows its call");
    for part in ["\"callId\": \"tu1\"", "\"tool\": \"write\"", "\"path\": \"notes.md\"", "\"text\": \"hello\""] {
        assert!(call.contains(part), "{part} is in the call: {call}");
    }
    let expect: String = Sha256::digest(call.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(view.digest.as_deref(), Some(expect.as_str()));
    assert!(!view.capped);
}

#[test]
fn a_removed_row_is_a_removal_and_the_end_row_and_the_envelope_end_the_stream() {
    let removed = r#"{"type":"removed","sessionId":"s-1","port":4471,"askId":1,"ask":{"kind":"question","ask_id":1,"prompt":"p","options":[]},"fingerprint":"3f9a2c4be1d07a55","firstSeen":1}"#;
    assert_eq!(change(removed), Change::Removed { session: "s-1".to_owned(), port: 4471, ask_id: 1 });
    assert_eq!(change(r#"{"type":"end","reason":"interrupted"}"#), Change::End);
    assert_eq!(change(r#"{"command":"ask.watch","gated":false,"message":"closed","status":"ok"}"#), Change::End);
}

#[test]
fn a_row_that_is_not_cores_view_is_other_and_never_a_card() {
    let good = fixture_rows()[1];
    let raw = good.replace("Which rice?", "Which\u{202e} rice?");
    let other_kind = good.replace("\"question\"", "\"riddle\"");
    let other_id = good.replace("\"askId\": 2", "\"askId\": 9");
    let no_print = good.replace("c01d5eed7ab1e002", "");
    let bad_print = good.replace("c01d5eed7ab1e002", "c01d \u{1b}[0m");
    let raw_session = good.replace("\"s-1\"", "\"s\u{202e}1\"");
    let unknown_type = good.replace("\"added\"", "\"renamed\"");
    for (name, text) in [
        ("a raw hidden character", &raw),
        ("an ask kind this app does not read", &other_kind),
        ("an ask id that is not the row's", &other_id),
        ("no fingerprint", &no_print),
        ("a fingerprint that is not plain", &bad_print),
        ("a raw hidden character in the session", &raw_session),
        ("a row type this app does not read", &unknown_type),
        ("text that is not JSON", &"not json \u{1b}[31m \u{202e}".to_owned()),
    ] {
        match change(text) {
            Change::Other(shown) => {
                assert!(!shown.chars().any(is_unsafe), "{name}: the note is spelled: {shown:?}");
            }
            other => panic!("{name}: no card is made, but {other:?}"),
        }
    }
}

#[test]
fn hostile_text_core_has_spelled_stays_text_in_every_string_of_the_view() {
    let view = held(fixture_rows()[2]).view();
    let wire = serde_json::to_string(&view).expect("a view serializes");
    assert!(!wire.chars().any(|c| c == '\u{202e}' || c == '\u{1b}'), "a raw escape reaches the page: {wire}");
    assert_eq!(view.prompt, "run\\u{202e}?");
    let call = view.call.expect("an approval shows its call");
    assert!(call.contains("onerror=alert(1)>"), "the markup stays text: {call}");
    assert!(!call.chars().filter(|c| *c != '\n').any(is_unsafe), "{call}");
}

#[test]
fn the_watch_follows_ask_watch_json_and_is_ended_by_interrupt() {
    let (exec, record) = load("follow", &fixture_path(), &[]);
    let mut watch = watch(&exec).expect("the watch runs");
    let ids: Vec<u64> = (0..3)
        .map(|_| match watch.next(WAIT) {
            Some(Change::Added(held)) => held.ask_id,
            other => panic!("an added ask is expected, not {other:?}"),
        })
        .collect();
    assert_eq!(ids, [1, 2, 3]);
    let calls = argv_of(&record);
    assert_eq!(calls.last().expect("the watch is recorded"), &["ask", "watch", "--json"]);
    let rest = watch.close().expect("the watch ends");
    assert_eq!(rest.last(), Some(&Change::End), "the envelope the fake prints on SIGINT arrives at the close: {rest:?}");
}

#[test]
fn a_watch_core_refuses_is_an_error_with_core_s_reason_spelled() {
    let (exec, _) = load("refused", &fixture_path(), &[("FAKE_AOIDE_ASKS_REFUSE", "the asks store is unreadable \u{1b}[31m")]);
    let why = watch(&exec).err().expect("a refused watch is an error");
    assert!(why.contains("the asks store is unreadable"), "{why}");
    assert!(!why.chars().any(is_unsafe), "{why:?}");
}

#[test]
fn the_ask_watch_is_declared_for_the_terminal_door() {
    let (exec, _) = load("declared", &fixture_path(), &[]);
    let spec = exec.schema().command(&["ask", "watch"]).expect("the schema declares ask watch");
    assert!(spec.doors.iter().any(|d| d == "cli"), "the app's child is the terminal's door: {:?}", spec.doors);
}
