//! `aoide do` through the REAL binary, against a fake `verba-volantia`.
//!
//! The contract under test is the shape a pipeline relies on:
//! `$(aoide do "…")` substitutes ONE copy-pasteable command, so text mode prints
//! the bare line on stdout and, on every refusal, NOTHING on stdout — the taught
//! what/why/fix is on stderr and the exit code is 1. `--json` keeps the
//! envelope. And the printed line is a line the same binary parses.
//!
//! Every child runs under an ISOLATED AOIDE_ROOT whose `config.toml` names the
//! fake by absolute path, so the real `~/.aoide`, the real PATH and any real
//! classifier are never touched.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Demo {
    root: PathBuf,
}

impl Demo {
    fn new(tag: &str, script: Option<&str>) -> Self {
        let root = std::env::temp_dir().join(format!("aoide-do-stdout-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let kit = root.join("verba").join("aoide");
        std::fs::create_dir_all(&kit).unwrap();
        for f in ["meta.json", "model.safetensors"] {
            std::fs::write(kit.join(f), "{}").unwrap();
        }
        let binary = root.join(if script.is_some() { "verba-volantia" } else { "not-installed" });
        if let Some(script) = script {
            std::fs::write(&binary, format!("#!/bin/sh\nread -r line\n{script}\n")).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(root.join("config.toml"), format!("[verba]\nbinary = {:?}\n", binary.display().to_string())).unwrap();
        Self { root }
    }

    fn aoide(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_aoide"))
            .args(args)
            .env("AOIDE_ROOT", &self.root)
            .env("AOIDE_STAGE_DIR", self.root.join("song").join("stage"))
            .env("AOIDE_STATE_DIR", self.root.join("state"))
            .env("AOIDE_AUDIT_LOG", self.root.join("log"))
            .env_remove("AOIDE_CONFIG")
            .output()
            .expect("the aoide binary runs");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn say(verdict: &str) -> String {
    format!("printf '%s\\n' '{verdict}'")
}

const ACCEPT: &str = r#"{"accept":true,"candidates":[{"intent":"session_trace","score":0.82}],"conflicts":[],"intent":"session_trace","margin":0.77,"slots":{"id":"abc123"},"threshold":0.64,"trailing_editorial_text":null}"#;
const ABSTAIN: &str = r#"{"accept":false,"candidates":[{"intent":"session_trace","score":0.31},{"intent":"session_prune","score":0.2}],"conflicts":[],"intent":"session_trace","margin":0.11,"slots":{"id":"zz"},"threshold":0.64,"trailing_editorial_text":null}"#;

#[test]
fn an_accepted_sentence_prints_one_bare_command_that_the_same_binary_parses() {
    let demo = Demo::new("accept", Some(&say(ACCEPT)));
    let (out, err, code) = demo.aoide(&["do", "show", "me", "the", "trace", "for", "abc123"]);
    assert_eq!((code, err.as_str()), (0, ""), "stderr: {err}");
    assert_eq!(out, "aoide session trace abc123\n", "stdout is the bare command and nothing else");

    let words: Vec<&str> = out.split_whitespace().skip(1).collect();
    let (_, err, code) = demo.aoide(&words);
    assert_ne!(code, 2, "the printed line parses as a command of this binary: {err}");

    let (out, _, code) = demo.aoide(&["do", "show the trace for abc123", "--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).expect("--json emits valid JSON");
    assert_eq!((v["status"].as_str(), v["data"]["command"].as_str()), (Some("ok"), Some("aoide session trace abc123")));
    assert_eq!(v["data"]["verdict"]["intent"], "session_trace");
}

#[test]
fn every_refusal_leaves_stdout_empty_and_teaches_on_stderr() {
    let abstains = Demo::new("abstain", Some(&say(ABSTAIN)));
    let (out, err, code) = abstains.aoide(&["do", "make me a sandwich"]);
    assert_eq!((out.as_str(), code), ("", 1));
    assert!(err.contains("did not resolve to one command") && err.contains("0.31  aoide session trace zz"), "{err}");
    assert!(err.contains("0.20  aoide session prune"), "{err}");

    let silent = Demo::new("missing", None);
    let (out, err, code) = silent.aoide(&["do", "anything"]);
    assert_eq!((out.as_str(), code), ("", 1));
    assert!(err.contains("is not installed") && err.contains("fix: aoide config set verba.binary"), "{err}");

    let garbled = Demo::new("garbled", Some("echo 'not a verdict'"));
    let (out, err, code) = garbled.aoide(&["do", "anything"]);
    assert_eq!((out.as_str(), code), ("", 1));
    assert!(err.contains("gave no answer"), "{err}");
}

#[test]
fn an_empty_sentence_is_a_usage_error_before_anything_is_spawned() {
    let demo = Demo::new("empty", Some(&say(ACCEPT)));
    let (out, err, code) = demo.aoide(&["do"]);
    assert_eq!((out.as_str(), code), ("", 2));
    assert!(err.contains("needs <utterance>"), "{err}");
}

#[test]
fn the_kit_spec_prints_bare_and_writes_where_it_is_told() {
    let demo = Demo::new("kit", None);
    let (out, err, code) = demo.aoide(&["do", "kit"]);
    assert_eq!((code, err.as_str()), (0, ""), "{err}");
    let spec: serde_json::Value = serde_json::from_str(&out).expect("stdout is the spec and only the spec");
    assert!(spec["functions"].as_array().unwrap().iter().any(|f| f["name"] == "session_trace"));

    let file: &Path = &demo.root.join("templates.json");
    let (out, err, code) = demo.aoide(&["do", "kit", "--out", &file.display().to_string()]);
    assert_eq!((code, err.as_str()), (0, ""), "{err}");
    assert!(out.contains("verba-volantia gen --spec templates.json --data data/aoide"), "{out}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(file).unwrap()).unwrap(), spec);
}
