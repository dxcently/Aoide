//! The PTY capability — ONE seam, two arms. A controlled terminal a child
//! agent runs on, whose master side this process reads, types into, resizes,
//! and closes.
//!
//! Nothing here is a second discovery path: every item below is the same
//! concept written twice, once per host, with the same contract on both. What
//! the caller sees is a [`Pty`], a [`PtyChild`], a [`Console`] and an
//! [`Inbox`] — never a raw fd, a signal, or a `HANDLE`.
//!
//! | the caller's need | Unix (`libc`) | native Windows (`windows-sys`) |
//! | --- | --- | --- |
//! | open a terminal for a child | `openpty(3)` | `CreatePseudoConsole` |
//! | make it the child's controlling tty | `setsid` + `TIOCSCTTY` + `dup2` in `pre_exec` | the `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` attribute on `CreateProcessW` |
//! | read what the child prints | `read(master)` | `ReadFile(output)` |
//! | type into the child | `write(master)` | `WriteFile(input)` |
//! | resize it | `ioctl(TIOCSWINSZ)` | `ResizePseudoConsole` |
//! | the real console's geometry | `ioctl(TIOCGWINSZ)` on stdin | `GetConsoleScreenBufferInfo` on stdout |
//! | raw keystrokes (Ctrl-C as a byte) | `termios` (`cfmakeraw`) | `SetConsoleMode` (`ENABLE_VIRTUAL_TERMINAL_INPUT`, echo/line/processed off) |
//! | a resize notification | `SIGWINCH` | the console's own size, re-read on the tick |
//! | wait for any source to be readable | `poll(2)` over fds, with a hung-up stdin counted as READY (`POLLHUP`/`POLLERR`/`POLLNVAL`, never `POLLIN` alone — an empty pipe whose writer closed reports only `POLLHUP`) | `WaitForMultipleObjects` over a CONSOLE's stdin and the inbox's `WSAEventSelect` events, with the output pipe POLLED — a synchronous anonymous pipe's read handle is ALWAYS signalled, so any other pipe in a wait-any array wins the index race forever and no other source is ever reported; a non-console stdin is POLLED and read by the byte for the same reason |
//! | where the interactive pump's stdout goes | `write(2)` on fd 1, unbuffered | `WriteFile` on the stdout handle, unbuffered |
//! | the child is alive | `waitpid(WNOHANG)` | `GetExitCodeProcess` |
//! | the child is gone, and how | `waitpid` + `ExitStatusExt::signal` | `WaitForSingleObject` + the exit code |
//! | end the child | `SIGKILL` to its process GROUP | `TerminateProcess` |
//! | close the channel | drop the master fd | `ClosePseudoConsole`, after the output is drained |
//! | the foreground process group | `tcgetpgrp(master)` | NONE (see below) |
//! | a descendant still holds the terminal | `POLLHUP` on the master | NONE — `None`, never a guessed `false` (see below) |
//!
//! **Where the arms genuinely differ, by name.**
//!
//! - **`Ended::Signal` never appears on Windows.** Windows has no signals; a
//!   process ends with a code and nothing else. A console Ctrl-C termination
//!   arrives there as its own NTSTATUS code, reported as the SIGNED `i32` the
//!   record carries — `0xC000013A` is `exitCode: -1073741510` — rather than
//!   dressed as a signal name this host does not have.
//! - **The deadline kill reaches the direct child only.** Unix's `killpg`
//!   reaches the whole process group the child leads; Windows has no process
//!   group, so `TerminateProcess` reaches the one process — a descendant of a
//!   timed-out run can outlive it. Named in `CORE-POSIX.md`'s PTY row.
//! - **[`Pty::foreground_pgid`] answers `0` on Windows** — there is no
//!   foreground process group to read, and `0` is the same "no foreground
//!   command" [`Pty::foreground_pgid`]'s Unix arm reports at a bare prompt. A
//!   conducted shell's live cwd/command tick therefore reads idle there; the
//!   shell's own cwd is unavailable on this host too (`conduct::proc_cwd`'s
//!   own named refusal). A named degradation, never a silent one.
//! - **[`Pty::hung_up`] answers `None` on Windows.** The pseudo console — not
//!   the child — holds the output pipe's write end, and this process holds the
//!   pseudo console, so the pipe never breaks while the session lives. Whether
//!   a descendant is still attached to the console is not a fact any Windows
//!   call in this tree can ask for, so `ptyHeldAfterExit` is never stamped
//!   there rather than guessed from a process-table walk that cannot see
//!   "attached to THIS console" — and the seam says so by SHAPE (`Option`),
//!   because a bare `bool` is how `ptyHeldAfterExit: true` came to be stamped
//!   on every Windows run, a `cmd /C echo` with no descendants at all
//!   included.
//!
//! Everything else is the same promise with the host's own call, and the
//! `spawn_on_pty` contract — the env shaping, the "spawn FIRST so a failed
//! exec registers no ghost" ordering, the child inheriting nothing of the
//! parent's harness markers — is written ONCE in [`spawn_on_pty`]'s arms and
//! pinned by tests on both hosts.

use std::io;
use std::path::Path;

use super::conduct::TaskContext;

// ── the vocabulary both arms speak ───────────────────────────────────────

/// A terminal's geometry in characters. The pixel fields are Unix's own
/// (`TIOCGWINSZ` reports them); a pseudo console has no notion of them and
/// leaves them zero, so nothing that reads a [`WinSize`] may depend on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) struct WinSize {
    pub rows: u16,
    pub cols: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

impl WinSize {
    /// The conventional headless geometry (`80x24`), the same one
    /// `session_conduct`'s own fallback uses — and the shape a pseudo console
    /// is given when its caller has no console to measure, because a `0x0`
    /// pseudo console is not a console a TUI can open on.
    pub(in crate::graph) const fn conventional() -> WinSize {
        WinSize { rows: 24, cols: 80, xpixel: 0, ypixel: 0 }
    }
}

/// How a conducted child ended, in the one vocabulary both hosts can fill: an
/// exit code, a signal that killed it, or a status this process could not
/// read at all. `classify_end` turns this into the outcome the command
/// reports; nothing else reads it.
///
/// **The two hosts do not have the same variants, and that is the point**: a
/// signal is a host capability, so a host without signals cannot produce
/// [`Ended::Signal`] and does not have it — a match on this type is
/// exhaustive on each host with no arm that cannot be reached.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) enum Ended {
    Code(i32),
    Signal(i32),
    Unknown,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) enum Ended {
    Code(i32),
    Unknown,
}

/// A signal's conventional name, for the outcome vocabulary's `signal` field
/// (`"TERM"`, `"KILL"`, …). An unmapped number keeps its number rather than
/// guessing a name; the constants are `libc`'s, so the mapping cannot drift
/// from the numbers this host actually delivers.
#[cfg(unix)]
pub(in crate::graph) fn signal_name(signo: i32) -> String {
    match signo {
        libc::SIGHUP => "HUP",
        libc::SIGINT => "INT",
        libc::SIGQUIT => "QUIT",
        libc::SIGKILL => "KILL",
        libc::SIGTERM => "TERM",
        libc::SIGPIPE => "PIPE",
        libc::SIGALRM => "ALRM",
        libc::SIGSEGV => "SEGV",
        libc::SIGABRT => "ABRT",
        libc::SIGUSR1 => "USR1",
        libc::SIGUSR2 => "USR2",
        _ => return format!("signal {signo}"),
    }
    .to_string()
}

// ── the child ────────────────────────────────────────────────────────────

/// The child running on this seam's terminal. Same four questions on both
/// hosts: is it alive, what is its pid, how did it end, and end it.
#[cfg(unix)]
pub(in crate::graph) struct PtyChild {
    inner: std::process::Child,
    ended: Option<Ended>,
}

#[cfg(unix)]
impl PtyChild {
    /// This child's pid — the process-group leader `spawn_on_pty` made it.
    pub(in crate::graph) fn id(&self) -> i32 {
        self.inner.id() as i32
    }

    /// Has it exited? Non-blocking, and caching: the status `waitpid` reports
    /// here is the SAME one [`PtyChild::ended`] returns.
    pub(in crate::graph) fn exited(&mut self) -> bool {
        if self.ended.is_some() {
            return true;
        }
        match self.inner.try_wait() {
            Ok(Some(st)) => {
                self.ended = Some(ended_of_status(&st));
                true
            }
            _ => false,
        }
    }

    /// How it ended, blocking until that is known.
    pub(in crate::graph) fn ended(&mut self) -> Ended {
        if let Some(e) = self.ended {
            return e;
        }
        let st = self.inner.wait();
        let e = match st {
            Ok(st) => ended_of_status(&st),
            Err(_) => Ended::Unknown,
        };
        self.ended = Some(e);
        e
    }

    /// Wait up to `within` for it to end, or `None` if it did not.
    pub(in crate::graph) fn wait_for(&mut self, within: std::time::Duration) -> Option<Ended> {
        if let Some(e) = self.ended {
            return Some(e);
        }
        let give_up = std::time::Instant::now() + within;
        while std::time::Instant::now() < give_up {
            match self.inner.try_wait() {
                Ok(Some(st)) => {
                    let e = ended_of_status(&st);
                    self.ended = Some(e);
                    return Some(e);
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        None
    }

    /// End it. The deadline kill targets the DIRECT child's OWN PROCESS GROUP:
    /// `setsid` in `spawn_on_pty`'s `pre_exec` already made that child a group
    /// leader, so `killpg(child_pid)` reaches it and whatever job is still in
    /// ITS group (an `sh -c 'sleep 300 &'` background job dies with the
    /// group), and can never reach an unrelated session — the same "never a
    /// roster id, never an ancestry walk" guarantee `Child::kill` carries. A
    /// descendant that called `setsid` for itself left the session BY ITS OWN
    /// ACT and is deliberately NOT signalled: the wrapper cleans up what it
    /// started, and the wiki says exactly that rather than promising arbitrary
    /// descendant cleanup.
    pub(in crate::graph) fn kill(&mut self) {
        let pgid = self.inner.id() as i32;
        if unsafe { libc::killpg(pgid, libc::SIGKILL) } != 0 {
            let _ = self.inner.kill();
        }
    }
}

#[cfg(unix)]
fn ended_of_status(st: &std::process::ExitStatus) -> Ended {
    use std::os::unix::process::ExitStatusExt;
    match st.signal() {
        Some(signo) => Ended::Signal(signo),
        None => match st.code() {
            Some(code) => Ended::Code(code),
            None => Ended::Unknown,
        },
    }
}

/// The child running on the pseudo console.
///
/// Windows has no signals (`Ended::Signal` cannot occur), and no `ECHILD`:
/// the process handle this seam holds is the one `CreateProcessW` returned,
/// so a wait can be asked at any time, in any process.
#[cfg(windows)]
pub(in crate::graph) struct PtyChild {
    process: windows_sys::Win32::Foundation::HANDLE,
    pid: i32,
    ended: Option<Ended>,
}

#[cfg(windows)]
impl Drop for PtyChild {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.process) };
    }
}

