#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aoide_desktop_shell::chat::cases::CASES;
use aoide_desktop_shell::chat::record::Push;
use aoide_desktop_shell::chat::{follow, say, FollowError};
use aoide_desktop_shell::exec::Exec;
use serde_json::{json, Value};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");
const WAIT: Duration = Duration::from_secs(5);

/// A scratch dir for `tag`, holding the fake's follow file of `lines` and the
/// exec the fake loads with `extra` on top. Returns the exec and the record of
/// every call.
fn load(tag: &str, lines: &[&[u8]], extra: &[(&str, &str)]) -> (Exec, PathBuf) {
    let dir = std::env::temp_dir().join(format!("aoide-desk-chat-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    let follow_file = dir.join("follow.txt");
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line);
        bytes.push(b'\n');
    }
    fs::write(&follow_file, bytes).expect("the follow file is written");
    let record = dir.join("record.jsonl");
    let mut env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
        (OsString::from("FAKE_AOIDE_FOLLOW_FILE"), follow_file.into_os_string()),
    ];
    env.extend(extra.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    let exec = Exec::load(Path::new(FAKE), &dir.join("home"), env).expect("the fake loads");
    (exec, record)
}

fn records(record: &Path) -> Vec<Value> {
    fs::read_to_string(record)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line is JSON"))
        .collect()
}

fn argv_of(call: &Value) -> Vec<String> {
    call["args"].as_array().expect("the args are recorded").iter().map(|a| a.as_str().unwrap().to_owned()).collect()
}

fn env_names(call: &Value) -> Vec<String> {
    let mut names: Vec<String> = call["env"].as_object().expect("the env is recorded").keys().cloned().collect();
    names.sort();
    names
}

#[test]
fn say_sends_with_its_id_submit_and_yes_and_the_text_as_one_positional() {
    let (exec, record) = load("say", &[], &[]);
    say(&exec, "s1", "hi there").expect("the send runs");
    let last = records(&record).pop().expect("the send is recorded");
    assert_eq!(argv_of(&last), ["send", "--json", "--id=s1", "--submit", "--yes", "--", "hi there"]);
}

#[test]
fn text_that_starts_with_dashes_and_holds_a_newline_stays_one_positional() {
    let (exec, record) = load("say-text", &[], &[]);
    say(&exec, "s1", "--yes\nrm -rf").expect("the send runs");
    let args = argv_of(&records(&record).pop().expect("the send is recorded"));
    assert_eq!(args.len(), 7, "{args:?}");
    assert_eq!(args[6], "--yes\nrm -rf");
}

#[test]
fn say_scrubs_the_session_id_and_both_autogate_switches_from_the_send_child() {
    let (exec, record) = load(
        "say-env",
        &[],
        &[
            ("AOIDE_SESSION_ID", "sess-1"),
            ("AOIDE_CONDUCT_AUTOGATE", "1"),
            ("AOIDE_CONDUCT_SIBLING_AUTOGATE", "1"),
        ],
    );
    say(&exec, "s1", "hi").expect("the send runs");
    let names = env_names(&records(&record).pop().expect("the send is recorded"));
    for dropped in ["AOIDE_SESSION_ID", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_CONDUCT_SIBLING_AUTOGATE"] {
        assert!(!names.iter().any(|n| n == dropped), "{dropped} reaches the send child: {names:?}");
    }
}

#[test]
fn a_session_whose_trace_answers_is_followed_with_follow_and_tail() {
    let (exec, record) = load("trace-args", &[b"{\"kind\":\"say\"}"], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the trace answers");
    assert_eq!(chat.case(), "trace");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"kind": "say"}))));
    let args = argv_of(&records(&record).pop().expect("the trace is recorded"));
    assert_eq!(args, ["session", "trace", "--json", "--follow", "--tail=200", "--", "eidolon-1"]);
    chat.close().expect("the stream ends");
}

#[test]
fn a_line_arrives_spelled_and_a_line_that_is_not_json_arrives_as_other() {
    let record_line = br#"{"kind":"say","text":"a\u202eb\u001b[31m"}"#;
    let text_line = "not json \u{1b}[0m \u{202e}".as_bytes();
    let (exec, _) = load("spelled", &[record_line, text_line], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the trace answers");
    assert_eq!(
        chat.next(WAIT),
        Some(Push::Record(json!({"kind": "say", "text": "a\\u{202e}b\\x1b[31m"})))
    );
    assert_eq!(chat.next(WAIT), Some(Push::Other("not json \\x1b[0m \\u{202e}".to_owned())));
    chat.close().expect("the stream ends");
}

#[test]
fn the_envelope_a_follow_prints_at_its_end_arrives_and_the_reader_survives_it() {
    let (exec, _) = load("envelope", &[b"{\"kind\":\"say\"}"], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the trace answers");
    assert!(chat.next(WAIT).is_some());
    let rest = chat.close().expect("the stream ends");
    assert_eq!(rest, [Push::Record(json!({"ok": true, "followed": true}))]);
}

#[test]
fn a_session_with_a_pty_and_no_trace_is_watched_and_its_frame_comes_first() {
    let (exec, _) = load("watch", &[b"plain line"], &[]);
    let mut chat = follow(&exec, "pty-1").expect("the watch answers");
    assert_eq!(chat.case(), "watch");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"frame": "live", "id": "pty-1"}))));
    assert_eq!(chat.next(WAIT), Some(Push::Other("plain line".to_owned())));
    chat.close().expect("the stream ends");
}

#[test]
fn a_hook_only_session_is_refused_and_each_case_says_why() {
    let (exec, _) = load("refused", &[], &[]);
    match follow(&exec, "hook-1") {
        Err(FollowError::Refused(refused)) => {
            let cases: Vec<&str> = refused.reasons.iter().map(|r| r.case).collect();
            assert_eq!(cases, ["trace", "watch"]);
            assert!(refused.reasons.iter().all(|r| r.why.contains("keeps no stream")), "{refused:?}");
        }
        Err(other) => panic!("the session is refused as a session, not an exec error: {other}"),
        Ok(_) => panic!("a hook-only session is followed"),
    }
}

#[test]
fn every_case_runs_a_verb_the_schema_declares() {
    let (exec, _) = load("registered", &[], &[]);
    let mut names: Vec<&str> = CASES.iter().map(|c| c.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), CASES.len(), "each case is named once");
    for case in CASES {
        assert!(exec.schema().command(case.words).is_some(), "the schema declares `{}`", case.words.join(" "));
    }
}

#[test]
fn the_chat_record_is_parsed_in_one_module_only() {
    let chat = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("chat");
    for entry in fs::read_dir(&chat).expect("the chat module is read") {
        let file = entry.expect("an entry").path();
        let text = fs::read_to_string(&file).expect("a source is read");
        let parser = file.file_name().is_some_and(|name| name == "record.rs");
        assert!(parser || !text.contains("serde_json::from_str"), "{} parses JSON outside record.rs", file.display());
    }
}
