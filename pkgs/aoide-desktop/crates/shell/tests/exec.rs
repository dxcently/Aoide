#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use std::os::unix::process::ExitStatusExt;

use aoide_desktop_shell::exec::{Event, Exec, ExecError, Flag, PinError, LINE_CAP, OUTPUT_CAP};
use serde_json::{json, Value};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aoide-desk-exec-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    dir
}

/// Loads the fake with `extra` on top of its record path, and returns the exec
/// and the file its calls are recorded to.
fn load(tag: &str, extra: &[(&str, &str)]) -> (Exec, PathBuf) {
    let dir = scratch(tag);
    let record = dir.join("record.jsonl");
    let mut env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
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

fn env_keys(call: &Value) -> Vec<String> {
    let mut keys: Vec<String> = call["env"]
        .as_object()
        .expect("the env is recorded")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[test]
fn spawned_text_that_starts_with_dashes_or_holds_equals_or_newlines_stays_positional() {
    let (exec, record) = load("positional", &[]);
    exec.call(&["send"], &[], &["--yes", "a=b", "one\ntwo"], None).expect("the call runs");
    let last = records(&record).pop().expect("the send is recorded");
    assert_eq!(last["args"], json!(["send", "--json", "--", "--yes", "a=b", "one\ntwo"]));
}

#[test]
fn a_declared_flag_reaches_the_child_in_its_own_form() {
    let (exec, record) = load("flags", &[]);
    let flags = [Flag { name: "id", value: Some("s1") }, Flag { name: "yes", value: None }];
    exec.call(&["send"], &flags, &["hi"], None).expect("the call runs");
    let last = records(&record).pop().expect("the send is recorded");
    assert_eq!(last["args"], json!(["send", "--json", "--id=s1", "--yes", "--", "hi"]));
}

#[test]
fn an_undeclared_flag_starts_no_child() {
    let (exec, record) = load("undeclared", &[]);
    let before = records(&record).len();
    let flags = [Flag { name: "nope", value: None }];
    let err = exec.call(&["send"], &flags, &["hi"], None).expect_err("the call is refused");
    assert!(matches!(err, ExecError::Undeclared { .. }), "{err:?}");
    assert_eq!(records(&record).len(), before, "a refused call starts no child");
}

#[test]
fn each_child_gets_only_its_scrubbed_environment() {
    let (exec, record) = load(
        "env",
        &[
            ("AOIDE_SESSION_ID", "sess-1"),
            ("AOIDE_MELETE_KEY", "melete-key"),
            ("AOIDE_TEST_VAULT_TOKEN", "vault-token"),
        ],
    );
    exec.call(&["send"], &[], &["hi"], None).expect("send runs");
    exec.call(&["melete", "status"], &[], &[], None).expect("melete status runs");
    let calls = records(&record);
    let (send, melete) = (&calls[calls.len() - 2], &calls[calls.len() - 1]);
    assert_eq!(env_keys(send), ["FAKE_AOIDE_RECORD", "PATH"]);
    assert_eq!(
        env_keys(melete),
        ["AOIDE_MELETE_KEY", "AOIDE_SESSION_ID", "FAKE_AOIDE_RECORD", "PATH"]
    );
}

#[test]
fn a_non_zero_exit_returns_its_stderr_verbatim() {
    let text = "refused: the session is gated\nsecond line: \u{e9}\n";
    let (exec, _) = load("stderr", &[("FAKE_AOIDE_EXIT", "3"), ("FAKE_AOIDE_STDERR", text)]);
    let out = exec.call(&["send"], &[], &["hi"], None).expect("the call returns the child's answer");
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(out.stderr, text.as_bytes());
}

#[test]
fn stdin_is_closed_unless_bytes_are_given() {
    let (exec, record) = load("stdin", &[]);
    exec.call(&["send"], &[], &["hi"], None).expect("the call runs");
    exec.call(&["send"], &[], &["hi"], Some(&b"piped bytes"[..])).expect("the call runs");
    let calls = records(&record);
    assert_eq!(calls[calls.len() - 2]["stdin"], "");
    assert_eq!(calls[calls.len() - 1]["stdin"], "piped bytes");
}

#[test]
fn output_at_the_cap_is_kept() {
    let at = OUTPUT_CAP.to_string();
    let (exec, _) = load("cap-at", &[("FAKE_AOIDE_STDOUT_BYTES", at.as_str())]);
    let out = exec.call(&["send"], &[], &["hi"], None).expect("output at the cap is kept");
    assert_eq!(out.stdout.len(), OUTPUT_CAP);
}

#[test]
fn stdout_past_the_cap_is_refused() {
    let over = (OUTPUT_CAP + 1).to_string();
    let (exec, _) = load("cap-stdout", &[("FAKE_AOIDE_STDOUT_BYTES", over.as_str())]);
    let err = exec.call(&["send"], &[], &["hi"], None).expect_err("output past the cap is refused");
    assert!(matches!(err, ExecError::OutputTooLarge { stream: "stdout" }), "{err:?}");
}

#[test]
fn stderr_past_the_cap_is_refused() {
    let over = (OUTPUT_CAP + 1).to_string();
    let (exec, _) = load("cap-stderr", &[("FAKE_AOIDE_STDERR_BYTES", over.as_str())]);
    let err = exec.call(&["send"], &[], &["hi"], None).expect_err("output past the cap is refused");
    assert!(matches!(err, ExecError::OutputTooLarge { stream: "stderr" }), "{err:?}");
}

/// Loads the fake with a config whose one vault names `token_env`.
fn with_token_env(tag: &str, token_env: &str) -> (Exec, PathBuf) {
    let config = json!({"data": {"config": {"context": {"vaults": {
        "v": {"endpoint": "e", "vault": "v", "tokenEnv": token_env}}}}}});
    let text = config.to_string();
    load(tag, &[("FAKE_AOIDE_CONFIG", text.as_str())])
}

#[test]
fn an_unreadable_config_refuses_every_verb_but_config_with_no_child() {
    let (exec, record) = load("no-config", &[("FAKE_AOIDE_NO_CONFIG", "1")]);
    let before = records(&record).len();
    let err = exec.call(&["send"], &[], &["hi"], None).expect_err("the verb is refused");
    assert!(matches!(err, ExecError::VaultKeysUnknown(_)), "{err:?}");
    assert_eq!(records(&record).len(), before, "no child started");
    exec.call(&["config"], &[], &[], None).expect("config still runs, to read the names");
    assert_eq!(records(&record).len(), before + 1);
}

#[test]
fn a_token_env_the_app_may_not_read_refuses_every_verb_with_no_child() {
    for name in ["1KEY", "BAD-NAME", "", "PATH", "HOME", "HOME=x", "XDG_RUNTIME_DIR", "AOIDE_ROOT"] {
        let (exec, record) = with_token_env("bad-name", name);
        let before = records(&record).len();
        let err = exec.call(&["send"], &[], &["hi"], None).expect_err("the verb is refused");
        assert!(matches!(err, ExecError::VaultKeysUnknown(_)), "{name:?}: {err:?}");
        assert_eq!(records(&record).len(), before, "{name:?}: no child started");
    }
}

#[test]
fn a_token_env_of_any_length_or_in_lower_case_is_accepted_and_scrubbed() {
    let long = "K".repeat(10 * 1024);
    for name in ["path", long.as_str()] {
        let config = json!({"data": {"config": {"context": {"vaults": {
            "v": {"endpoint": "e", "vault": "v", "tokenEnv": name}}}}}});
        let text = config.to_string();
        let extra = [("FAKE_AOIDE_CONFIG", text.as_str()), (name, "vault-token")];
        let (exec, record) = load("accepted-name", &extra);
        exec.call(&["send"], &[], &["hi"], None).expect("the verb runs");
        let last = records(&record).pop().expect("the send is recorded");
        assert!(!env_keys(&last).iter().any(|key| key == name), "the vault key reaches no child");
    }
}

#[test]
fn a_deadline_longer_than_the_default_runs_under_the_default() {
    let (exec, _) = load("deadline-max", &[]);
    exec.deadline(Duration::MAX).call(&["send"], &[], &["hi"], None).expect("the call runs");
}

#[test]
fn a_bad_token_env_is_shown_through_visible() {
    let (exec, _) = with_token_env("spelled", "KEY\u{202e}X");
    let text = exec.call(&["send"], &[], &["hi"], None).expect_err("refused").to_string();
    assert!(text.contains("KEY\\u{202e}X") && !text.contains('\u{202e}'), "{text}");
}

#[test]
fn a_call_past_its_deadline_is_refused_as_a_timeout() {
    let (exec, record) = load("deadline", &[("FAKE_AOIDE_SLEEP", "30")]);
    let exec = exec.deadline(Duration::from_millis(300));
    let started = Instant::now();
    let err = exec.call(&["send"], &[], &["hi"], None).expect_err("the call is refused");
    assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5), "refused at its deadline, not after the sleep");
    assert_eq!(records(&record).len(), 3, "schema, config and the one call");
}