#[cfg(windows)]
impl PtyChild {
    pub(in crate::graph) fn id(&self) -> i32 {
        self.pid
    }

    pub(in crate::graph) fn exited(&mut self) -> bool {
        if self.ended.is_some() {
            return true;
        }
        self.poll_exit().is_some()
    }

    pub(in crate::graph) fn ended(&mut self) -> Ended {
        if let Some(e) = self.ended {
            return e;
        }
        self.wait_for(std::time::Duration::from_secs(u32::MAX as u64)).unwrap_or(Ended::Unknown)
    }

    pub(in crate::graph) fn wait_for(&mut self, within: std::time::Duration) -> Option<Ended> {
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        if let Some(e) = self.ended {
            return Some(e);
        }
        let ms = within.as_millis().min(u32::MAX as u128) as u32;
        let waited = unsafe { WaitForSingleObject(self.process, ms) };
        if waited != windows_sys::Win32::Foundation::WAIT_OBJECT_0 {
            if waited == windows_sys::Win32::Foundation::WAIT_TIMEOUT {
                return None;
            }
            self.ended = Some(Ended::Unknown);
            return self.ended;
        }
        self.poll_exit()
    }

    /// `GetExitCodeProcess`: `STILL_ACTIVE` means "not finished", anything
    /// else IS the exit code. The one place a status is read, so `exited`,
    /// `ended` and `wait_for` can never disagree.
    fn poll_exit(&mut self) -> Option<Ended> {
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;
        let mut code: u32 = 0;
        if unsafe { GetExitCodeProcess(self.process, &mut code) } == 0 {
            return None;
        }
        if code == STILL_ACTIVE as u32 {
            return None;
        }
        let e = Ended::Code(code as i32);
        self.ended = Some(e);
        Some(e)
    }

    /// `TerminateProcess` — uncatchable, so a survivor is impossible, and the
    /// DIRECT process only: Windows has no process groups, so this arm reaches
    /// exactly what Unix's `killpg` reaches plus nothing else (named in the
    /// module table).
    pub(in crate::graph) fn kill(&mut self) {
        use windows_sys::Win32::System::Threading::TerminateProcess;
        unsafe { TerminateProcess(self.process, KILLED_EXIT_CODE) };
    }
}

/// The exit code this seam's own kill leaves behind — `1`, the conventional
/// "failed" code, so a `killedWith` report reads as the wrapper's act rather
/// than as something the child chose.
#[cfg(windows)]
const KILLED_EXIT_CODE: u32 = 1;

/// The window a reader keeps draining a pseudo console after its client
/// exited, when nothing is readable at that instant (`Pty::after_exit_settle`'s
/// own doc: conhost renders on its own pipeline, so the last frame can land
/// after the exit). Long enough for a renderer on a loaded box, short enough
/// that a finished run does not sit on it.
#[cfg(windows)]
const CONPTY_EXIT_SETTLE: std::time::Duration = std::time::Duration::from_millis(700);

// ── the terminal ─────────────────────────────────────────────────────────

/// The child's terminal, master side. Owned: every drop path closes it.
#[cfg(unix)]
pub(in crate::graph) struct Pty {
    master: std::os::unix::io::OwnedFd,
}

/// The child's pseudo console, plus the two handles this process speaks
/// through: `input` is the write end of the child's input channel and
/// `output` the read end of its output channel (Microsoft's own ConPTY
/// sample's naming: `inputWriteSide` / `outputReadSide`).
#[cfg(windows)]
pub(in crate::graph) struct Pty {
    hpc: windows_sys::Win32::System::Console::HPCON,
    input: windows_sys::Win32::Foundation::HANDLE,
    output: windows_sys::Win32::Foundation::HANDLE,
}

/// **The close ORDER is load-bearing, and it is not the declaration order.**
/// `ClosePseudoConsole` may emit a final frame into the output pipe, and a
/// console writing into a pipe whose read end is open with nobody reading it
/// is a DEADLOCK in a single-threaded caller — Microsoft's own warning on that
/// call. So this process's two ends are closed FIRST (any such write then
/// fails fast against a closed pipe instead of blocking a console that cannot
/// be closed until its write returns), and the console is closed last. The
/// drain that must precede all of this is the caller's
/// (`conduct_multiplex` returns only once the child is gone and its output was
/// read), which is why `Pty::close` is called after it, never before.
#[cfg(windows)]
impl Drop for Pty {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Console::ClosePseudoConsole;
        unsafe {
            CloseHandle(self.input);
            CloseHandle(self.output);
            ClosePseudoConsole(self.hpc);
        }
    }
}

#[cfg(unix)]
impl Pty {
    pub(in crate::graph) fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = unsafe {
                libc::read(
                    std::os::unix::io::AsRawFd::as_raw_fd(&self.master),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n > 0 {
                return Ok(n as usize);
            }
            if n == 0 {
                return Ok(0);
            }
            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EINTR) => continue,
                // A pty master whose slave side is gone reads EIO on Linux —
                // the same "nothing more will come" a 0-byte read means.
                Some(libc::EIO) => return Ok(0),
                _ => return Err(err),
            }
        }
    }

    pub(in crate::graph) fn write(&mut self, mut data: &[u8]) {
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&self.master);
        while !data.is_empty() {
            let n = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };
            if n <= 0 {
                if n < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            data = &data[n as usize..];
        }
    }

    pub(in crate::graph) fn resize(&mut self, ws: &WinSize) {
        let ws: libc::winsize = ws.into();
        unsafe {
            libc::ioctl(
                std::os::unix::io::AsRawFd::as_raw_fd(&self.master),
                libc::TIOCSWINSZ,
                &ws as *const libc::winsize,
            );
        }
    }

    pub(in crate::graph) fn has_output(&self) -> bool {
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&self.master);
        let mut fds = [libc::pollfd { fd, events: libc::POLLIN, revents: 0 }];
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 1, 0) };
        rc > 0 && (fds[0].revents & libc::POLLIN) != 0
    }

    /// Is every holder of the PTY slave gone? The master reports `POLLHUP`
    /// once the last slave fd closes and nothing else — which is exactly the
    /// "no descendant is holding the terminal" fact recorded when a child
    /// exits while a descendant still owns the slave. `Some` here: this host
    /// can answer the question.
    pub(in crate::graph) fn hung_up(&self) -> Option<bool> {
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&self.master);
        let mut fds = [libc::pollfd { fd, events: libc::POLLIN, revents: 0 }];
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 1, 0) };
        Some(rc > 0 && (fds[0].revents & libc::POLLHUP) != 0)
    }

    /// The foreground process group of the child's terminal: the shell's pid
    /// at a bare prompt, another pid while a command runs. A negative value
    /// when the question cannot be answered (a headless arm with no tty).
    pub(in crate::graph) fn foreground_pgid(&self) -> i32 {
        unsafe { libc::tcgetpgrp(std::os::unix::io::AsRawFd::as_raw_fd(&self.master)) }
    }

    /// How long a caller must keep draining after the child exited, when
    /// nothing is readable at this instant. `ZERO` here: a pty's buffered
    /// output is already in the master by the time `read` can see it, so
    /// "nothing readable now" IS "nothing more will come" — the settle window
    /// exists for the other arm's renderer pipeline, never for this one.
    pub(in crate::graph) fn after_exit_settle(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }

    /// Close the master. On Unix that is the drop; it exists so the caller
    /// reads the same line on both hosts, where on Windows it carries the
    /// `ClosePseudoConsole` ordering the module note spells out.
    pub(in crate::graph) fn close(self) {}
}

#[cfg(windows)]
impl Pty {
    /// What the pseudo console's output pipe says about itself, in one peek:
    /// whether there are bytes to read, and whether its writer has closed (the
    /// pipe is broken). Both questions come from the same call because a
    /// blocking `ReadFile` on a pipe that is only *signaled* — which a pipe's
    /// read handle is even with nothing in it — parks this process inside a
    /// console it then cannot drain: measured on ThinkChiyo as a
    /// `session_conduct` that never returned.
    fn output_state(&self) -> (bool, bool) {
        let mut available: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.output,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        (ok != 0 && available > 0, ok == 0)
    }

