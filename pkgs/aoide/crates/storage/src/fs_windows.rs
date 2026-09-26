//! The Windows half of [`super`]'s FS substrate — the two primitives whose
//! Unix shape has no Windows equivalent, kept in one sibling file the way
//! `aoide-protocol`'s `feed_windows.rs` sits beside `feed.rs`.
//!
//! | concern | Unix | here |
//! | --- | --- | --- |
//! | the stage/state lock | `flock(LOCK_EX)` | `LockFileEx` on the same file |
//! | the no-clobber seed | `renameat2(RENAME_NOREPLACE)` | `MoveFileExW` *without* `MOVEFILE_REPLACE_EXISTING` |
//! | preserving a link | `fs::symlink` | `symlink_dir`/`symlink_file`, kind from the target |
//!
//! **What the Windows lock does and does not guarantee.** `flock` is an
//! advisory lock on an OPEN FILE DESCRIPTION, so it survives a `fork`/
//! `setsid` into a detached child — a property Windows has no fork to need,
//! and therefore no promise to keep. What `LockFileEx` gives instead: the
//! lock belongs to the HANDLE, so a second handle in the same process blocks
//! exactly as a second process's does (Windows locks are per-handle, not
//! per-process) — which is what [`super::with_stage_lock`]'s re-entrancy flag
//! exists to survive on Unix and what makes it necessary here too. `flock`'s
//! advisory half is the ONE difference worth naming: a Windows byte-range
//! lock is MANDATORY, so another process that tried to read or write the
//! locked bytes would fail rather than proceed. Nothing here ever reads or
//! writes a lock file's contents, so no caller can observe the difference —
//! and the file's own existence, not its bytes, is what the lock file is for.
//!
//! Every entry point keeps its Unix caller's fail-closed shape: an error
//! means "not held", never "assume held".

use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_LOCK_VIOLATION, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, MoveFileExW, UnlockFileEx,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

// ── the lock ─────────────────────────────────────────────────────────────

/// The whole file, from offset 0: `flock` locks the open file description,
/// `LockFileEx` locks a byte RANGE, so "all of it" has to be spelled out.
/// Every lock and unlock in this module names the same range, because
/// `UnlockFileEx` releases only the range it is given.
const WHOLE_FILE_LOW: u32 = u32::MAX;
const WHOLE_FILE_HIGH: u32 = u32::MAX;

/// Take the exclusive lock, blocking until it is free — the shape
/// [`super::lock_path`] and `super::with_stage_lock` both need (a concurrent
/// opener waits rather than races).
pub(crate) fn lock_exclusive(file: &File) -> io::Result<()> {
    lock(file, LOCKFILE_EXCLUSIVE_LOCK)
}