#[test]
fn a_later_longer_deadline_does_not_lengthen_an_earlier_one() {
    let (exec, _record) = load("deadline-chain", &[("FAKE_AOIDE_SLEEP", "30")]);
    let exec = exec.deadline(Duration::from_millis(300)).deadline(Duration::MAX);
    let started = Instant::now();
    let err = exec.call(&["send"], &[], &["hi"], None).expect_err("the call is refused");
    assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5), "refused at the first deadline, not after the sleep");
}

#[test]
fn a_program_that_will_not_print_its_schema_is_refused_at_load() {
    let dir = scratch("no-schema");
    let env = vec![
        (OsString::from("FAKE_AOIDE_NO_SCHEMA"), OsString::from("1")),
        (OsString::from("FAKE_AOIDE_EXIT"), OsString::from("2")),
        (OsString::from("FAKE_AOIDE_STDERR"), OsString::from("old aoide")),
    ];
    let err = Exec::load(Path::new(FAKE), &dir, env).err().expect("the load is refused");
    assert!(matches!(err, ExecError::Bootstrap { .. }), "{err:?}");
}

#[test]
fn a_relative_program_is_refused_at_load() {
    let dir = scratch("relative-load");
    let err = Exec::load(Path::new("aoide"), &dir, Vec::new()).err().expect("the load is refused");
    assert!(matches!(err, ExecError::Pin(PinError::Relative(_))), "{err:?}");
}