    pub(in crate::graph) fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (readable, closed) = self.output_state();
        if !readable && !closed {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the pseudo console has nothing to read",
            ));
        }
        let mut read: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReadFile(
                self.output,
                buf.as_mut_ptr() as *mut u8,
                buf.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            // The pseudo console's write end closing is this arm's EOF, the
            // same answer a 0-byte `read` gives on Unix.
            return match err.raw_os_error() {
                Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_HANDLE_EOF as i32 => {
                    Ok(0)
                }
                _ => Err(err),
            };
        }
        Ok(read as usize)
    }

    pub(in crate::graph) fn write(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let mut wrote: u32 = 0;
            let ok = unsafe {
                windows_sys::Win32::Storage::FileSystem::WriteFile(
                    self.input,
                    data.as_ptr() as *const u8,
                    data.len() as u32,
                    &mut wrote,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || wrote == 0 {
                break;
            }
            data = &data[wrote as usize..];
        }
    }

    /// `ResizePseudoConsole` — the same act `TIOCSWINSZ` performs, and just as
    /// best-effort: a resize the console refuses (a size of zero, a console
    /// already closing) is not a reason to tear a live session down, exactly
    /// as the Unix arm ignores its own `ioctl`'s return.
    pub(in crate::graph) fn resize(&mut self, ws: &WinSize) {
        unsafe {
            windows_sys::Win32::System::Console::ResizePseudoConsole(
                self.hpc,
                windows_sys::Win32::System::Console::COORD {
                    X: ws.cols as i16,
                    Y: ws.rows as i16,
                },
            );
        }
    }

    pub(in crate::graph) fn has_output(&self) -> bool {
        self.output_state().0
    }

    /// `None`, always, and by name: the pseudo console holds the output
    /// pipe's write end, so it never breaks while this process holds the
    /// console — and "a descendant is still attached" is not a question any
    /// call in this tree can ask (module note, "Where the arms genuinely
    /// differ"). `None` is what the caller needs: a host that cannot answer
    /// must not have its `false` stamped as a fact, which is what a bare
    /// `bool` did (`ptyHeldAfterExit: true` on every Windows run, including a
    /// `cmd /C echo` with no descendants at all).
    pub(in crate::graph) fn hung_up(&self) -> Option<bool> {
        None
    }

    /// `0` — no foreground process group exists on this host (module note).
    pub(in crate::graph) fn foreground_pgid(&self) -> i32 {
        0
    }

    /// How long a caller must keep draining after the client exited, when
    /// nothing is readable at this instant — **and this arm needs it, measured
    /// on ThinkChiyo**: a pseudo console renders on its own pipeline, so a
    /// just-exited client's last frame can land a beat after its exit, and a
    /// reader that stops at "nothing readable right now" loses the tail of
    /// every run (the first shape of this seam read an empty buffer from a
    /// `cmd /C echo` that had already printed). A pty needs no such window
    /// (`after_exit_settle`'s Unix arm), which is why the number lives in the
    /// seam rather than in the loop.
    pub(in crate::graph) fn after_exit_settle(&self) -> std::time::Duration {
        CONPTY_EXIT_SETTLE
    }

    pub(in crate::graph) fn close(self) {}
}

#[cfg(unix)]
impl From<&WinSize> for libc::winsize {
    fn from(ws: &WinSize) -> libc::winsize {
        libc::winsize {
            ws_row: ws.rows,
            ws_col: ws.cols,
            ws_xpixel: ws.xpixel,
            ws_ypixel: ws.ypixel,
        }
    }
}

#[cfg(unix)]
impl From<libc::winsize> for WinSize {
    fn from(ws: libc::winsize) -> WinSize {
        WinSize { rows: ws.ws_row, cols: ws.ws_col, xpixel: ws.ws_xpixel, ypixel: ws.ws_ypixel }
    }
}

/// Open a terminal and spawn `program args` on it, returning the (reapable)
/// child plus the master side.
///
/// The child's environment is shaped HERE on both hosts, once: `AOIDE_SESSION_ID`
/// is set, the parent harness's own session markers are dropped
/// ([`super::conduct::scrub_session_markers`]'s list — the child must never
/// believe it is running inside its parent's harness session), and a managed
/// task's own two variables are exported.
///
/// The child is a session leader (Unix) / a pseudo-console client (Windows),
/// and the caller owns the order: spawn FIRST, so a failed exec registers no
/// ghost session.
#[cfg(unix)]
pub(in crate::graph) fn spawn_on_pty(
    program: &str,
    args: &[String],
    session_id: &str,
    ws: Option<WinSize>,
    task: Option<&TaskContext>,
) -> io::Result<(PtyChild, Pty)> {
    use std::os::unix::io::{FromRawFd, OwnedFd, RawFd};
    use std::os::unix::process::CommandExt;

    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let ws: Option<libc::winsize> = ws.as_ref().map(Into::into);
    let wsp = ws
        .as_ref()
        .map(|w| w as *const libc::winsize)
        .unwrap_or(std::ptr::null());
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            wsp,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // Own the master at once: it is now closed on any early return / on drop.
    let master_owned = unsafe { OwnedFd::from_raw_fd(master) };

    let slave_fd = slave;
    let master_fd = master;
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .env(super::conduct::CHILD_SESSION_ENV, session_id);
    super::conduct::scrub_session_markers(&mut cmd);
    if let Some(task) = task {
        cmd.env(super::conduct::CHILD_TASK_ENV, &task.slug);
        if let Some(path) = &task.instructions_path {
            cmd.env(super::conduct::CHILD_TASK_INSTRUCTIONS_ENV, path);
        }
    }
    // The child's `pre_exec` ordering is load-bearing and each step is a raw
    // libc call (async-signal-safe): `setsid()` starts a new session with NO
    // controlling tty; `ioctl(slave, TIOCSCTTY)` then acquires the slave as
    // this session's ctty (only a session leader without a ctty may do this —
    // hence setsid FIRST); the slave is dup'd over fds 0/1/2 so the child's
    // std streams ARE the pty; and the master + spare slave fd are closed in
    // the child. All of this precedes exec.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(slave_fd, libc::TIOCSCTTY, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            for target in 0..3 {
                if libc::dup2(slave_fd, target) == -1 {
                    return Err(io::Error::last_os_error());
                }
            }
            libc::close(master_fd);
            if slave_fd > 2 {
                libc::close(slave_fd);
            }
            Ok(())
        });
    }
    let spawned = cmd.spawn();
    // The parent never speaks on the slave — close it whatever spawn returned.
    unsafe {
        libc::close(slave);
    }
    let child = spawned?;
    Ok((
        PtyChild { inner: child, ended: None },
        Pty { master: master_owned },
    ))
}

#[cfg(windows)]
pub(in crate::graph) fn spawn_on_pty(
    program: &str,
    args: &[String],
    session_id: &str,
    ws: Option<WinSize>,
    task: Option<&TaskContext>,
) -> io::Result<(PtyChild, Pty)> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
        UpdateProcThreadAttribute, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
        STARTF_USESTDHANDLES, STARTUPINFOEXW,
    };

    // The child's input channel (we keep the write end, it gets the read end)
    // and its output channel (it gets the write end, we keep the read end).
    // The FIRST pair is owned as soon as it exists, so a failure of the second
    // `CreatePipe` closes it rather than leaking two handles.
    let mut input_read: HANDLE = std::ptr::null_mut();
    let mut input_write: HANDLE = std::ptr::null_mut();
    if unsafe { CreatePipe(&mut input_read, &mut input_write, std::ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let console_read = OwnedHandle(input_read);
    let input = OwnedHandle(input_write);
    let mut output_read: HANDLE = std::ptr::null_mut();
    let mut output_write: HANDLE = std::ptr::null_mut();
    if unsafe { CreatePipe(&mut output_read, &mut output_write, std::ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let output = OwnedHandle(output_read);
    // The console's own two ends are closed with it, on every path out of this
    // function (after the spawn, which is where Microsoft's sample closes
    // them).
    let console_write = OwnedHandle(output_write);

    let size = ws.unwrap_or_else(WinSize::conventional);
    let mut hpc: windows_sys::Win32::System::Console::HPCON = 0;
    let hr = unsafe {
        windows_sys::Win32::System::Console::CreatePseudoConsole(
            windows_sys::Win32::System::Console::COORD { X: size.cols as i16, Y: size.rows as i16 },
            console_read.0,
            console_write.0,
            0,
            &mut hpc,
        )
    };
    if hr != 0 {
        // The console's own two ends are owned handles: they close on this
        // return without a line of their own.
        return Err(io::Error::from_raw_os_error(hr));
    }
    // The pseudo console owns those two ends now. Microsoft's own sample
    // closes its copies AFTER `CreateProcessW` (the console keeps its own
    // duplicate), so they stay open across the spawn below and are closed
    // with it.
    let hpc = Hpcon(hpc);

    // STARTUPINFOEX + the pseudo console attribute: the attribute's VALUE is
    // the HPCON itself, passed AS the pointer — Microsoft's own sample hands
    // `hpc` (not `&hpc`) to `UpdateProcThreadAttribute`, i.e. the console
    // handle is this attribute's value rather than the address of one, and
    // the OS reads it that way. Passing the address of the handle instead is
    // accepted by the call and leaves the child unable to attach to its
    // console: measured on ThinkChiyo as `STATUS_DLL_INIT_FAILED`
    // (`0xC0000142`) and not one byte of output.
    //
    // **The shape this arm ships, and what it buys.** `STARTF_USESTDHANDLES`
    // with all three handles NULL, and `bInheritHandles = FALSE`: the shape
    // shipping ConPTY implementations use, and the one the runs on ThinkChiyo
    // prove. The pseudo console attaches the client to `CONIN$`/`CONOUT$`, so
    // the child's own stdout IS a console — `[Console]::IsOutputRedirected`
    // answers `False` and `mode con` answers its geometry, which is what
    // `the_childs_own_stdout_is_a_console_not_this_processs_pipe`,
    // `a_resize_reaches_the_childs_own_console` and the four channel runs
    // assert — while nothing of this process's goes to the child (no handle
    // inheritance at all: the parent's streams are never duplicated into it,
    // and its stdin is not the pipe conhost itself reads).
    //
    // Two neighbouring shapes are NOT this one, and the difference is worth
    // keeping here because both look plausible: leaving
    // `STARTF_USESTDHANDLES` off entirely was measured losing the child's
    // output — the child runs and exits 0 while nothing but conhost's own
    // startup sequence reaches the output pipe — and naming the console's OWN
    // two pipe ends (with the inheritance that needs) was measured, by the
    // independent re-review, delivering no child output at all (the child
    // exiting 1).
    //
    // The sizing call below FAILS by design (it reports
    // `ERROR_INSUFFICIENT_BUFFER` while filling in the size), so its return is
    // ignored and only the size it leaves behind is used — treating that
    // failure as an error was a bug this arm carried for one round.
    let mut attrsize: usize = 0;
    unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attrsize) };
    if attrsize == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the pseudo console attribute list has no size on this host",
        ));
    }
    let mut attr_buf: Vec<u64> = vec![0; attrsize.div_ceil(8)];
    let attr_list = attr_buf.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = std::ptr::null_mut();
    si.StartupInfo.hStdOutput = std::ptr::null_mut();
    si.StartupInfo.hStdError = std::ptr::null_mut();
    si.lpAttributeList = attr_list;
    // `attr_list` is initialized exactly once, and only a list that WAS
    // initialized is deleted (the sizing call above is a probe that fails by
    // design, so `attrsize` is the only thing it leaves behind).
    let initialized = unsafe { InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attrsize) };
    let mut ok = initialized;
    if ok != 0 {
        ok = unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                hpc.0 as *const core::ffi::c_void,
                std::mem::size_of::<windows_sys::Win32::System::Console::HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
    }
    if ok == 0 {
        let err = io::Error::last_os_error();
        if initialized != 0 {
            unsafe { DeleteProcThreadAttributeList(attr_list) };
        }
        return Err(err);
    }

    // A pseudo console has no "unset" size: when the caller has no console to
    // measure, it gets the conventional 80x24 the headless fallback uses,
    // because a 0x0 pseudo console is not one a TUI can open on.
    let mut cmdline = command_line(program, args);
    let env = environment_block(session_id, task);
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let spawned = unsafe {
        CreateProcessW(
            std::ptr::null(),
            cmdline.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0, // bInheritHandles: nothing of this process's goes to the child.
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            env.as_ptr() as *const core::ffi::c_void,
            std::ptr::null(),
            &si.StartupInfo,
            &mut pi,
        )
    };
    unsafe { DeleteProcThreadAttributeList(attr_list) };
    if spawned == 0 {
        return Err(io::Error::last_os_error());
    }
    // The spawn happened: the thread handle goes with it, and the pseudo
    // console's own pipe ends are closed here (the sample's own lifetime rule
    // — "close these after CreateProcess of the child application with
    // pseudoconsole object"). Both are owned handles, so every earlier return
    // closed them already.
    unsafe { CloseHandle(pi.hThread) };
    drop(console_read);
    drop(console_write);
    Ok((
        PtyChild { process: pi.hProcess, pid: pi.dwProcessId as i32, ended: None },
        Pty { hpc: hpc.into_raw(), input: input.into_raw(), output: output.into_raw() },
    ))
}

