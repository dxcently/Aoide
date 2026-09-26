//! The native process table: the facts Unix reads out of `/proc` and the two
//! acts it performs on a pid, answered by the APIs that own them on Windows.
//!
//! | fact | Unix | here |
//! | --- | --- | --- |
//! | a pid's parent | `/proc/<pid>/stat` field 4 | `PROCESSENTRY32W.th32ParentProcessID` |
//! | a pid's start time | `/proc/<pid>/stat` field 22 | `GetProcessTimes` creation `FILETIME` |
//! | is a pid live | `kill(pid, 0)` | `OpenProcess` + `GetExitCodeProcess` |
//! | whose token a pid is | `/proc/<pid>/status` `Uid:` | `OpenProcess`+`OpenProcessToken`+`GetTokenInformation` |
//! | a pid's argv | `/proc/<pid>/cmdline` | `NtQueryInformationProcess(ProcessCommandLineInformation)` + `CommandLineToArgvW` |
//! | end a pid | `kill(pid, SIGTERM\|SIGKILL)` | `TerminateProcess` |
//! | wait for a pid's end | `waitpid(pid, …, WNOHANG)` | `WaitForSingleObject(h, 0)` |
//!
//! Three callers, one implementation (`pkgs/aoide/crates/AGENTS.md`: no
//! cross-crate copying): `aoide_storage::attest`'s pid-ancestry walk and
//! pid-reuse defence, `aoide_storage::fs`'s process acts, and `crate::dialog`'s
//! locker probe. It lives in this crate because `aoide-protocol` is the DAG
//! leaf all of them already depend on.
//!
//! **Liveness keeps the Unix reading, one-for-one.** `ERROR_INVALID_PARAMETER`
//! (and `ERROR_NOT_FOUND`) is the one absent verdict — the exact stand-in for
//! `ESRCH`; `ERROR_ACCESS_DENIED` is live, exactly as `EPERM` is; and any
//! other failure, or a `GetExitCodeProcess` that does not answer, is
//! conservatively live, so an unanswerable probe never reaps. A liveness
//! probe is not an identity: a recycled pid is live.
//!
//! **A pid of `0` is refused before the call**, the same discipline the Unix
//! arm applies to `0` and to anything above `pid_t::MAX`: `0` names Windows'
//! Idle pseudo-process, which is not a process any caller can be, and the API
//! answers it with the same `ERROR_INVALID_PARAMETER` an out-of-range pid
//! gets. Refusing it here keeps "absent" from depending on which refusal the
//! caller's pid happened to produce.
//!
//! **A start time is the creation time**, in `FILETIME` units (100 ns since
//! 1601) rather than clock ticks since boot. It is the same fact under a
//! different unit: monotonic within a boot, unique per process instance, and
//! therefore the value a pid-reuse defence re-derives and compares. Its
//! caller refuses a literal `0` as "the pid does not exist" (the Unix
//! contract's own rule); on Windows only the Idle and System pseudo-processes
//! report one, and neither can name a session.

use std::io;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NOT_FOUND, FILETIME, HANDLE, HLOCAL,
    INVALID_HANDLE_VALUE, LocalFree, STILL_ACTIVE, UNICODE_STRING, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::{LookupAccountSidW, PSID, SidTypeUnknown};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};

/// One row of the process table: what a pid is, who started it, and the
/// executable's file name (`comm`'s role on Unix, with Windows' own
/// `.exe` suffix still attached — folding that is the asking caller's
/// policy, not this reader's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
}

/// A snapshot handle that closes itself, so no early return leaks it.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// The whole process table, in one `CreateToolhelp32Snapshot` walk. An error
/// is returned rather than an empty list: "no processes" and "the snapshot
/// failed" are different answers, and a caller that conflates them turns a
/// failed read into evidence.
pub fn processes() -> io::Result<Vec<Proc>> {
    let snapshot = Snapshot(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) });
    if snapshot.0 == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut out = Vec::new();
    // `Process32FirstW` seeds the walk; every later row comes from
    // `Process32NextW`, whose `ERROR_NO_MORE_FILES` end is the documented
    // stop, checked by the BOOL rather than by a count.
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }
    loop {
        out.push(Proc {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            exe: exe_name(&entry),
        });
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            return Ok(out);
        }
    }
}

/// The NUL-terminated `szExeFile` as a `String`. Lossy on purpose: a name
/// that is not valid UTF-16 still identifies the process to a human, and
/// refusing to answer would lose the whole table over one odd row.
fn exe_name(entry: &PROCESSENTRY32W) -> String {
    let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..end])
}