/// How long a test waits for one event from a held child before it fails.
const WAIT: Duration = Duration::from_secs(5);

/// Loads the fake with a follow file that holds `lines`, one per line, and `extra`
/// on top. Returns the exec and the record file of every call.
fn held(tag: &str, lines: &[Vec<u8>], extra: &[(&str, &str)]) -> (Exec, PathBuf) {
    let dir = scratch(tag);
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

/// The `--follow` flag, the one flag a held trace is started with.
const FOLLOW: Flag<'static> = Flag { name: "follow", value: None };

/// Whether a process has not exited: its stat reads, and it is not a zombie.
fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|text| {
        text.rfind(')').and_then(|at| text[at + 1..].split_whitespace().next()) != Some("Z")
    })
}

#[test]
fn a_held_child_streams_its_lines_before_it_exits() {
    let lines = [b"one".to_vec(), b"two".to_vec(), b"three".to_vec()];
    let (exec, _) = held("stream", &lines, &[]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    let pid = follow.id();
    for line in &lines {
        assert_eq!(follow.next(WAIT), Some(Event::Line(line.clone())));
    }
    assert!(alive(pid), "the child still runs: its lines came before any exit");
    follow.end().expect("the follow ends");
}

#[test]
fn a_line_past_the_cap_is_dropped_and_counted_and_the_next_line_arrives() {
    let long = vec![b'x'; 2 * 1024 * 1024];
    assert!(long.len() > LINE_CAP, "the line is past the cap");
    let (exec, _) = held("cap", &[b"before".to_vec(), long.clone(), b"after".to_vec()], &[]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"before".to_vec())));
    assert_eq!(follow.next(WAIT), Some(Event::Dropped(long.len())));
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"after".to_vec())));
    follow.end().expect("the follow ends");
}