#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};

// ── the real console (stdin/stdout), and its resize source ───────────────

/// The terminal THIS process is attached to — the one whose keys are typed
/// into the child and whose size the child's terminal is kept at. Inert by
/// construction when there is none (a headless conduct, a test, a pipe for
/// stdin): every method answers honestly instead of pretending.
#[cfg(unix)]
pub(in crate::graph) struct Console {
    fd: std::os::unix::io::RawFd,
    saved: libc::termios,
    active: bool,
}

#[cfg(unix)]
impl Console {
    /// Attach to this process's own stdin, raw-mode it so keystrokes reach the
    /// child unbuffered and unechoed, and arm the resize latch.
    /// `headless` skips both: a headless conduct has no controlling tty to
    /// raw-mode or resize.
    pub(in crate::graph) fn attach(headless: bool) -> Console {
        let fd = libc::STDIN_FILENO;
        if headless {
            return Console { fd, saved: unsafe { std::mem::zeroed() }, active: false };
        }
        install_winch_handler();
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::isatty(fd) } != 1 || unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Console { fd, saved, active: false };
        }
        let mut raw = saved;
        unsafe {
            libc::cfmakeraw(&mut raw);
            libc::tcsetattr(fd, libc::TCSANOW, &raw);
        }
        Console { fd, saved, active: true }
    }

    /// The real terminal's geometry, or `None` when there is no terminal.
    pub(in crate::graph) fn size() -> Option<WinSize> {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut ws as *mut libc::winsize)
        };
        if rc == 0 && (ws.ws_row != 0 || ws.ws_col != 0) {
            Some(ws.into())
        } else {
            None
        }
    }

    /// A pending resize, consumed. The `SIGWINCH` handler only flips a flag
    /// (async-signal-safe); the loop services it here by re-reading the real
    /// tty size.
    pub(in crate::graph) fn resized(&mut self) -> Option<WinSize> {
        use std::sync::atomic::Ordering;
        if !WINCH.swap(false, Ordering::SeqCst) {
            return None;
        }
        Console::size()
    }

    pub(in crate::graph) fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = unsafe {
                libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n >= 0 {
                return Ok(n as usize);
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(err);
        }
    }

    /// Restore the terminal to exactly what it was. Runs on normal return AND
    /// on unwind (panic=unwind), so no exit path can leave a wedged terminal.
    pub(in crate::graph) fn restore(&mut self) {
        if self.active {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved);
            }
            self.active = false;
        }
    }
}

#[cfg(unix)]
impl Drop for Console {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(unix)]
static WINCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_winch(_sig: libc::c_int) {
    WINCH.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Install the SIGWINCH handler WITHOUT `SA_RESTART`, so a resize interrupts
/// `poll()` (returns `EINTR`) and the loop can propagate the new size promptly.
#[cfg(unix)]
fn install_winch_handler() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_winch as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
    }
}

/// The console this process is attached to, raw-moded.
///
/// Raw mode here is `SetConsoleMode`, and each bit it clears is the same
/// promise `cfmakeraw` makes on Unix: `ENABLE_ECHO_INPUT` off (the child's own
/// terminal echoes, never this process's console), `ENABLE_LINE_INPUT` off
/// (keystrokes reach the child as they are typed, not at a newline),
/// `ENABLE_PROCESSED_INPUT` off (Ctrl-C arrives as the byte `0x03` for the
/// child's console to interpret, instead of killing THIS process), and
/// `ENABLE_VIRTUAL_TERMINAL_INPUT` on so keys arrive as the VT sequences the
/// child expects. The output handle gets `ENABLE_VIRTUAL_TERMINAL_PROCESSING`
/// so the pseudo console's own VT output renders rather than printing escapes.
#[cfg(windows)]
pub(in crate::graph) struct Console {
    stdin: windows_sys::Win32::Foundation::HANDLE,
    /// What this process's stdin IS, asked once at attach — the question the
    /// wait needs answered before it offers the handle to
    /// `WaitForMultipleObjects`. A CONSOLE signals only when a key arrives; a
    /// PIPE's read handle is signaled even with nothing in it, so waiting on it
    /// would win every race and strand the loop in a blocking read (the same
    /// trap the output pipe had); a file, `NUL` or an absent handle signals
    /// immediately and returns EOF, so it is read once and dropped.
    stdin_kind: StdinKind,
    saved: Vec<(windows_sys::Win32::Foundation::HANDLE, u32)>,
    last: Option<WinSize>,
    polled_at: Option<std::time::Instant>,
}

/// What this process's stdin is, as far as the loop cares: the thing it can
/// WAIT on (a console), the thing it must POLL (a pipe), or the thing it reads
/// once and drops (a file, `NUL`, no handle at all).
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StdinKind {
    Console,
    Pipe,
    Other,
}

#[cfg(windows)]
impl Console {
    /// May this console's stdin handle be offered to the wait? **Only a
    /// console's may**: a console signals when a key arrives, while a pipe's
    /// read handle is signalled even with nothing in it — offering it to
    /// `WaitForMultipleObjects` would win every race and leave the inbox
    /// unreportable, the same trap the output pipe had. A non-console stdin is
    /// therefore POLLED exactly as that pipe is (`stdin_polled_ready`), and read
    /// without ever blocking (`read`), which is what Unix's `poll` gives a pipe
    /// for free (an empty pipe is simply not `POLLIN` there).
    pub(in crate::graph) fn stdin_is_waitable(&self) -> bool {
        self.stdin_kind == StdinKind::Console
    }

    /// Is there anything readable on a POLLED stdin? **Everything that is not a
    /// console is polled**, and the read below is what learns whether there are
    /// bytes, whether there are none yet (`WouldBlock`, which the loop ignores)
    /// or whether the writer has closed (`Ok(0)`, which the loop treats exactly
    /// as the Unix arm treats a 0-byte `read` — "our own stdin closed; keep
    /// bridging the rest"). Polling unconditionally is what makes the answer
    /// impossible to get wrong: nothing here can block.
    pub(in crate::graph) fn stdin_polled_ready(&self) -> bool {
        self.stdin_kind != StdinKind::Console
    }

    /// How many bytes a PIPE stdin has ready (`None` when this handle is not a
    /// pipe at all — `PeekNamedPipe` refusing, which is what a regular file or
    /// `NUL` answers), plus whether its writer has closed. One call, so the
    /// read below cannot race the answer.
    fn pipe_state(&self) -> (Option<u32>, bool) {
        use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
        let mut available: u32 = 0;
        let peeked = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.stdin,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if peeked != 0 {
            return (Some(available), false);
        }
        let closed = io::Error::last_os_error().raw_os_error() == Some(ERROR_BROKEN_PIPE as i32);
        (None, closed)
    }

    pub(in crate::graph) fn attach(headless: bool) -> Console {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
            ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT,
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        let stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut stdin_kind = StdinKind::Other;
        let is_a_console = unsafe { GetConsoleMode(stdin, &mut 0u32) } != 0;
        if is_a_console {
            stdin_kind = StdinKind::Console;
        } else if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetFileType(stdin)
        } == windows_sys::Win32::Storage::FileSystem::FILE_TYPE_PIPE
        {
            stdin_kind = StdinKind::Pipe;
        }
        let mut console = Console {
            stdin,
            stdin_kind,
            saved: Vec::new(),
            last: None,
            polled_at: None,
        };
        if headless {
            return console;
        }
        // A stdin that is not a console is FORWARDED like any other (see
        // `stdin_polled_ready` and `read` below): a pipe is polled and read by
        // the byte, a file or `NUL` is read directly, and both end in the same
        // `Ok(0)` the Unix arm's 0-byte read gives. There is no kind left that
        // cannot be forwarded, so nothing is warned about here — the warning
        // this arm briefly carried is gone with the fix it stood in for.
        let mut mode: u32 = 0;
        if unsafe { GetConsoleMode(stdin, &mut mode) } != 0 {
            let raw = (mode & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
                | ENABLE_VIRTUAL_TERMINAL_INPUT;
            if unsafe { SetConsoleMode(stdin, raw) } != 0 {
                console.saved.push((stdin, mode));
            }
        }
        let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if unsafe { GetConsoleMode(stdout, &mut mode) } != 0 {
            if unsafe { SetConsoleMode(stdout, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) } != 0 {
                console.saved.push((stdout, mode));
            }
        }
        console.last = Console::size();
        console
    }

    /// The console screen buffer's VISIBLE window, in characters — never its
    /// buffer size, which on a scrolled-back console is the whole scrollback
    /// and would open every child at hundreds of rows.
    pub(in crate::graph) fn size() -> Option<WinSize> {
        use windows_sys::Win32::System::Console::{
            GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO,
            STD_OUTPUT_HANDLE,
        };
        let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        if unsafe { GetConsoleScreenBufferInfo(stdout, &mut info) } == 0 {
            return None;
        }
        let cols = info.srWindow.Right - info.srWindow.Left + 1;
        let rows = info.srWindow.Bottom - info.srWindow.Top + 1;
        if cols <= 0 || rows <= 0 {
            return None;
        }
        Some(WinSize { rows: rows as u16, cols: cols as u16, xpixel: 0, ypixel: 0 })
    }