/// `pid`'s ancestry, self first, nearest ancestor first — the walk
/// `aoide_storage::attest::pid_ancestry` performs over `/proc`, done from ONE
/// snapshot rather than one lookup per level. Bounded by `limit`, stops at the
/// root (the System process's parent is `0`, which names no process), and
/// returns what it has if the pid is not in the table at all.
pub fn parent_chain(pid: u32, limit: usize) -> Vec<u32> {
    let Ok(table) = processes() else {
        return Vec::new();
    };
    let mut chain = Vec::new();
    let mut cur = pid;
    for _ in 0..limit {
        chain.push(cur);
        match table.iter().find(|p| p.pid == cur) {
            Some(p) if p.parent > 1 && p.parent != cur => cur = p.parent,
            _ => break,
        }
    }
    chain
}

/// `pid`'s creation time in `FILETIME` units, or `None` for a pid this
/// process cannot open or time — a vanished pid, a malformed read. A pid
/// above the real range, or `0`, is refused by the API and lands here too.
pub fn start_time(pid: u32) -> Option<u64> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
}

/// Is `pid` a live process? See the module doc for the reading — this is the
/// `kill(pid, 0)` arm's contract with a different syscall under it.
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return match io::Error::last_os_error().raw_os_error() {
            // The two absent verdicts: no process owns this pid. Anything
            // else — `ERROR_ACCESS_DENIED` above all, another user's process
            // that certainly exists — is conservatively live.
            Some(code) if code == ERROR_INVALID_PARAMETER as i32 || code == ERROR_NOT_FOUND as i32 => false,
            Some(code) if code == ERROR_ACCESS_DENIED as i32 => true,
            _ => true,
        };
    }
    let mut code: u32 = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe { CloseHandle(handle) };
    // An open on a pid that has exited but whose handle is still held
    // elsewhere succeeds and reports its exit code — reaped, not live.
    ok == 0 || code == STILL_ACTIVE as u32
}

/// End `pid`. **`TerminateProcess` is the only termination primitive this
/// host has**: there is no signal to send and nothing to catch — a Windows
/// child cannot trap, ignore or be politely asked (measured: an attempt to
/// make one ignore termination has no primitive to call), so a caller's
/// "ask, then wait a bound" and "force" are the same call here. The exit code
/// is a nonzero failure code, the convention every Windows task manager
/// follows. Access is checked by the kernel: another user's process is an
/// `ERROR_ACCESS_DENIED` error, never a silent no-op.
///
/// `0` is refused before the call, as every other function here refuses it.
pub fn terminate(pid: u32) -> io::Result<()> {
    if pid == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pid 0 names no process"));
    }
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let ok = unsafe { TerminateProcess(handle, 1) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A wait on `pid`'s end: `waitpid(pid, …, WNOHANG)`'s three verdicts, with
/// `WaitForSingleObject` under them.
///
/// | `waitpid` | here |
/// | --- | --- |
/// | returns `pid`: reaped | `WAIT_OBJECT_0`: the process object is signalled, `pid` has ended |
/// | returns `0`: still running | `WAIT_TIMEOUT`: the zero-length test timed out |
/// | returns `-1` (`ECHILD`): no wait can succeed | an unopenable handle: no wait can succeed |
///
/// **Two facts differ, and neither is hidden from a caller.** There is no
/// zombie on this host: a process object is signalled when it ends and is
/// freed with its last handle, so `Exited` says "it has ended" and never "this
/// process collected a child's status". And a wait here is not restricted to
/// this process's own children: `SYNCHRONIZE` is available for any process
/// this token may open, so unlike `waitpid`, a pid left by an EARLIER
/// invocation's spawn is waitable — a strictly wider answer, and a caller that
/// treats `NotWaitable` as "not our child, poll liveness instead" takes the
/// same branch it always took.
///
/// `block` distinguishes the two `waitpid` modes a caller here uses:
/// `false` is `WNOHANG` (a zero-length test), `true` is a real wait.
pub fn wait_for_exit(pid: u32, block: bool) -> Waited {
    if pid == 0 {
        return Waited::NotWaitable;
    }
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Waited::NotWaitable;
    }
    let timeout = if block { INFINITE } else { 0 };
    let event = unsafe { WaitForSingleObject(handle, timeout) };
    unsafe { CloseHandle(handle) };
    match event {
        WAIT_OBJECT_0 => Waited::Exited,
        WAIT_TIMEOUT => Waited::Running,
        // `WAIT_FAILED`/`WAIT_ABANDONED` on a process handle: nothing here can
        // answer "has it ended", which is `NotWaitable`'s whole meaning.
        _ => Waited::NotWaitable,
    }
}