#[test]
fn ending_a_follow_interrupts_the_child_which_prints_its_end_line_and_is_reaped() {
    let (exec, _) = held("end", &[b"one".to_vec()], &[]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    let pid = follow.id();
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"one".to_vec())));
    let ended = follow.end().expect("the follow ends");
    let status = ended.status.expect("the child exits within the bound");
    assert_eq!(status.code(), Some(0), "it ended on the interrupt, which the fake answers with an exit of zero");
    assert_ne!(status.signal(), Some(9), "the child was not killed");
    assert_eq!(ended.rest, [Event::Line(br#"{"ok":true,"followed":true}"#.to_vec())]);
    assert!(!Path::new(&format!("/proc/{pid}")).exists(), "the pid is gone once the call returns");
}

#[test]
fn what_a_child_prints_after_its_interrupt_is_capped_and_its_overflow_is_counted() {
    let (exec, _) = held("flood-after", &[b"one".to_vec()], &[("FAKE_AOIDE_AFTER_INT", "40")]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"one".to_vec())));
    let ended = follow.end().expect("the follow ends");
    let kept: usize = ended.rest.iter().map(|e| if let Event::Line(l) = e { l.len() } else { 0 }).sum();
    assert!(kept <= aoide_desktop_shell::exec::QUEUE_BYTES, "{kept} bytes were kept after the interrupt");
    let dropped: usize = ended.rest.iter().map(|e| if let Event::Dropped(n) = e { *n } else { 0 }).sum();
    assert!(dropped > 0, "the overflow is counted in bytes");
}

#[test]
fn ending_a_follow_with_sixteen_unread_lines_returns_within_the_bound() {
    let lines: Vec<Vec<u8>> = (0..16).map(|_| vec![b'a'; 512 * 1024]).collect();
    let (exec, _) = held("unread16", &lines, &[]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    std::thread::sleep(Duration::from_millis(800));
    let started = Instant::now();
    let ended = follow.end().expect("the follow ends");
    assert!(started.elapsed() < Duration::from_secs(5), "the end returns within its bound");
    assert!(ended.status.is_some(), "the child exits within the bound");
}

#[test]
fn dropping_a_follow_interrupts_its_child_and_reaps_it() {
    let (exec, _) = held("drop", &[b"one".to_vec()], &[]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    let pid = follow.id();
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"one".to_vec())));
    drop(follow);
    let until = Instant::now() + Duration::from_secs(5);
    while Path::new(&format!("/proc/{pid}")).exists() {
        assert!(Instant::now() < until, "the dropped follow's child is gone within 5 s");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn an_undeclared_flag_starts_no_held_child() {
    let (exec, record) = held("held-undeclared", &[], &[]);
    let before = records(&record).len();
    let flags = [Flag { name: "nope", value: None }];
    let err = exec.follow(&["session", "trace"], &flags, &["ab"]).err().expect("the follow is refused");
    assert!(matches!(err, ExecError::Undeclared { .. }), "{err:?}");
    assert_eq!(records(&record).len(), before, "a refused follow starts no child");
}

#[test]
fn a_held_child_gets_only_its_scrubbed_environment() {
    let (exec, record) = held("held-env", &[b"ready".to_vec()], &[("AOIDE_SESSION_ID", "sess-1"), ("AOIDE_MELETE_KEY", "k")]);
    let follow = exec.follow(&["session", "trace"], &[FOLLOW], &["ab"]).expect("the follow starts");
    assert_eq!(follow.next(WAIT), Some(Event::Line(b"ready".to_vec())));
    follow.end().expect("the follow ends");
    let held = records(&record).pop().expect("the held child is recorded");
    assert_eq!(env_keys(&held), ["AOIDE_SESSION_ID", "FAKE_AOIDE_FOLLOW_FILE", "FAKE_AOIDE_RECORD", "PATH"]);
}