    /// A resize, noticed rather than delivered: this host has no `SIGWINCH`,
    /// so the loop's own tick asks the console how big it is now and pushes
    /// the answer to the child through `ResizePseudoConsole`.
    ///
    /// The question costs a syscall and the loop's poll is ~25 ms
    /// ([`CONPTY_POLL_MS`]), so it is asked at most every
    /// [`CONSOLE_SIZE_POLL`] — a console cannot be resized by hand faster than
    /// that, and this way the poll rate costs nothing.
    pub(in crate::graph) fn resized(&mut self) -> Option<WinSize> {
        if self
            .polled_at
            .is_some_and(|at| at.elapsed() < CONSOLE_SIZE_POLL)
        {
            return None;
        }
        self.polled_at = Some(std::time::Instant::now());
        let now = Console::size()?;
        if self.last == Some(now) {
            return None;
        }
        self.last = Some(now);
        Some(now)
    }

    /// Read this process's own stdin without ever blocking on it: a pipe is
    /// asked how many bytes it has (`pipe_state`) and exactly those are read, a
    /// handle that is not a pipe at all (a regular file, `NUL`) is read
    /// directly — a file read returns data and then EOF, and `NUL` answers EOF
    /// immediately, which is what Unix does with a redirected file too. A pipe
    /// with nothing in it answers `WouldBlock` (the loop's "nothing to service
    /// yet"), and a pipe whose writer has closed answers `Ok(0)` — the same
    /// answer, and the same meaning, the Unix arm's 0-byte `read` gives.
    pub(in crate::graph) fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};
        // **The arm is chosen by the kind `attach` already asked for, never by
        // a fresh question to the OS.** `PeekNamedPipe` is a PIPE's call: on a
        // console handle its answer is undocumented (a successful peek with
        // nothing "available" would answer `WouldBlock` here for ever and drop
        // every keystroke of the flagship interactive case), so the console
        // path never calls it. `Other` is a file or `NUL`: a read there returns
        // data and then EOF and cannot block, exactly what Unix does with a
        // redirected file.
        let want = match self.stdin_kind {
            StdinKind::Console => buf.len(),
            StdinKind::Other => buf.len(),
            StdinKind::Pipe => {
                let (available, closed) = self.pipe_state();
                if closed {
                    // A CLOSED writer is not an empty pipe: whatever it wrote
                    // is still in there, and `ReadFile` returns those bytes
                    // before it answers the broken pipe. Believing the close
                    // instead of draining first dropped a producer's last line
                    // — measured on ThinkChiyo, where `PeekNamedPipe` reports
                    // the broken pipe rather than the remaining count once the
                    // write end is gone.
                    buf.len()
                } else {
                    match available {
                        Some(0) => {
                            return Err(io::Error::new(
                                io::ErrorKind::WouldBlock,
                                "no piped stdin bytes ready yet",
                            ))
                        }
                        // Exactly what the pipe HAS: a read can then never wait
                        // for more.
                        Some(n) => (n as usize).min(buf.len()),
                        None => buf.len(),
                    }
                }
            }
        };
        let mut read: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReadFile(
                self.stdin,
                buf.as_mut_ptr() as *mut u8,
                want as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_HANDLE_EOF as i32 => {
                    Ok(0)
                }
                _ => Err(err),
            };
        }
        Ok(read as usize)
    }

    pub(in crate::graph) fn restore(&mut self) {
        use windows_sys::Win32::System::Console::SetConsoleMode;
        for (handle, mode) in self.saved.drain(..) {
            unsafe { SetConsoleMode(handle, mode) };
        }
    }
}

#[cfg(windows)]
impl Drop for Console {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Write this process's own stdout, unbuffered, retrying a short write. The
/// seam's answer to "where does the interactive pump's output go", and it must
/// stay a RAW write on both arms: `std::io::stdout()` is a `LineWriter`, which
/// holds a chunk back until its last newline (or its ~1 KiB buffer fills), and
/// a TUI's output is newline-free escape traffic — the interactive pump is
/// latency-sensitive by design. Unix: `libc::write` on fd 1 (the shape this
/// seam moved here). Windows: `WriteFile` on the stdout handle. Both are
/// best-effort by the same rule the log side keeps: a write error stops
/// mirroring, never tears the session down.
#[cfg(unix)]
pub(in crate::graph) fn write_stdout(mut data: &[u8]) {
    while !data.is_empty() {
        let n = unsafe {
            libc::write(libc::STDOUT_FILENO, data.as_ptr() as *const libc::c_void, data.len())
        };
        if n <= 0 {
            if n < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        data = &data[n as usize..];
    }
}

#[cfg(windows)]
pub(in crate::graph) fn write_stdout(mut data: &[u8]) {
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    while !data.is_empty() {
        let mut wrote: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::WriteFile(
                handle,
                data.as_ptr(),
                data.len() as u32,
                &mut wrote,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || wrote == 0 {
            break;
        }
        data = &data[wrote as usize..];
    }
}

// ── the injection inbox ──────────────────────────────────────────────────

/// The per-session injection socket's receiving end: the connections whose
/// bytes are TYPED into the child, exactly as this process's own keyboard is.
///
/// The self-injection gate lives here because the accept does: a connector
/// whose OWN nearest live registered session is the session this socket
/// belongs to is a true self-injection (a session typing into itself), and is
/// dropped unforwarded. Both a `peer_cred` failure and an unresolvable
/// connector fail OPEN (allowed) — this guard only ever refuses the one
/// narrow, known shape it exists to catch.
#[cfg(unix)]
pub(in crate::graph) struct Inbox {
    listener: std::os::unix::net::UnixListener,
    conns: Vec<std::os::unix::net::UnixStream>,
    id: String,
}

#[cfg(unix)]
impl Inbox {
    pub(in crate::graph) fn bind(path: &Path, id: &str) -> io::Result<Inbox> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(path); // clear a stale socket from a prior crash.
        let listener = std::os::unix::net::UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Inbox { listener, conns: Vec::new(), id: id.to_string() })
    }

    pub(in crate::graph) fn handles(&self) -> Vec<std::os::unix::io::RawFd> {
        use std::os::unix::io::AsRawFd;
        let mut fds = vec![self.listener.as_raw_fd()];
        fds.extend(self.conns.iter().map(|c| c.as_raw_fd()));
        fds
    }

    /// Every byte an injection connection has delivered since the last call,
    /// one chunk per connection read. Accepts first, so a connection that
    /// arrived and wrote before this call is never missed.
    pub(in crate::graph) fn drain(&mut self) -> Vec<Vec<u8>> {
        use std::io::Read as _;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if self.conns.len() >= MAX_INJECTION_CONNS {
                        continue; // a bound, not a queue (module note).
                    }
                    let self_injection = super::identity::peer_cred(&stream)
                        .map(|cred| refused_self_injection(cred.pid, &self.id))
                        .unwrap_or(false);
                    if self_injection {
                        continue; // dropped outright — never accepted into the set.
                    }
                    let _ = stream.set_nonblocking(true);
                    self.conns.push(stream);
                }
                Err(_) => break, // EAGAIN — no more pending.
            }
        }
        let mut chunks = Vec::new();
        let mut buf = [0u8; 8192];
        let mut still = Vec::new();
        for mut conn in std::mem::take(&mut self.conns) {
            if !readable_now(std::os::unix::io::AsRawFd::as_raw_fd(&conn)) {
                still.push(conn);
                continue;
            }
            loop {
                match conn.read(&mut buf) {
                    Ok(0) => break, // EOF — this injection is done.
                    Ok(n) => chunks.push(buf[..n].to_vec()),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        still.push(conn);
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        self.conns = still;
        chunks
    }
}

/// A bound on live injection connections, not a queue: conduct is not a
/// server, and every accepted connection is one more source the wait must
/// watch (1 stdin + 1 listener + these ≤ `MAXIMUM_WAIT_OBJECTS`). Beyond it a
/// connection is closed on accept rather than kept and ignored — an ignored
/// connection would look accepted to `aoide send` and deliver nothing. ONE
/// constant for both arms: it is one policy, and this file's whole claim is one
/// concept written twice.
const MAX_INJECTION_CONNS: usize = 32;

#[cfg(unix)]
fn readable_now(fd: std::os::unix::io::RawFd) -> bool {
    let mut fds = [libc::pollfd { fd, events: libc::POLLIN, revents: 0 }];
    let rc = unsafe { libc::poll(fds.as_mut_ptr(), 1, 0) };
    rc > 0
}

/// The injection socket's receiving end on native Windows: the same
/// `AF_UNIX` socket (`aoide_protocol::win_unix`), waited on with the one
/// primitive this host has for a socket — `WSAEventSelect`, which signals an
/// event handle the loop's own `WaitForMultipleObjects` can watch. Same
/// bound, same gate, same bytes out.
#[cfg(windows)]
pub(in crate::graph) struct Inbox {
    listener: aoide_protocol::win_unix::UnixListener,
    listener_event: WsaEvent,
    conns: Vec<(aoide_protocol::win_unix::UnixStream, WsaEvent)>,
    id: String,
}

#[cfg(windows)]
impl Inbox {
    pub(in crate::graph) fn bind(path: &Path, id: &str) -> io::Result<Inbox> {
        use windows_sys::Win32::Networking::WinSock::{WSAEventSelect, FD_ACCEPT};
        // The socket file's OWN policy cannot be read on this host (an AF_UNIX
        // socket is a reparse point), so the PARENT directory is the only gate
        // — and a native socket binder creates it through
        // `owner_only::ensure_private_dir` for exactly that reason
        // (`storage::runtime_dir`'s and `win_unix`'s own contract: an elevated
        // token's default owner is `BUILTIN\Administrators`, and the fallback
        // runtime dir is not per-user on every host). This is the ONE binder
        // that did not, which made a live agent's stdin reachable by any local
        // user who could write the socket file.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            aoide_protocol::owner_only::ensure_private_dir(parent)?;
        }
        let _ = std::fs::remove_file(path); // clear a stale socket from a prior crash.
        let listener = aoide_protocol::win_unix::UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        let listener_event = WsaEvent::new()?;
        if unsafe {
            WSAEventSelect(listener.as_raw_socket(), listener_event.0, FD_ACCEPT as i32)
        } == SOCKET_ERROR
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Inbox { listener, listener_event, conns: Vec::new(), id: id.to_string() })
    }

    pub(in crate::graph) fn handles(&self) -> Vec<windows_sys::Win32::Foundation::HANDLE> {
        // A `WSAEVENT` is an `isize`, not a `HANDLE` — the same kernel object
        // under two names, and the wait needs them spelled the second way.
        use windows_sys::Win32::Foundation::HANDLE;
        let mut handles = vec![self.listener_event.0 as HANDLE];
        handles.extend(self.conns.iter().map(|(_, e)| e.0 as HANDLE));
        handles
    }

    pub(in crate::graph) fn drain(&mut self) -> Vec<Vec<u8>> {
        use windows_sys::Win32::Networking::WinSock::{
            recv, WSAEventSelect, FD_ACCEPT, FD_CLOSE, FD_READ, WSAEINTR, WSAEWOULDBLOCK,
        };
        let mut chunks = Vec::new();
        let mut buf = [0u8; 8192];

        if network_event(self.listener.as_raw_socket(), self.listener_event.0) & FD_ACCEPT as i32
            != 0
        {
            loop {
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if self.conns.len() >= MAX_INJECTION_CONNS {
                            continue; // the same bound the Unix arm keeps.
                        }
                        let self_injection = super::identity::peer_cred(&stream)
                            .map(|cred| refused_self_injection(cred.pid, &self.id))
                            .unwrap_or(false);
                        if self_injection {
                            continue;
                        }
                        let event = match WsaEvent::new() {
                            Ok(e) => e,
                            Err(_) => continue,
                        };
                        if unsafe {
                            WSAEventSelect(
                                stream.as_raw_socket(),
                                event.0,
                                (FD_READ | FD_CLOSE) as i32,
                            )
                        } == SOCKET_ERROR
                        {
                            continue;
                        }
                        self.conns.push((stream, event));
                    }
                    Err(_) => break, // no more pending.
                }
            }
        }

        let mut still = Vec::new();
        for (stream, event) in std::mem::take(&mut self.conns) {
            let events = network_event(stream.as_raw_socket(), event.0);
            if events & (FD_READ | FD_CLOSE) as i32 == 0 {
                still.push((stream, event));
                continue;
            }
            let mut done = false;
            loop {
                let n = unsafe {
                    recv(
                        stream.as_raw_socket(),
                        buf.as_mut_ptr(),
                        buf.len() as i32,
                        0,
                    )
                };
                if n > 0 {
                    chunks.push(buf[..n as usize].to_vec());
                    continue;
                }
                if n == 0 {
                    done = true; // EOF — this injection is done.
                } else {
                    let err = io::Error::last_os_error();
                    match err.raw_os_error() {
                        Some(code) if code == WSAEWOULDBLOCK as i32 => {}
                        Some(code) if code == WSAEINTR as i32 => continue,
                        _ => done = true,
                    }
                }
                break;
            }
            if !done {
                still.push((stream, event));
            }
        }
        self.conns = still;
        chunks
    }
}

