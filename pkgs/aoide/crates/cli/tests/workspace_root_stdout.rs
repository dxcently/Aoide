//! `aoide workspace root` — the launcher contract, verified through the REAL
//! binary rather than through the handler.
//!
//! The command exists so a launcher can write
//! `kitty --directory "$(aoide workspace root 2>/dev/null || echo "$HOME")"`,
//! which is only true if stdout carries ONE BARE PATH and NOTHING AT ALL when
//! the command refuses (a line of envelope text on stdout would be substituted
//! into the argv). That is the `special` arm in `aoide-cli`'s `run_cli`, and
//! this is what pins it: `--json` keeps the envelope, text mode does not.
//!
//! Every child runs under an ISOLATED AOIDE_ROOT/STAGE/STATE/LOG, so the real
//! `~/.aoide` is never read and never written — and so
//! `migrate_root_once` (which runs in the binary's `main`) has nothing to
//! move: all three of its branches are gated on those env vars being unset,
//! and all three are set here.

use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("aoide-workspace-root-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

struct Demo {
    root: PathBuf,
    stage: PathBuf,
}

impl Demo {
    fn new(tag: &str) -> Self {
        let root = tmp(tag);
        let stage = root.join("song").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        Self { root, stage }
    }

    /// One `aoide` invocation under the isolated root, returning
    /// `(stdout, stderr, exit code)`.
    fn aoide(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_aoide"))
            .args(args)
            .env("AOIDE_ROOT", &self.root)
            .env("AOIDE_STAGE_DIR", &self.stage)
            .env("AOIDE_STATE_DIR", self.root.join("state"))
            .env("AOIDE_AUDIT_LOG", self.root.join("log"))
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

/// Write `projects.json` directly (the stage file IS the store; the mutation
/// commands that write it are daemon-owned and covered by their own suites).
fn write_projects(stage: &Path, projects: &str) {
    std::fs::write(
        stage.join("projects.json"),
        format!("{{\"schemaVersion\":\"0\",\"projects\":[{projects}]}}"),
    )
    .unwrap();
}

#[test]
fn root_prints_one_bare_path_on_stdout_and_nothing_at_all_when_it_refuses() {
    let demo = Demo::new("stdout");
    let folder = demo.root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    let folder = folder.to_string_lossy().into_owned();
    write_projects(
        &demo.stage,
        &format!(
            r#"{{"name":"aoide","path":"{folder}","roots":["{folder}"],"workspaces":[3]}},
               {{"name":"cadenza","path":"","roots":[],"workspaces":[5]}}"#
        ),
    );

    // Bound with a folder: exactly the path, newline-terminated, exit 0.
    let (out, err, code) = demo.aoide(&["workspace", "root", "3"]);
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, format!("{folder}\n"), "stdout is the bare path and nothing else");

    // A NAME-ONLY project: non-zero, and stdout EMPTY (the `||` fires).
    let (out, err, code) = demo.aoide(&["workspace", "root", "5"]);
    assert_ne!(code, 0, "a folderless project has nothing to print");
    assert_eq!(out, "", "nothing on stdout, ever, on a refusal");
    assert!(err.contains("has no folder"), "the reason is on stderr: {err}");

    // Unbound workspace: same shape.
    let (out, err, code) = demo.aoide(&["workspace", "root", "9"]);
    assert_ne!(code, 0);
    assert_eq!(out, "", "nothing on stdout");
    assert!(err.contains("not bound"), "the reason is on stderr: {err}");

    // No workspace and no compositor here: a refusal asking for the number.
    let (out, _, code) = demo.aoide(&["workspace", "root"]);
    assert_ne!(code, 0);
    assert_eq!(out, "");

    // `--json` keeps the envelope, so a tool still gets the structure.
    let (out, _, code) = demo.aoide(&["workspace", "root", "3", "--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).expect("--json emits valid JSON");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["data"]["root"], folder);
    assert_eq!(v["data"]["project"], "aoide");
    assert_eq!(v["data"]["workspace"], 3);
}
