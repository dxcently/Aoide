//! The desktop's calls into core. Each call is one core verb, execed through the core door: `aoide`
//! on `AOIDE_CORE_BIN`, else on PATH. aoided gates and audits every write that way, and strillix
//! links no core writer. Nothing is execed while aoided is not listening: the call waits, unsent,
//! until its socket answers. A call core answers, yes or no, is settled. A call whose exec failed
//! stays owed and is tried again on a later tick. No aoide binary at all skips the call with one log
//! line and no error: the desktop runs without core.

use aoide_conduct::shellbridge::run_core_step_with;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

/// How long the listening probe waits on aoided's socket before it counts aoided absent.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// What one core call came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    /// Core accepted the call.
    Done,
    /// Core ran the call and answered not-ok. The call is settled: the same fact is not sent again.
    Refused,
    /// The exec, spawn or wait failed, so core did not run the call. The call stays owed.
    NotRun,
    /// aoided is not listening, so nothing was sent. The call stays owed until aoided listens.
    Absent,
    /// No aoide binary could run. The call is skipped.
    Unavailable,
}

/// Exec one core verb through the door, with `stdin` piped in when it carries a payload. A daemon
/// that is not listening is not called. The probe connects and closes at once; aoided reads that
/// end of file and returns, running no verb and writing no audit line.
pub(crate) fn call<S: AsRef<str>>(argv: &[S], stdin: Option<&str>) -> Reply {
    if !aoided_listening() {
        return if core_binary_present() {
            Reply::Absent
        } else {
            Reply::Unavailable
        };
    }
    let argv: Vec<String> = argv.iter().map(|s| s.as_ref().to_string()).collect();
    match run_core_step_with(&argv, stdin) {
        Ok((true, _, _)) => Reply::Done,
        // A CLI that loses aoided mid-call answers not-ok: settled too, and pid liveness covers the window.
        Ok((false, _, _)) => Reply::Refused,
        Err(_) => Reply::NotRun,
    }
}

/// Is aoided's socket accepting connections? A connect that closes at once is all it takes.
fn aoided_listening() -> bool {
    let socket = aoide_storage::attest::daemon_socket_path();
    aoide_storage::attest::connect_bounded(&socket, PROBE_TIMEOUT).is_some()
}

/// Would the exec find an aoide binary? The same resolution the spawn uses: an absolute path is
/// checked where it is, a bare name is looked up on PATH.
fn core_binary_present() -> bool {
    let bin = aoide_protocol::bin::core_bin();
    if Path::new(&bin).is_absolute() {
        Path::new(&bin).is_file()
    } else {
        aoide_protocol::bin::on_path(&bin)
    }
}

/// Logs each problem once, not on every tick: a streak of one problem is logged when it starts, and
/// logged again after the call has succeeded in between. It also holds the facts core refused, so the
/// sweep does not ask for them again.
#[derive(Default)]
pub(crate) struct Latch {
    logged: HashSet<String>,
    refused: HashSet<String>,
}

impl Latch {
    /// Record one call's reply under `key`. True when the call is settled (done, refused, or skipped
    /// for want of a binary), so it is not owed again.
    pub(crate) fn reply(&mut self, key: String, what: &str, reply: Reply) -> bool {
        match reply {
            Reply::Done => {
                self.logged.remove(&key);
                true
            }
            Reply::Refused => {
                if self.logged.insert(key) {
                    eprintln!("[aoide/shellbridge] {what} was refused by core");
                }
                true
            }
            Reply::NotRun => {
                if self.logged.insert(key) {
                    eprintln!("[aoide/shellbridge] {what} did not run; retrying on a later tick");
                }
                false
            }
            Reply::Absent => {
                if self.logged.insert(key) {
                    eprintln!("[aoide/shellbridge] aoided is not running; {what} waits for it");
                }
                false
            }
            Reply::Unavailable => {
                if self.logged.insert(key) {
                    eprintln!("[aoide/shellbridge] no aoide binary; skipped {what}");
                }
                true
            }
        }
    }

    /// Has core refused this fact, named by its session, window and workspace?
    pub(crate) fn is_refused(&self, fact: &str) -> bool {
        self.refused.contains(fact)
    }

    /// Record that core refused this fact, so it is not asked again.
    pub(crate) fn refuse(&mut self, fact: String) {
        self.refused.insert(fact);
    }

    #[cfg(test)]
    pub(crate) fn logged(&self) -> usize {
        self.logged.len()
    }
}