#[cfg(windows)]
const SOCKET_ERROR: i32 = -1;

/// The network events pending on `socket`, resetting the association's event
/// on the way out — `WSAEnumNetworkEvents`' documented contract, and the only
/// way to ask "was it readable, and was it closed".
#[cfg(windows)]
fn network_event(
    socket: windows_sys::Win32::Networking::WinSock::SOCKET,
    event: windows_sys::Win32::Networking::WinSock::WSAEVENT,
) -> i32 {
    use windows_sys::Win32::Networking::WinSock::{WSAEnumNetworkEvents, WSANETWORKEVENTS};
    let mut events: WSANETWORKEVENTS = unsafe { std::mem::zeroed() };
    if unsafe { WSAEnumNetworkEvents(socket, event, &mut events) } == SOCKET_ERROR {
        return 0;
    }
    events.lNetworkEvents
}

/// A Winsock event handle, closed on drop.
#[cfg(windows)]
struct WsaEvent(windows_sys::Win32::Networking::WinSock::WSAEVENT);

#[cfg(windows)]
impl WsaEvent {
    fn new() -> io::Result<WsaEvent> {
        use windows_sys::Win32::Networking::WinSock::{WSACreateEvent, WSA_INVALID_EVENT};
        let event = unsafe { WSACreateEvent() };
        if event == WSA_INVALID_EVENT {
            return Err(io::Error::last_os_error());
        }
        Ok(WsaEvent(event))
    }
}

#[cfg(windows)]
impl Drop for WsaEvent {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Networking::WinSock::WSACloseEvent(self.0) };
    }
}

/// Does this connecting pid's OWN nearest live registered session resolve to
/// the session this socket belongs to? A pure decision over the staged roster
/// (`identity::is_self_originated`'s own doc has the walk), kept here because
/// the accept is here on both hosts — one gate, called from one place per arm.
#[cfg(any(unix, windows))]
fn refused_self_injection(pid: i32, id: &str) -> bool {
    use super::model::{load_stage, sessions_path, SessionsFile};
    let sessions = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    super::identity::is_self_originated(pid, &sessions, id)
}

// ── the wait ─────────────────────────────────────────────────────────────

/// Which input the loop may service now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::graph) struct Ready {
    pub master: bool,
    pub stdin: bool,
    pub inbox: bool,
}

/// Wait for any input to become readable, at most `timeout_ms` milliseconds.
/// The ONE readiness primitive both hosts implement with their own call:
/// `poll(2)` over fds on Unix, `WaitForMultipleObjects` over handles on
/// Windows. A readiness this call reports is always re-checked by the read
/// that services it, so a spurious wake is harmless.
#[cfg(unix)]
pub(in crate::graph) fn wait_ready(
    pty: &Pty,
    console: &Console,
    inbox: Option<&Inbox>,
    stdin_open: bool,
    timeout_ms: i32,
) -> Ready {
    use std::os::unix::io::AsRawFd;
    let mut fds: Vec<libc::pollfd> = Vec::new();
    if stdin_open {
        // **A hung-up stdin is READY, not idle.** `poll(2)` reports an empty
        // pipe whose write end has closed as `POLLHUP` and *not* `POLLIN`
        // (measured on this host), so a readiness test that asked only for
        // `POLLIN` would never wake this arm again: the loop would keep polling
        // a fd that returns immediately, for the rest of the run, and never
        // reach the 0-byte `read` that latches `stdin_eof`. That is a 100 %-core
        // spin on `producer | aoide conduct` once the producer exits — the
        // defect this mask closes, and the exact semantics the Windows arm now
        // reaches by asking the pipe rather than believing a close.
        fds.push(libc::pollfd {
            fd: console.fd,
            events: libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL,
            revents: 0,
        });
    }
    fds.push(libc::pollfd { fd: pty.master.as_raw_fd(), events: libc::POLLIN, revents: 0 });
    let fds_before_inbox = fds.len();
    if let Some(inbox) = inbox {
        for fd in inbox.handles() {
            fds.push(libc::pollfd { fd, events: libc::POLLIN, revents: 0 });
        }
    }
    let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
    if rc <= 0 {
        return Ready::default();
    }
    let stdin_ready = stdin_open
        && (fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)) != 0;
    let master_ready =
        (fds[stdin_open as usize].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0;
    let inbox_ready = fds[fds_before_inbox..].iter().any(|p| p.revents != 0);
    Ready { master: master_ready, stdin: stdin_ready, inbox: inbox_ready }
}

/// Wait for any input to become readable, at most `timeout_ms` milliseconds.
/// The ONE readiness primitive both hosts implement with their own call:
/// `poll(2)` over fds on Unix, `WaitForMultipleObjects` over the handles it
/// CAN wait on, with two things deliberately kept OUT of the array. The
/// pseudo console's output pipe: a synchronous anonymous pipe's read handle is
/// *always* signalled, so a pipe in a wait-any array wins the lowest-index race
/// forever and no other source can ever be reported (proven on ThinkChiyo with
/// a raw probe; it is why `aoide send` into a Windows session was dead). And a
/// non-console stdin: it is the same trap (a pipe's handle is signalled when
/// empty), so **only a console's stdin gets a slot** — the seat the wait's
/// answer is valid in — while every other stdin is POLLED and read
/// non-blocking by the loop below. Both are why this arm clamps its own wait to
/// [`CONPTY_POLL_MS`]: the caller's longer budget is served as a bounded
/// sequence of short waits, each of which still reports an inbox byte as soon
/// as it lands. `Ready.master` is therefore always derived from the pipe's own
/// state, never from the wait.
#[cfg(windows)]
pub(in crate::graph) fn wait_ready(
    pty: &Pty,
    console: &Console,
    inbox: Option<&Inbox>,
    stdin_open: bool,
    timeout_ms: i32,
) -> Ready {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Threading::WaitForMultipleObjects;
    let mut handles: Vec<HANDLE> = Vec::new();
    // The stdin slot exists ONLY for a console (see `wait_ready`'s own doc: a
    // console is the one handle class whose readiness this wait can be trusted
    // to report). Every other stdin is polled below and read without blocking.
    let stdin_slot = if stdin_open && console.stdin_is_waitable() {
        handles.push(console.stdin);
        Some(handles.len() - 1)
    } else {
        None
    };
    let inbox_from = handles.len();
    if let Some(inbox) = inbox {
        for handle in inbox.handles() {
            handles.push(handle);
        }
    }
    let inbox_count = handles.len() - inbox_from;
    let waited = if handles.is_empty() {
        // Nothing waitable at all (no console stdin AND no inbox): the budget
        // is a poll's here too, never zero — a caller asking for 0 is asking to
        // drain, and `sleep(0)` would spin the whole settle window.
        std::thread::sleep(std::time::Duration::from_millis(
            if timeout_ms <= 0 {
                CONPTY_POLL_MS
            } else {
                (timeout_ms as u32).min(CONPTY_POLL_MS)
            } as u64,
        ));
        windows_sys::Win32::Foundation::WAIT_TIMEOUT
    } else {
        // The budget is a poll's, never zero: the pipe is re-peeked at this
        // rate, and a caller asking for 0 (the loop does, once the child has
        // exited and it is only draining) would otherwise turn the settle
        // window into a hot loop of `WaitForMultipleObjects(0)` calls.
        let ms = if timeout_ms <= 0 {
            CONPTY_POLL_MS
        } else {
            (timeout_ms as u32).min(CONPTY_POLL_MS)
        };
        unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, ms) }
    };
    let mut ready = ready_of_wait(waited, stdin_slot, inbox_from, inbox_count, pty.has_output());
    // The POLLED stdin, kept out of the pure mapping above: a console's stdin
    // is the wait's own answer (`stdin_slot` is `Some`), and every other stdin
    // is asked here and read without blocking — which is how a
    // `producer | aoide conduct` keeps its producer's bytes on this host
    // exactly as it does on Unix. `stdin_slot.is_none()` IS "this stdin is not
    // a console" (`Console::stdin_is_waitable`'s own predicate), so there is no
    // second clause to write.
    if stdin_slot.is_none() && console.stdin_polled_ready() {
        ready.stdin = true;
    }
    ready
}
/// The millisecond budget one Windows wait may spend before the pipe is polled
/// again. The output pipe cannot be waited on (see [`wait_ready`]), so an
/// inbox event is reported within this bound and the pipe is re-peeked at it;
/// the loop's own 1 s tick still applies on top.
#[cfg(windows)]
const CONPTY_POLL_MS: u32 = 25;

