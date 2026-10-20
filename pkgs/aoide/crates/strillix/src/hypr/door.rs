//! The desktop's calls into core. Each call is one core verb, execed through the core door: `aoide`
//! on `AOIDE_CORE_BIN`, else on PATH. aoided gates and audits every write that way, and strillix
//! links no core writer. A call is not execed while aoided is not listening: it waits, unsent,
//! until its socket answers. A yes from core settles a call, and so does a no given while aoided
//! listens. A no given while aoided is not listening leaves the call owed, since the verb may
//! already have reached aoided. A call whose exec failed stays owed and is tried again on a later
//! tick. No aoide binary at all skips the call with one log line and no error: the desktop runs
//! without core.

use aoide_conduct::shellbridge::run_core_step_with;
use std::collections::HashSet;
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
    /// aoided is not listening: the call was not sent, or core answered no after aoided stopped
    /// listening. The call stays owed until aoided listens.
    Absent,
    /// No aoide binary could run. The call is skipped.
    Unavailable,
}

/// Exec one core verb through the door, with `stdin` piped in when it carries a payload. A daemon
/// that is not listening is not called. The probe connects and closes at once; aoided reads that
/// end of file and returns, running no verb and writing no audit line.
pub(crate) fn call<S: AsRef<str>>(argv: &[S], stdin: Option<&str>) -> Reply {
    if !aoide_protocol::bin::core_present() {
        return Reply::Unavailable;
    }
    if !aoided_listening() {
        return Reply::Absent;
    }
    let argv: Vec<String> = argv.iter().map(|s| s.as_ref().to_string()).collect();
    match run_core_step_with(&argv, stdin) {
        Ok((true, _, _)) => Reply::Done,
        // A not-ok while aoided is not listening waits, as a call made while aoided is down
        // does: the CLI answers it without sending when it cannot reach aoided, and a verb
        // aoided read before it stopped is sent again. Any other not-ok is core's answer, and
        // it is settled.
        Ok((false, _, _)) if !aoided_listening() => Reply::Absent,
        Ok((false, _, _)) => Reply::Refused,
        Err(_) => Reply::NotRun,
    }
}

/// Is aoided's socket accepting connections? A connect that closes at once is all it takes.
fn aoided_listening() -> bool {
    let socket = aoide_storage::attest::daemon_socket_path();
    aoide_storage::attest::connect_bounded(&socket, PROBE_TIMEOUT).is_some()
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

    /// Logs a problem that is not a core call, once per streak: logged when `key` starts, and again
    /// after [`Latch::recovered`] has run for it.
    pub(crate) fn problem(&mut self, key: &str, message: &str) {
        if self.logged.insert(key.to_string()) {
            eprintln!("{message}");
        }
    }

    /// Ends the streak that [`Latch::problem`] logged under `key`.
    pub(crate) fn recovered(&mut self, key: &str) {
        self.logged.remove(key);
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