/// The three verdicts of a wait — see [`wait_for_exit`] for the mapping from
/// `waitpid` and for the two places Windows answers differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// The process has ended.
    Exited,
    /// It is still running (the non-blocking wait's timeout).
    Running,
    /// No wait can succeed for this pid from this process.
    NotWaitable,
}

/// The user a token belongs to, as the canonical `S-1-5-…` string Windows
/// prints — the native answer to the `Uid:` line of `/proc/<pid>/status`,
/// and the only form of "who" a Windows caller can compare or show. There is
/// no uid: a SID is the fact, and a number derived from one would be
/// invented.
///
/// `0` is refused before the call, exactly as [`is_alive`] refuses it (the
/// Idle pseudo-process is nobody's peer). A pid this process may not query
/// is an error, not a guess.
pub fn process_user_sid(pid: u32) -> io::Result<String> {
    if pid == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pid 0 names no user"));
    }
    sid_of_pid(pid)
}

/// `pid`'s command line, split into the arguments Windows itself would read
/// it as — `argv` from `/proc/<pid>/cmdline` on Unix. The one fact a caller
/// cannot get from the process table: `PROCESSENTRY32W` carries the image
/// name, never the line.
///
/// The string comes from `NtQueryInformationProcess(ProcessCommandLineInformation)`
/// (class 60, `windows-sys`'s `Wdk_System_Threading`), the read Task Manager
/// itself makes, into a buffer this process owns — the answer describes bytes
/// inside that buffer, so **nothing here frees memory it did not take** (a
/// class that ever answered with an allocation of its own would leak a few
/// hundred bytes here, never free a block this process never owned). The
/// split is `CommandLineToArgvW`'s, because how a Windows command line
/// tokenizes is the host's rule and not a caller's dialect.
///
/// `0` and an unopenable pid are refused exactly as the other reads refuse
/// them: a pid this process may not query is an error, never a guess. An EMPTY
/// command line is not that error — a process can genuinely have none, and the
/// answer then is an empty argument list, because tokenizing an empty line
/// would fabricate one empty argument that no process ever passed.
pub fn command_argv(pid: u32) -> io::Result<Vec<String>> {
    let raw = command_line(pid)?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let wide: Vec<u16> = raw.encode_utf16().chain(std::iter::once(0)).collect();
    let mut argc: i32 = 0;
    let argv = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut argc) };
    if argv.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut args = Vec::with_capacity(argc.max(0) as usize);
    for i in 0..argc as isize {
        let start = unsafe { *argv.offset(i) };
        if start.is_null() {
            continue;
        }
        let mut len = 0usize;
        while unsafe { *start.add(len) } != 0 {
            len += 1;
        }
        args.push(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(start, len) }));
    }
    unsafe { LocalFree(argv as HLOCAL) };
    Ok(args)
}

/// [`command_argv`]'s raw string, before this host's own tokenizing: the exact
/// line `NtQueryInformationProcess` wrote into a buffer this process owns —
/// see [`command_argv`]'s doc for why that ownership matters.
fn command_line(pid: u32) -> io::Result<String> {
    if pid == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pid 0 names no process"));
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut size: u32 = 0;
    let probe = unsafe {
        NtQueryInformationProcess(handle, ProcessCommandLineInformation, std::ptr::null_mut(), 0, &mut size)
    };
    let floor = std::mem::size_of::<UNICODE_STRING>() as u32;
    if probe >= 0 || size < floor {
        unsafe { CloseHandle(handle) };
        // A success with no length, or a length that cannot hold the
        // descriptor the answer starts with: a line this reader cannot read,
        // reported rather than returned as an empty string.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the process reports no readable command line (status {probe:#x}, {size} bytes)"),
        ));
    }
    let mut buf = vec![0u8; size as usize];
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            ProcessCommandLineInformation,
            buf.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    unsafe { CloseHandle(handle) };
    if status < 0 {
        return Err(io::Error::other(format!(
            "NtQueryInformationProcess(ProcessCommandLineInformation) failed: {status:#x}"
        )));
    }
    // The descriptor is READ unaligned rather than borrowed as a reference:
    // `buf` is a byte buffer (alignment 1) and `UNICODE_STRING` is not, so a
    // `&*` cast would be an alignment assumption this code has no right to
    // make — `read_unaligned` copies the three fields out instead.
    let unicode = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const UNICODE_STRING) };
    // A process with no command line at all: the answer's length is zero, so
    // there is no string to read and — load-bearing — no `Buffer` to trust.
    // Answered as an empty line, which the caller turns into an empty argv;
    // refusing here would report a real state as a read failure.
    if unicode.Length == 0 {
        return Ok(String::new());
    }
    // The string must lie inside the buffer THIS process allocated: the
    // descriptor's own pointer is what decides, so a host that ever answered
    // with a buffer of its own (or a length past the end) is refused by name
    // rather than read through.
    let start = unicode.Buffer as usize;
    let base = buf.as_ptr() as usize;
    if unicode.Length % 2 != 0 || start < base || start + unicode.Length as usize > base + buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "the command line ({len} bytes at {start:#x}) does not lie inside the buffer this process \
                 allocated ({base:#x}..{end:#x})",
                len = unicode.Length,
                start = start,
                base = base,
                end = base + buf.len()
            ),
        ));
    }
    let len = unicode.Length as usize / 2;
    Ok(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(unicode.Buffer, len) }))
}