/// How often the real console's geometry is asked for (`Console::resized`):
/// the loop polls at [`CONPTY_POLL_MS`], and a console cannot be resized by
/// hand faster than a few times a second, so a quarter second is free
/// accuracy.
#[cfg(windows)]
const CONSOLE_SIZE_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// `WaitForMultipleObjects`' own answer mapped to [`Ready`] — pure, so every
/// row is a table test. **`WAIT_FAILED` is not "inbox ready"**: the earlier
/// shape tested `waited < WAIT_OBJECT_0`, which on `u32` is never true, so a
/// failure fell through to `signalled >= inbox_from` and the loop spun through
/// `drain()` with no timeout honored. A failed wait is no source at all, and it
/// says so on stderr rather than being swallowed.
#[cfg(windows)]
fn ready_of_wait(
    waited: u32,
    stdin_slot: Option<usize>,
    inbox_from: usize,
    inbox_count: usize,
    master: bool,
) -> Ready {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    if waited == WAIT_TIMEOUT {
        return Ready { master, stdin: false, inbox: false };
    }
    // `WAIT_FAILED` (`u32::MAX`) is a failure, not a source. A `WAIT_ABANDONED`
    // cannot occur (no mutex is ever in this array), so there is no third case
    // to name — and no `waited < WAIT_OBJECT_0` clause either, which on a `u32`
    // could never be true and was dead the day it was written.
    if waited == u32::MAX {
        eprintln!(
            "aoide conduct: WaitForMultipleObjects failed (0x{waited:08X}); nothing was read this tick"
        );
        return Ready { master, stdin: false, inbox: false };
    }
    let signalled = (waited - WAIT_OBJECT_0) as usize;
    Ready {
        master,
        stdin: stdin_slot == Some(signalled),
        inbox: inbox_count > 0 && signalled >= inbox_from,
    }
}

// ── the raw Win32 handles this seam owns ─────────────────────────────────

/// A `HANDLE` this seam owns while a spawn is still being built: every early
/// return between `CreatePipe` and the `Pty` being constructed closes it
/// exactly once. A spawn failure has no client attached, so the close ORDER
/// the live `Pty` documents cannot deadlock here — this type exists so the
/// failure path leaks nothing, and `into_raw` hands the handle to the `Pty`
/// that owns it from then on.
#[cfg(windows)]
struct OwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl OwnedHandle {
    fn into_raw(self) -> windows_sys::Win32::Foundation::HANDLE {
        let handle = self.0;
        std::mem::forget(self);
        handle
    }
}

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
struct Hpcon(windows_sys::Win32::System::Console::HPCON);

#[cfg(windows)]
impl Hpcon {
    fn into_raw(self) -> windows_sys::Win32::System::Console::HPCON {
        let hpc = self.0;
        std::mem::forget(self);
        hpc
    }
}

#[cfg(windows)]
impl Drop for Hpcon {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::System::Console::ClosePseudoConsole(self.0) };
    }
}

/// `CreateProcessW`'s command line: the program, then each argument, quoted by
/// the same rules the C runtime parses them back with (a quoted argument keeps
/// its spaces, a backslash before a quote is escaped, trailing backslashes are
/// doubled). `std::process::Command` does this internally and exposes none of
/// it, and this arm cannot borrow it — the pseudo console is an attribute of
/// `CreateProcessW` itself, which `Command` never calls with one.
#[cfg(windows)]
fn command_line(program: &str, args: &[String]) -> Vec<u16> {
    let mut line = String::new();
    push_quoted(&mut line, program);
    for arg in args {
        line.push(' ');
        push_quoted(&mut line, arg);
    }
    wide(&line)
}

#[cfg(windows)]
fn push_quoted(out: &mut String, arg: &str) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => {
                backslashes += 1;
                out.push('\\');
            }
            '"' => {
                // The quote is escaped by doubling the backslashes before it.
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push('\\');
                out.push('"');
            }
            _ => {
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    // Trailing backslashes would escape the closing quote: double them.
    for _ in 0..backslashes {
        out.push('\\');
    }
    out.push('"');
}