/// Try the exclusive lock without waiting. `Ok(false)` is the ordinary
/// "another holder has it" answer — the same thing `LOCK_NB` reports as
/// `EWOULDBLOCK` — and is never an error. Only a genuine failure is `Err`.
pub(crate) fn try_lock_exclusive(file: &File) -> io::Result<bool> {
    match lock(file, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY) {
        Ok(()) => Ok(true),
        Err(e) if e.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Release the lock — tolerated on a handle that was never locked (the same
/// best-effort `LOCK_UN` a `Drop` guard issues on Unix).
pub(crate) fn unlock(file: &File) {
    let mut overlapped = OVERLAPPED::default();
    unsafe {
        UnlockFileEx(
            file.as_raw_handle() as HANDLE,
            0,
            WHOLE_FILE_LOW,
            WHOLE_FILE_HIGH,
            &mut overlapped,
        );
    }
}

fn lock(file: &File, flags: u32) -> io::Result<()> {
    let mut overlapped = OVERLAPPED::default();
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            flags,
            0,
            WHOLE_FILE_LOW,
            WHOLE_FILE_HIGH,
            &mut overlapped,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// ── the no-clobber rename ────────────────────────────────────────────────

/// Rename `from` → `to` ONLY if `to` does not already exist. `Ok(true)`
/// renamed, `Ok(false)` the target was already there (a concurrent writer
/// beat us), `Err` anything else — the same three answers, and the same
/// meaning, `renameat2(RENAME_NOREPLACE)` gives.
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<bool> {
    let wide_from = wide(from)?;
    let wide_to = wide(to)?;
    // MOVEFILE_REPLACE_EXISTING is deliberately NOT passed: its ABSENCE is
    // the no-clobber, exactly as RENAME_NOREPLACE's presence is on Unix.
    if unsafe { MoveFileExW(wide_from.as_ptr(), wide_to.as_ptr(), 0) } != 0 {
        return Ok(true);
    }
    match io::Error::last_os_error().raw_os_error() {
        Some(code) if code == ERROR_ALREADY_EXISTS as i32 || code == ERROR_FILE_EXISTS as i32 => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

// ── preserving a link ────────────────────────────────────────────────────

/// Create `link` as a symbolic link to `target`, the kind chosen from what
/// the SOURCE currently resolves to — Windows' `CreateSymbolicLinkW` has no
/// "unknown kind", so the choice cannot be deferred the way `symlink(2)`
/// defers it. A source that does not resolve (a dangling link) becomes a
/// FILE link: the link is still recorded as a link, never followed, and the
/// caller's own "never followed" contract is what the target's bytes matter
/// for. Unprivileged creation is an ordinary error here (Windows needs
/// SeCreateSymbolicLinkPrivilege or Developer Mode) — it propagates rather
/// than being papered over, because a copy that silently replaced a link
/// with a regular file would change what the tree means.
pub(crate) fn symlink(source: &Path, target: PathBuf, link: &Path) -> io::Result<()> {
    let is_dir = std::fs::metadata(source).map(|meta| meta.is_dir()).unwrap_or(false);
    if is_dir {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// A path as a NUL-terminated wide string, with an embedded NUL refused
/// rather than silently truncating the name `MoveFileExW` would act on.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut out: Vec<u16> = path.as_os_str().encode_wide().collect();
    if out.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the path contains an embedded NUL"));
    }
    out.push(0);
    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("aoide-fs-win-{tag}-{}", std::process::id()))
    }

    #[test]
    fn a_no_clobber_rename_moves_an_absent_target_and_leaves_a_present_one() {
        let dir = tmp("noreplace");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("from");
        let to = dir.join("to");
        std::fs::write(&from, "contents").unwrap();

        assert!(rename_no_replace(&from, &to).unwrap(), "an absent target is renamed onto");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "contents");
        assert!(!from.exists(), "the source is gone, not copied");

        // The target now exists: the no-clobber rename must refuse, leaving
        // BOTH files exactly as they were.
        std::fs::write(&from, "second").unwrap();
        assert!(!rename_no_replace(&from, &to).unwrap(), "a present target is never clobbered");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "contents", "the target is untouched");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "second", "the source is untouched");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_lock_is_held_against_a_second_handle_and_probed_without_waiting() {
        let path = tmp("lock");
        let _ = std::fs::remove_file(&path);
        let first = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&path).unwrap();
        let second = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&path).unwrap();

        // A second HANDLE — this process's own — is a second holder: Windows
        // locks belong to the handle, not the process.
        assert!(try_lock_exclusive(&first).unwrap(), "an unheld lock is taken");
        assert!(!try_lock_exclusive(&second).unwrap(), "a held lock answers 'not yours', never an error");

        unlock(&first);
        assert!(try_lock_exclusive(&second).unwrap(), "the release frees the file for the next holder");

        unlock(&second);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_blocking_lock_waits_for_the_holder_to_release() {
        let path = tmp("blocking");
        let _ = std::fs::remove_file(&path);
        let held = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&path).unwrap();
        lock_exclusive(&held).unwrap();

        let waiter_path = path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&waiter_path).unwrap();
            lock_exclusive(&file).unwrap();
            tx.send(()).unwrap();
            file
        });

        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(rx.try_recv().is_err(), "a blocking lock must not pass a held one");
        unlock(&held);
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(), "the release lets the waiter through");

        let _ = waiter.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
