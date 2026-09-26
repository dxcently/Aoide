//! The one place a command LINE is handed to this host's own interpreter.
//!
//! A command line stored as operator-authored TEXT — a secrets backend
//! template, `upkeep.verifyCommand` — needs a shell to run it, and which shell
//! that is, is a fact about the HOST, never about the caller: `sh -c` on Unix,
//! `cmd /C` on native Windows, which has no `sh`. Two crates ask the same
//! question of the same host, so the answer lives here, once
//! (`pkgs/aoide/crates/AGENTS.md`: expose, never copy).
//!
//! **The line goes over VERBATIM on Windows.** `cmd` is not a
//! `CommandLineToArgvW` program: it re-applies its own quote rules to `/C`'s
//! remainder, so [`Command::arg`]'s MSVC-style quoting mangles any line
//! carrying inner quotes — measured on native Windows, the identical
//! `findstr "^" >C:\…\k.txt` exits 1 through `arg` (whose inner quotes became
//! `\"`, which `cmd` does not read as an escape) and 0 through `raw_arg`. The
//! Unix arm keeps `.arg`, where exactly one argv element is what `sh -c` must
//! receive.
//!
//! A caller owns everything else — `current_dir`, `Stdio`, the wait — so this
//! seam returns a [`Command`] it has only pointed at the right interpreter.
//! What it does NOT do is decide whether the line is meaningful: a line naming
//! a program this host lacks still fails in the child, where it belongs.
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// `command`, ready to spawn under this host's interpreter. The caller sets
/// `current_dir`/`Stdio` and waits; on a non-zero exit the CHILD's own status
/// is the answer (no caller of this seam treats a spawn that cannot even start
/// as anything but that child's failure).
pub fn command(command: &str) -> Command {
    #[cfg(unix)]
    let c = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    #[cfg(windows)]
    let c = {
        let mut c = Command::new("cmd");
        c.arg("/C");
        c.raw_arg(command);
        c
    };
    c
}

#[cfg(test)]
mod tests {
    /// Native Windows only, and deliberately so: on Unix this seam is `sh -c`
    /// with one argv element, which every template-running test in the two
    /// consuming crates already exercises end to end. What needs its own
    /// assertion is the Windows arm's VERBATIM hand-over — the second line
    /// below is the discriminator, and it is exactly the shape `arg` breaks.
    #[cfg(windows)]
    #[test]
    fn a_line_runs_through_this_hosts_interpreter_and_a_quoted_program_survives() {
        let status = super::command("exit 3")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("spawn this host's interpreter");
        assert_eq!(status.code(), Some(3), "the line's own exit code is the verdict");

        let status = super::command("\"%COMSPEC%\" /C exit 0")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("spawn this host's interpreter");
        assert!(
            status.success(),
            "a line that quotes its own program must arrive verbatim, got {status}"
        );
    }
}