/// The child's environment block, built rather than inherited: `CreateProcessW`
/// with `CREATE_UNICODE_ENVIRONMENT` takes a block, and this child's block is
/// the parent's MINUS the harness session markers, PLUS its own session id and
/// a managed task's two variables — exactly the shaping [`spawn_on_pty`]'s
/// Unix arm gets from `Command::env`/`env_remove`. Entries are
/// `NAME=VALUE\0`, the whole block ends with a second `\0`, and the order is
/// the case-insensitive alphabetical order the environment block convention
/// asks for.
/// The child's environment block, built rather than inherited: `CreateProcessW`
/// with `CREATE_UNICODE_ENVIRONMENT` takes a block, and this child's block is
/// the parent's MINUS the harness session markers, PLUS its own session id and
/// a managed task's two variables — exactly the shaping [`spawn_on_pty`]'s
/// Unix arm gets from `Command::env`/`env_remove`. Entries are
/// `NAME=VALUE\0`, the whole block ends with a second `\0`, and the order is
/// the one an environment block requires: **the `=`-prefixed entries Windows
/// keeps for a drive's current directory come FIRST, then everything else
/// sorted case-insensitively by name.** Getting that order wrong is not
/// cosmetic — a block out of the required order fails the child's own startup
/// with `STATUS_DLL_INIT_FAILED` (`0xC0000142`), which is exactly what a
/// `cmd /C echo` child did on ThinkChiyo before this ordering was fixed.
#[cfg(windows)]
fn environment_block(session_id: &str, task: Option<&TaskContext>) -> Vec<u16> {
    let mut vars: Vec<(String, std::ffi::OsString)> = std::env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            !aoide_protocol::agents::session_env_markers()
                .any(|marker| marker.eq_ignore_ascii_case(&name))
        })
        .map(|(name, value)| (name.to_string_lossy().into_owned(), value))
        .collect();
    let mut set = |name: &str, value: String| {
        vars.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        vars.push((name.to_string(), value.into()));
    };
    set(super::conduct::CHILD_SESSION_ENV, session_id.to_string());
    if let Some(task) = task {
        set(super::conduct::CHILD_TASK_ENV, task.slug.clone());
        if let Some(path) = &task.instructions_path {
            set(super::conduct::CHILD_TASK_INSTRUCTIONS_ENV, path.clone());
        }
    }
    // `=`-prefixed names (a drive's current directory) are Windows' own
    // hidden entries: they must lead the block, in the order Windows gave
    // them, and only the rest is sorted.
    let (mut leading, mut sorted): (Vec<_>, Vec<_>) =
        vars.into_iter().partition(|(name, _)| name.starts_with('='));
    sorted.sort_by(|a, b| a.0.to_ascii_uppercase().cmp(&b.0.to_ascii_uppercase()));
    leading.append(&mut sorted);

    use std::os::windows::ffi::OsStrExt;
    let mut block: Vec<u16> = Vec::new();
    for (name, value) in leading {
        block.extend(std::ffi::OsStr::new(&name).encode_wide());
        block.push('=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{spawn_on_pty, wait_ready, Console};
    #[cfg(windows)]
    use super::*;
    #[cfg(windows)]
    use crate::graph::testutil::{read_to_exit, unique_stage};

    /// `WaitForMultipleObjects`' own answer, as a table — including the row the
    /// review proved wrong: a FAILED wait must be **no source at all**, never
    /// "inbox ready" (the old test `waited < WAIT_OBJECT_0` is never true on a
    /// `u32`, so a failure fell through and the loop spun through `drain()`
    /// with no timeout honored).
    #[cfg(windows)]
    #[test]
    fn a_failed_wait_is_no_source_at_all() {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        // Nothing signalled: the pipe's own state is still reported, the two
        // waitable sources are not.
        assert_eq!(
            ready_of_wait(WAIT_TIMEOUT, Some(0), 1, 1, true),
            Ready { master: true, stdin: false, inbox: false }
        );
        // A failed wait is a failure, not a source — whatever the slots are.
        assert_eq!(
            ready_of_wait(u32::MAX, Some(0), 1, 1, false),
            Ready::default(),
            "WAIT_FAILED must never read as an inbox byte"
        );
        // The stdin slot, and then the first inbox slot past it.
        assert_eq!(
            ready_of_wait(WAIT_OBJECT_0, Some(0), 1, 2, false),
            Ready { master: false, stdin: true, inbox: false }
        );
        assert_eq!(
            ready_of_wait(WAIT_OBJECT_0 + 1, Some(0), 1, 2, false),
            Ready { master: false, stdin: false, inbox: true }
        );
        // No stdin open: the first handle IS the first inbox event.
        assert_eq!(
            ready_of_wait(WAIT_OBJECT_0, None, 0, 1, false),
            Ready { master: false, stdin: false, inbox: true }
        );
        // And an inbox handle count of zero never claims an inbox byte.
        assert_eq!(
            ready_of_wait(WAIT_OBJECT_0 + 3, None, 0, 0, false),
            Ready::default()
        );
    }

    /// A failed spawn leaks nothing: the two error paths the review named (a
    /// failing second `CreatePipe`, a failing `UpdateProcThreadAttribute`) are
    /// the ones a bogus program exercises — `CreateProcessW` on a program the
    /// host cannot find fails after the pipes, the console and the attribute
    /// list already exist. The PROCESS's own handle count is the fact, so this
    /// needs no instrumentation inside the arm.
    #[cfg(windows)]
    #[test]
    fn a_failed_spawn_leaks_no_handle() {
        // The handle count is PROCESS-wide, so this measurement must not run
        // beside another test's own handles: the crate's own lock is what
        // serializes it (`crate::env_lock`).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
        let handle_count = || {
            let mut count: u32 = 0;
            assert_ne!(
                unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) },
                0
            );
            count
        };
        let program = "aoide-w5-no-such-program-anywhere";
        let args: Vec<String> = Vec::new();
        // Warm up: the first calls can load a DLL or two, and the kernel trims
        // its handle table on its own schedule — which is why the assertion
        // below is "no GROWTH", not "identical". A leak is growth, and the two
        // handles per failed spawn this arm used to leave behind would show as
        // +40 after twenty.
        for _ in 0..3 {
            let _ = spawn_on_pty(program, &args, "w5-leak", Some(WinSize::conventional()), None);
        }
        let before = handle_count();
        for _ in 0..20 {
            let err = spawn_on_pty(program, &args, "w5-leak", Some(WinSize::conventional()), None);
            assert!(err.is_err(), "a program that does not exist cannot spawn");
        }
        let after = handle_count();
        assert!(
            after <= before,
            "20 failed spawns grew this process's handles {before} → {after}"
        );
    }

    /// The child's own stdout is a CONSOLE, not this process's pipe: the
    /// pseudo console attaches the client to `CONOUT$` because the spawn names
    /// `STARTF_USESTDHANDLES` with all three handles NULL and inherits nothing
    /// (`bInheritHandles = FALSE`) — the shape shipping ConPTY implementations
    /// use, and the one the runs here prove. Naming the console's own pipe ends
    /// instead leaves a conducted TUI seeing a redirected stdout, degrading or
    /// refusing, which is what this test refuses to accept.
    #[cfg(windows)]
    #[test]
    fn the_childs_own_stdout_is_a_console_not_this_processs_pipe() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_stage("conpty-console");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let (mut child, mut pty) = spawn_on_pty(
            "powershell",
            &[
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "[Console]::IsOutputRedirected".to_string(),
            ],
            "conpty-console",
            Some(WinSize::conventional()),
            None,
        )
        .expect("a child starts on a pseudo console");
        let out = read_to_exit(&mut pty, &mut child);
        let text = String::from_utf8_lossy(&out).into_owned();
        pty.close();
        assert!(
            text.contains("False"),
            "the child's stdout must be a console (IsOutputRedirected False): {text:?}"
        );
        assert!(
            !text.contains("True"),
            "the child's stdout was this process's pipe: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The injection socket's PARENT directory is owner-only after `bind`, on
    /// this host's own terms: a Windows AF_UNIX socket is a reparse point whose
    /// own policy cannot be read, so the directory is the only gate, and this
    /// is the one native binder that did not create it through
    /// `owner_only::ensure_private_dir`.
    #[cfg(windows)]
    #[test]
    fn binding_the_inbox_leaves_its_directory_owner_only() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("aoide-pty-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        let door = root.join("rt").join("aoide");
        let path = door.join("session-bind.sock");

        let inbox = Inbox::bind(&path, "bind-probe").expect("the injection socket binds");
        assert!(
            aoide_protocol::owner_only::dir_privacy(&door).unwrap().is_none(),
            "the socket's own directory must be owner-only: {:?}",
            aoide_protocol::owner_only::dir_privacy(&door)
        );
        drop(inbox);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The mechanism the Windows arm's wait stands on, proven before anything
    /// is built on it: an `AF_UNIX` connection must signal through
    /// `WSAEventSelect` and be answerable by `WaitForMultipleObjects`, and the
    /// inbox must hand back the bytes the connection wrote. This is the probe
    /// the coordinator asked for first — if it were false, the inbox would
    /// have to feed the loop from a thread instead of from an event.
    #[cfg(windows)]
    #[test]
    fn an_af_unix_connection_signals_the_inboxs_event_and_delivers_its_bytes() {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::WaitForMultipleObjects;

        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("aoide-pty-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        let path = root.join("inject.sock");

        let mut inbox = Inbox::bind(&path, "wsa-probe").expect("the injection socket binds");
        let handles = inbox.handles();
        assert_eq!(handles.len(), 1, "one handle before any connection: the listener's");
        assert_eq!(
            unsafe { WaitForMultipleObjects(1, handles.as_ptr(), 0, 0) },
            WAIT_TIMEOUT,
            "nothing to accept yet, and the wait says so rather than blocking"
        );

        let mut client =
            aoide_protocol::win_unix::UnixStream::connect(&path).expect("the door connects");
        let handles = inbox.handles();
        assert_eq!(
            unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, 5_000) },
            WAIT_OBJECT_0,
            "an inbound AF_UNIX connection sets the listener's event"
        );
        assert!(inbox.drain().is_empty(), "accepted, but nothing written yet");

        use std::io::Write as _;
        client.write_all(b"typed text").expect("the door writes");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut delivered = Vec::new();
        while delivered.is_empty() && std::time::Instant::now() < deadline {
            let handles = inbox.handles();
            assert!(handles.len() > 1, "the accepted connection has its own event");
            assert_ne!(
                unsafe {
                    WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, 5_000)
                },
                WAIT_TIMEOUT,
                "an AF_UNIX socket must signal through WSAEventSelect"
            );
            for chunk in inbox.drain() {
                delivered.extend(chunk);
            }
        }
        assert_eq!(delivered, b"typed text");

        inbox.drain();
        drop(inbox);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F8, the Unix twin of the Windows EOF path: a pipe whose write end has
    /// closed is reported by `poll(2)` as `POLLHUP` — **not** `POLLIN` — so a
    /// readiness test that asked only for `POLLIN` never wakes the loop's stdin
    /// arm again once the producer exits. The loop then spins on a poll that
    /// returns immediately for the rest of the run (a 100 %-core burn on
    /// `producer | aoide conduct`) and never reaches the 0-byte `read` that
    /// latches `stdin_eof`. This run is the failing-first proof: with the old
    /// mask, `ready.stdin` is false and nothing is latched.
    #[cfg(unix)]
    #[test]
    fn a_hung_up_stdin_is_reported_ready_so_the_loop_latches_its_eof() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "the fixture's pipe");
        let (read_fd, write_fd) = (fds[0], fds[1]);
        let saved_stdin = unsafe { libc::dup(libc::STDIN_FILENO) };
        assert!(saved_stdin >= 0, "dup of this process's stdin");
        assert_eq!(
            unsafe { libc::dup2(read_fd, libc::STDIN_FILENO) },
            libc::STDIN_FILENO
        );
        // The producer exits: its write end closes, which is the shape under
        // test. What is left is a pipe with no writer and no bytes.
        unsafe { libc::close(write_fd) };

        let (mut child, pty) = spawn_on_pty(
            "sh",
            &["-c".to_string(), "sleep 5".to_string()],
            "hung-up-stdin",
            None,
            None,
        )
        .expect("a pty child starts");
        let mut console = Console::attach(true);
        let ready = wait_ready(&pty, &console, None, true, 0);
        let mut buf = [0u8; 64];
        let eof = console.read(&mut buf);

        child.kill();
        let _ = child.wait_for(std::time::Duration::from_secs(2));
        pty.close();
        unsafe {
            libc::dup2(saved_stdin, libc::STDIN_FILENO);
            libc::close(saved_stdin);
            libc::close(read_fd);
        }

        assert!(
            ready.stdin,
            "a hung-up stdin must be REPORTED ready — otherwise the loop never wakes its stdin \
             arm again and spins on a poll that returns at once"
        );
        assert_eq!(eof.map(|n| n).unwrap_or(usize::MAX), 0, "and it reads as EOF, which is what latches stdin_eof");
    }

    /// The command line the child's own C runtime will parse back: a plain
    /// argument, one with spaces, one with an embedded quote, one with a
    /// trailing backslash, an empty one. Each is quoted by the documented
    /// rules, and the empty argument stays a real (empty) argument rather
    /// than vanishing.
    #[cfg(windows)]
    #[test]
    fn command_line_quotes_the_arguments_the_c_runtime_parses_back() {
        let args = ["plain", "with space", "a\"b", "back\\slash", "trail\\", ""]
            .map(str::to_string);
        let line = command_line("cmd", &args);
        let line = String::from_utf16(&line[..line.len() - 1]).unwrap();
        // `trail\` is deliberately NOT quoted: with no space and no quote in
        // it, nothing needed escaping, and a bare trailing backslash only
        // matters inside a quoted argument (where it would escape the closing
        // quote — the case `back\slash` above never reaches either).
        assert_eq!(
            line,
            r#"cmd plain "with space" "a\"b" back\slash trail\ """#
        );
    }

    /// The environment block carries the child's own session id and a managed
    /// task's two variables, drops every parent harness marker, and ends with
    /// the double NUL the block convention requires.
    #[cfg(windows)]
    #[test]
    fn environment_block_shapes_the_childs_own_environment() {
        let marker = aoide_protocol::agents::session_env_markers()
            .next()
            .expect("at least one harness marker is registered");
        std::env::set_var(marker, "the-parents-session");
        let task = TaskContext {
            slug: "a-task".into(),
            instructions_path: Some("C:\\x\\y.md".into()),
            report_to: None,
        };
        let block = environment_block("child-id", Some(&task));
        let entries: Vec<String> = block[..block.len() - 1]
            .split(|c| *c == 0)
            .map(|e| String::from_utf16_lossy(e))
            .collect();
        assert!(entries.iter().any(|e| e == "AOIDE_SESSION_ID=child-id"), "{entries:?}");
        assert!(entries.iter().any(|e| e == "AOIDE_TASK=a-task"), "{entries:?}");
        assert!(entries.iter().any(|e| e == "AOIDE_TASK_INSTRUCTIONS=C:\\x\\y.md"), "{entries:?}");
        assert!(
            !entries.iter().any(|e| e.starts_with(&format!("{marker}="))),
            "the parent's harness marker must be gone: {entries:?}"
        );
        assert_eq!(*block.last().unwrap(), 0, "the block ends with its own NUL");
        std::env::remove_var(marker);
    }
}
