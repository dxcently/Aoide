//! The native process table: the three facts Unix reads out of `/proc`,
//! answered by the APIs that own them on Windows.
//!
//! | fact | Unix | here |
//! | --- | --- | --- |
//! | a pid's parent | `/proc/<pid>/stat` field 4 | `PROCESSENTRY32W.th32ParentProcessID` |
//! | a pid's start time | `/proc/<pid>/stat` field 22 | `GetProcessTimes` creation `FILETIME` |
//! | is a pid live | `kill(pid, 0)` | `OpenProcess` + `GetExitCodeProcess` |
//!
//! Two callers, one implementation (`pkgs/aoide/crates/AGENTS.md`: no
//! cross-crate copying): `aoide_storage::attest`'s pid-ancestry walk and
//! pid-reuse defence, and `crate::dialog`'s locker probe. It lives in this
//! crate because `aoide-protocol` is the DAG leaf both of them already depend
//! on.
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
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NOT_FOUND, FILETIME, HANDLE, INVALID_HANDLE_VALUE,
    STILL_ACTIVE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

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
}