/// The SID of the user THIS process's token is for — the native stand-in for
/// `geteuid()`'s "which user am I", and the other half of every same-user
/// comparison ([`process_user_sid`] is the first).
pub fn current_user_sid() -> io::Result<String> {
    crate::owner_only::TokenUserSid::current()?.to_string_sid()
}

/// A pid's token user, with the pid-reuse window closed around it: the
/// process is created once, but a pid is only a NAME for it, and between the
/// moment a caller is told a pid and the moment this reads its token the name
/// can be reassigned. The creation time is therefore read BY NAME, before and
/// after the token read, and a change means the token belongs to a different
/// process than the one the caller asked about — refused, never returned.
///
/// That does not make the answer atomic: the two `start_time` reads and the
/// token read are three separate opens, and a reuse that happened to land
/// wholly inside a window between them would be invisible. What it removes is
/// the case that matters — a pid that died and was reused is never reported
/// as the old process's user, because one of the two time reads lands on the
/// other side of the reuse and the pair disagrees. A caller needing a
/// stronger proof needs a handle held across the whole read; nothing in this
/// core holds process handles, and this is the honest limit of a pid-based
/// lookup.
fn sid_of_pid(pid: u32) -> io::Result<String> {
    sid_of_pid_with(pid, start_time)
}

/// [`sid_of_pid`] with the creation-time reads INJECTED — the production pair
/// (`start_time` before the token read, `start_time` after it) is what this is
/// called with, and everything else is the same body. The seam exists because
/// the refusal below cannot be reached by waiting: it needs a pid whose
/// creation time MOVED between two reads microseconds apart, and no test can
/// arrange for a real process to die and have its name reused inside that
/// window. Injecting the reads is the only way the branch is exercised at all,
/// and the token read it brackets stays the real one.
fn sid_of_pid_with(pid: u32, mut read_start_time: impl FnMut(u32) -> Option<u64>) -> io::Result<String> {
    let before = read_start_time(pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no process with pid {pid}")))?;
    let sid = crate::owner_only::TokenUserSid::of_process(pid)?.to_string_sid()?;
    let after = read_start_time(pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no process with pid {pid}")))?;
    if before != after {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("pid {pid} was reused between the two reads: the token read is not this process's"),
        ));
    }
    Ok(sid)
}

/// The account behind a SID, as `DOMAIN\name` — the spelling `whoami` uses —
/// or an error when the SID names no account this host can resolve. This is
/// DISPLAY DATA (`aoide_secrets::peercred::username_for`'s origin line), so a
/// failure is reported, never guessed at: no name is invented for a SID that
/// has none.
///
/// The SID arrives as the string [`process_user_sid`] produced, because that
/// string is the only form an identity is carried in; converting it back is
/// what `LookupAccountSidW` needs, and the SID it allocates is freed here.
pub fn account_name_of_sid(sid: &str) -> io::Result<String> {
    let mut text: Vec<u16> = sid.encode_utf16().collect();
    text.push(0);
    let mut raw: PSID = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // The SID is `LocalAlloc`'d by the call above, so it is freed by the
    // guard, on the error path too.
    let owned = OwnedLocal(raw);

    let mut name = vec![0u16; 256];
    let mut domain = vec![0u16; 256];
    let mut name_len = name.len() as u32;
    let mut domain_len = domain.len() as u32;
    let mut kind = SidTypeUnknown;
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            owned.0,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    if name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("the SID {sid} resolves to no name")));
    }
    Ok(if domain.is_empty() { name } else { format!("{domain}\\{name}") })
}

