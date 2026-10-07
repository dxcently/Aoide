//! The shell-out: one utterance to `verba-volantia dispatch`, one verdict back.
//!
//! Core links no VV code. It finds the binary the way a shell would, hands it
//! the utterance on stdin and the kit on `--out`, and reads the first JSON line
//! of stdout. Every way that can go wrong (no binary, no kit, a child that
//! hangs, dies or says something unreadable) is one taught [`Refusal`]; nothing
//! here decides what the verdict means.

use aoide_protocol::output::{Fix, Kind, Refusal};
use aoide_storage::config;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One dispatch is a forward pass over a sub-2 MB model; a child still silent
/// after this is hung, not slow.
const LIMIT: Duration = Duration::from_secs(20);
/// More than this on stdout is not a verdict; the rest is read and dropped so
/// the child never blocks on a full pipe.
const OUTPUT_CAP: u64 = 1 << 20;
const POLL: Duration = Duration::from_millis(10);
const KIT_FILES: &[&str] = &["meta.json", "model.safetensors"];
const TRAIN_FIX: &str = "aoide do kit --help";

pub struct Kit {
    pub binary: String,
    pub dir: PathBuf,
}

impl Kit {
    /// The binary and kit directory this box is configured for: `[verba]` in
    /// `config.toml`, else `verba-volantia` on `PATH` and `$AOIDE_ROOT/verba/aoide`.
    pub fn configured() -> Result<Kit, Refusal> {
        let loaded = config::load().map_err(|e| {
            Refusal::new(
                Kind::Refused,
                "the runtime config cannot be read",
                format!("`aoide do` reads `[verba]` from it, and {e}"),
                Fix::Run("aoide config".into()),
            )
        })?;
        let verba = loaded.config.verba;
        let binary = if verba.binary.trim().is_empty() { config::default_verba_binary() } else { verba.binary };
        let dir = match verba.weights_dir.trim() {
            "" => aoide_storage::fs::root().join("verba").join("aoide"),
            dir => PathBuf::from(dir),
        };
        Ok(Kit { binary, dir })
    }

    /// Both halves present, else the refusal that says which and how to supply it.
    pub fn check(&self) -> Result<(), Refusal> {
        if !has_binary(&self.binary) {
            return Err(Refusal::new(
                Kind::Refused,
                format!("`{}` is not installed", self.binary),
                "`aoide do` shells out to the verba-volantia classifier (github.com/noah427/verba-volantia, \
                 `cargo build --release`) and finds none on PATH or at `[verba] binary`",
                Fix::Run("aoide config set verba.binary /path/to/verba-volantia".into()),
            ));
        }
        let missing: Vec<&str> = KIT_FILES.iter().copied().filter(|f| !self.dir.join(f).is_file()).collect();
        if !missing.is_empty() {
            return Err(Refusal::new(
                Kind::Refused,
                format!("no trained kit in `{}`", self.dir.display()),
                format!(
                    "the classifier reads a directory holding {} (and an optional lexicon.txt); {} not there",
                    KIT_FILES.join(" and "),
                    missing.join(" and ")
                ),
                Fix::Run(TRAIN_FIX.into()),
            ));
        }
        Ok(())
    }
}

fn has_binary(name: &str) -> bool {
    if name.contains(std::path::MAIN_SEPARATOR) || name.contains('/') {
        Path::new(name).is_file()
    } else {
        aoide_protocol::bin::on_path(name)
    }
}

/// The verdict `verba-volantia dispatch` printed for `utterance`.
pub fn classify(kit: &Kit, utterance: &str) -> Result<Value, Refusal> {
    kit.check()?;
    let mut cmd = Command::new(&kit.binary);
    cmd.arg("dispatch").arg("--out").arg(&kit.dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = run(cmd, utterance, LIMIT).map_err(|e| failure(kit, e))?;
    let last_error = out.stderr.lines().last().map(str::trim).filter(|l| !l.is_empty());
    if !out.ok {
        let why = match last_error {
            Some(last) => format!("it exited with an error: {last}"),
            None => "it exited with an error and said nothing".to_string(),
        };
        return Err(Refusal::new(Kind::Failed, "the classifier gave no answer", why, Fix::Run(TRAIN_FIX.into())));
    }
    let Some(line) = out.stdout.lines().map(str::trim).find(|l| l.starts_with('{')) else {
        return Err(Refusal::new(
            Kind::Failed,
            "the classifier gave no answer",
            "it printed no verdict; it reads nothing in the utterance to classify",
            Fix::Run(TRAIN_FIX.into()),
        ));
    };
    serde_json::from_str::<Value>(line)
        .ok()
        .filter(|v| v["intent"].is_string())
        .ok_or_else(|| {
            Refusal::new(
                Kind::Failed,
                "the classifier's answer cannot be read",
                format!("`{}` printed a line that is not a verdict (no JSON object with an `intent`)", kit.binary),
                Fix::Run(TRAIN_FIX.into()),
            )
        })
}

fn failure(kit: &Kit, e: RunError) -> Refusal {
    let (what, why) = match e {
        RunError::Spawn(e) => (format!("`{}` could not be started", kit.binary), format!("the operating system refused it: {e}")),
        RunError::TimedOut => (
            format!("`{}` did not answer", kit.binary),
            format!("it was still running after {} seconds and was stopped", LIMIT.as_secs()),
        ),
        RunError::Io(e) => (format!("`{}` could not be read", kit.binary), format!("its output failed: {e}")),
    };
    Refusal::new(Kind::Failed, what, why, Fix::Run(TRAIN_FIX.into()))
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = (&mut pipe).take(OUTPUT_CAP).read_to_end(&mut bytes);
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

enum RunError {
    Spawn(std::io::Error),
    TimedOut,
    Io(std::io::Error),
}

struct Ran {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Run `cmd` with `line` on its stdin, bounded by `limit`. Both pipes are
/// drained on threads so a chatty child never blocks on a full buffer, and the
/// write is a thread too so a child that never reads cannot hold the caller.
fn run(mut cmd: Command, line: &str, limit: Duration) -> Result<Ran, RunError> {
    let mut child = cmd.spawn().map_err(RunError::Spawn)?;
    let mut stdin = child.stdin.take();
    let line = format!("{line}\n");
    std::thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(line.as_bytes());
        }
    });
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait().map_err(RunError::Io)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            None => std::thread::sleep(POLL),
        }
    };
    let join = |h: std::thread::JoinHandle<String>| h.join().unwrap_or_default();
    Ok(Ran { ok: status.success(), stdout: join(stdout), stderr: join(stderr) })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg(script).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }

    #[test]
    fn a_child_still_running_at_the_limit_is_stopped_and_reported_as_timed_out() {
        let started = Instant::now();
        let out = run(sh("sleep 5"), "x", Duration::from_millis(200));
        assert!(matches!(out, Err(RunError::TimedOut)));
        assert!(started.elapsed() < Duration::from_secs(4), "the caller was held past the limit");
    }

    #[test]
    fn output_past_the_cap_is_dropped_without_blocking_the_child() {
        let out = run(sh("head -c 3000000 /dev/zero | tr '\\0' 'a'; echo; echo '{}'"), "x", Duration::from_secs(20)).ok().unwrap();
        assert!(out.ok && out.stdout.len() as u64 <= OUTPUT_CAP);
    }
}
