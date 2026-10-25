#![deny(unsafe_code)]
#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use aoide_desktop_shell::exec::{Exec, ExecError, Flag, PinError, OUTPUT_CAP};
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
fn text_that_starts_with_dashes_or_holds_equals_or_newlines_stays_positional() {
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
    assert_eq!(env_keys(send), ["AOIDE_SESSION_ID", "FAKE_AOIDE_RECORD", "PATH"]);
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
    for name in ["1KEY", "BAD-NAME", "", "PATH", "HOME", "XDG_RUNTIME_DIR", "AOIDE_ROOT"] {
        let (exec, record) = with_token_env("bad-name", name);
        let before = records(&record).len();
        let err = exec.call(&["send"], &[], &["hi"], None).expect_err("the verb is refused");
        assert!(matches!(err, ExecError::VaultKeysUnknown(_)), "{name:?}: {err:?}");
        assert_eq!(records(&record).len(), before, "{name:?}: no child started");
    }
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