/// A `PSID` allocated by the SID helpers, released on the way out — the same
/// shape `owner_only`'s allocator guards have, for the same reason: an early
/// return must not leak it.
struct OwnedLocal(PSID);

impl Drop for OwnedLocal {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0 as HLOCAL) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_pid_is_live_and_is_its_own_first_ancestor() {
        let me = std::process::id();
        assert!(is_alive(me));
        let chain = parent_chain(me, 64);
        assert_eq!(chain.first(), Some(&me), "the walk is self-first");
        assert!(chain.len() <= 64, "the walk is bounded");
        assert!(chain.len() > 1, "a test process is started by something: {chain:?}");
    }

    #[test]
    fn a_pid_that_cannot_name_one_process_is_never_live() {
        // `0` is Windows' Idle pseudo-process, and `u32::MAX` is far above
        // any real pid — the same two refusals the Unix arm makes before its
        // own syscall.
        assert!(!is_alive(0));
        assert!(!is_alive(u32::MAX));
        assert!(start_time(u32::MAX).is_none());
    }

    #[test]
    fn our_own_start_time_is_a_nonzero_creation_time_that_repeats() {
        let me = std::process::id();
        let first = start_time(me).expect("a live pid has a creation time");
        assert!(first > 0, "a real process's creation FILETIME is never zero");
        assert_eq!(start_time(me), Some(first), "the same process reports the same creation time");
    }

    #[test]
    fn the_table_carries_our_own_row_with_a_parent_and_an_exe_name() {
        let me = std::process::id();
        let table = processes().expect("a snapshot of the process table");
        let row = table.iter().find(|p| p.pid == me).expect("our own pid is in the table");
        assert!(row.parent > 0, "{}'s parent is a real pid, got {}", me, row.parent);
        assert!(table.iter().any(|p| p.pid == row.parent), "the parent is a row in the same table");
        assert!(!row.exe.is_empty(), "a process table row names its executable");
    }

    // ── the user a pid's token is for ────────────────────────────────────

    /// The assertion a constant, a `None`, or a never-run arm cannot pass:
    /// the string has to be a SID, and the SID this process names for itself
    /// has to be the same one a caller reading it by PID gets.
    #[test]
    fn this_process_names_one_sid_for_itself_by_token_and_by_pid() {
        let mine = current_user_sid().expect("this process's own token user");
        assert!(mine.starts_with("S-1-"), "a token user is a SID, not a name or a number: {mine}");
        assert_eq!(process_user_sid(std::process::id()).expect("read by pid"), mine);
    }

    #[test]
    fn a_child_process_runs_as_the_same_user_and_a_bogus_pid_has_no_user() {
        let mut child = std::process::Command::new("cmd")
            .arg("/C")
            .arg("exit 0")
            .spawn()
            .expect("spawn a short-lived child");
        let sid = process_user_sid(child.id()).expect("a live child's token user");
        assert_eq!(sid, current_user_sid().expect("ours"), "a child inherits its parent's token user");
        let _ = child.wait();

        assert_eq!(process_user_sid(0).expect_err("pid 0 is refused").kind(), io::ErrorKind::InvalidInput);
        assert!(process_user_sid(u32::MAX).is_err(), "a pid above the real range names nobody");
    }

    /// The REFUSAL branch, which a happy path, a constant or a `None` cannot
    /// cover: a pid whose creation time moved between the two reads is never
    /// reported as the process the caller asked about. Driven through the
    /// injected reads (see [`sid_of_pid_with`]) — no test can make a real
    /// process die and be renamed inside that window — and the agreeing pair
    /// asserted alongside it so the seam cannot pass by refusing everything.
    #[test]
    fn a_pid_that_changes_identity_between_the_two_reads_is_refused() {
        let me = std::process::id();
        let mut call = 0;
        let err = super::sid_of_pid_with(me, |_| {
            call += 1;
            Some(if call == 1 { 1 } else { 2 })
        })
        .expect_err("a changed creation time must refuse, not return a SID");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("reused"), "{err}");

        assert_eq!(
            super::sid_of_pid_with(me, |_| Some(7)).expect("two agreeing reads"),
            current_user_sid().expect("this process's own token user")
        );
    }

    /// A peer that EXITED before the read — and the answer here is NOT the
    /// Unix one: MEASURED on this host, a just-reaped pid still resolves
    /// (`OpenProcess` on the exiting object succeeds long enough to name its
    /// token user), so "the peer is gone" is not a refusal a pid-based lookup
    /// gets for free. The case is still worth pinning, because what a reaped
    /// pid must never do is name a DIFFERENT user than the one that process
    /// ran as — that is the pid-reuse hazard, and the two bracketing
    /// creation-time reads (`sid_of_pid_with`) are what close it rather than
    /// this function's pid check. A pid that never existed is the refusal that
    /// does fire, asserted in `a_child_process_runs_as_the_same_user...`.
    #[test]
    fn a_peer_that_exits_before_the_read_names_our_own_user_or_nothing() {
        let mut child = std::process::Command::new("cmd")
            .arg("/C")
            .arg("exit 0")
            .spawn()
            .expect("spawn a short-lived child");
        let pid = child.id();
        let _ = child.wait();

        match process_user_sid(pid) {
            Ok(sid) => assert_eq!(
                sid,
                current_user_sid().expect("ours"),
                "a reaped pid {pid} must never name somebody else's user"
            ),
            Err(_) => {}
        }
    }

    // ── a pid's command line ─────────────────────────────────────────────

    /// The contract, against a child spawned with a line this test knows
    /// exactly: the arguments come back in order, whole, and unmangled —
    /// including an argument with spaces and one that would be a quote
    /// character to a naive splitter. A constant, an empty vec or a
    /// never-run arm cannot pass it.
    ///
    /// The child is `cmd` with a held-open stdin: MEASURED on this host, `cmd`
    /// with arguments but no `/C` ignores them, prints its banner and waits on
    /// stdin, so it is a live process whose line this test chose — the same
    /// shape `cmd /C exit 0` cannot be, since that one is gone before the read.
    #[test]
    fn a_childs_command_line_reads_back_as_the_arguments_it_was_given() {
        let mut child = std::process::Command::new("cmd")
            // The marker is what the assertion hunts for; the two arguments
            // around it are the interesting shapes.
            .args(["aoide-argvo-marker", "two words", "1234:127.0.0.1:8710"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn a child that waits on stdin");
        let held = child.stdin.take().expect("the piped stdin this test holds open");
        let pid = child.id();
        let args = command_argv(pid).expect("a live child's command line");

        assert!(args.len() >= 4, "a spawned line keeps its arguments: {args:?}");
        assert_eq!(args[args.len() - 3], "aoide-argvo-marker", "{args:?}");
        assert_eq!(args[args.len() - 2], "two words", "an argument with a space stays one argument: {args:?}");
        assert_eq!(args[args.len() - 1], "1234:127.0.0.1:8710", "{args:?}");
        let head = args[0].to_ascii_lowercase();
        assert!(head.ends_with("cmd.exe") || head.ends_with("cmd"), "argv[0] names the program: {args:?}");

        drop(held);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn a_pid_that_cannot_name_one_process_has_no_command_line() {
        assert_eq!(
            command_argv(0).expect_err("pid 0 is refused").kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(command_argv(u32::MAX).is_err(), "a pid above the real range names no process");
    }

    // ── ending a process, and waiting for it ─────────────────────────────

    /// `terminate` then a blocking wait: the process is gone, and the wait
    /// that would have found it running does not. The exit is not a signal —
    /// there is none to send — so what this pins is the verdict trail, not a
    /// politeness nobody can ask for on this host.
    #[test]
    fn terminate_ends_a_child_and_the_wait_reports_exactly_that() {
        let mut child = std::process::Command::new("cmd")
            .arg("/C")
            .arg("ping -n 60 127.0.0.1 > NUL")
            .spawn()
            .expect("spawn a long-lived child");
        let pid = child.id();

        assert_eq!(wait_for_exit(pid, false), Waited::Running, "a spawned child is still running while its work is");

        terminate(pid).expect("terminate a child of this process");
        assert_eq!(wait_for_exit(pid, true), Waited::Exited, "after the kill the wait is satisfied");
        assert!(!is_alive(pid), "and the liveness probe agrees it has ended");
        let _ = child.wait();
    }

    #[test]
    fn a_pid_that_cannot_name_one_process_is_never_terminated_or_waited_on() {
        assert_eq!(
            terminate(0).expect_err("pid 0 is refused").kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(wait_for_exit(0, false), Waited::NotWaitable);
        assert_eq!(wait_for_exit(u32::MAX, false), Waited::NotWaitable);
    }
}
