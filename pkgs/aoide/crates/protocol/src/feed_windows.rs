//! The Windows half of [`super`]'s feed primitive: the two facts Unix takes
//! from the filesystem, answered against the object's own handle.
//!
//! **The owner-only policy itself lives in [`crate::owner_only`]**, shared
//! with `aoide-storage`'s private-write half (`pkgs/aoide/crates/AGENTS.md`:
//! no cross-crate copying — one implementation, exposed from the leaf crate
//! both consumers already depend on). What this module keeps is the FEED's
//! use of it: create-if-absent ([`create_new`]), then validate an existing
//! file before touching it ([`open_existing`] + [`reject_reparse_point`] +
//! [`file_refusal`]), then the EOF append below.
//!
//! **`0o600` is the only supported creation mode here.** The group-shared
//! `0o640` (`aoide-secrets`' broker feed) has no Windows mapping in this
//! slice and a world-readable mode is not silently narrowed to owner-only:
//! both are refused by name, before any directory or file is touched.
//!
//! **An existing file is validated before it is used.** A successful open is
//! not evidence of privacy, so the handle is opened without truncation and
//! the object's own owner and DACL are read back from it: the DACL must be
//! present and `SE_DACL_PROTECTED`, non-empty, every ACE an
//! `ACCESS_ALLOWED_ACE_TYPE` for that same token user (a deny, audit or
//! foreign-trustee ACE refuses), with the union of the masks covering the
//! owner's own read/write bits. Only then may anything seek, truncate or
//! write. A reparse point is refused outright: this module will not write
//! through a link whose target it cannot name.
//!
//! **Identity** cannot be `(dev, ino)`: [`file_identity`] reads the 128-bit
//! `FILE_ID_INFO` through the handle (`FileIdInfo`, correct on ReFS too),
//! with `GetFileInformationByHandle` as a documented fallback on
//! `ERROR_INVALID_PARAMETER` only. A failure propagates, never "same file".
//!
//! **The append is the documented EOF write, not a file-pointer dance.** A
//! Windows handle that also needs `FILE_WRITE_DATA` (for the past-cap
//! truncate) honours the file pointer, so `seek(EOF)` + `write_all` would
//! lose the positioning Unix gets from `O_APPEND`. [`write_at_eof`] instead
//! writes through `WriteFile` with `Offset`/`OffsetHigh` both
//! `0xFFFFFFFF`, whose documented meaning is "the end of file at write
//! time"; the truncate is `SetFilePointerEx(0)` + `SetEndOfFile` on the
//! same validated handle, and the following append needs no seek. Both are
//! on ONE handle, so nothing revalidates a path. What this does NOT yet
//! prove is cross-writer atomicity under concurrent appenders — that needs
//! the native runtime stress test, and is recorded as a residual difference
//! rather than claimed as Unix `O_APPEND` equivalence.

use super::owner_only_mode;
use crate::owner_only::{
    APPEND_ACCESS, CreateError, create_new, file_refusal, last_error, open_existing, reject_reparse_point, wide,
};
use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_PARAMETER, ERROR_PATH_NOT_FOUND, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_ID_INFO, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFileSizeEx, OPEN_EXISTING, SYNCHRONIZE, SetEndOfFile, SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::IO::{OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0};

// ── identity ─────────────────────────────────────────────────────────────

/// What identifies one file on Windows. The 128-bit form is the primary
/// answer (it is the one that is correct on ReFS); the legacy
/// `(volume, index)` form is only produced where `FileIdInfo` is not
/// supported, and the two are never equal to each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FileId {
    Full { volume: u64, id: [u8; 16] },
    Legacy { volume: u32, index: u64 },
}

/// Identity of an already-open handle. A failure is an error, never a
/// "probably the same file".
pub(super) fn file_identity(file: &File) -> io::Result<FileId> {
    let handle = file.as_raw_handle() as HANDLE;
    let mut info = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            &mut info as *mut FILE_ID_INFO as *mut c_void,
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } != 0
    {
        return Ok(FileId::Full { volume: info.VolumeSerialNumber, id: info.FileId.Identifier });
    }
    let error = last_error();
    // Documented fallback, on the one error that means "this filesystem has
    // no FileIdInfo" — any other failure is real and propagates.
    if error.raw_os_error() != Some(ERROR_INVALID_PARAMETER as i32) {
        return Err(error);
    }
    let mut legacy = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(handle, &mut legacy) } == 0 {
        return Err(last_error());
    }
    Ok(FileId::Legacy {
        volume: legacy.dwVolumeSerialNumber,
        index: ((legacy.nFileIndexHigh as u64) << 32) | legacy.nFileIndexLow as u64,
    })
}

/// Identity of the file a PATH currently names — the other side of
/// [`super::Follower::poll`]'s comparison. Opened with maximally permissive
/// sharing so probing never blocks the writer, and mapped to `NotFound` for
/// the two codes that mean the path is not there, which is what the Unix
/// side's `std::fs::metadata` already reports for a missing path.
pub(super) fn path_identity(path: &Path) -> io::Result<FileId> {
    let wide_path = wide(path)?;
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = last_error();
        return Err(match error.raw_os_error() {
            Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32 => {
                io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND as i32)
            }
            _ => error,
        });
    }
    let file = unsafe { File::from_raw_handle(handle as RawHandle) };
    file_identity(&file)
}

// ── the append itself ────────────────────────────────────────────────────

/// Write at the CURRENT end of file, positioned by the kernel at write time:
/// `WriteFile` with both `Offset` and `OffsetHigh` set to `0xFFFFFFFF` is
/// Microsoft's documented EOF form — "functionally equivalent to previously
/// calling CreateFile with FILE_APPEND_DATA" — so there is no
/// `SetFilePointerEx(FILE_END)` + write pair whose window loses append
/// positioning to a concurrent writer. `write_all` cannot express this (it
/// passes a null `lpOverlapped`, i.e. the file-pointer form).
///
/// Success means the requested bytes were written: a `TRUE` with a short
/// count, or a `FALSE`, is an error to narrate — never a partial write to
/// loop over (looping would restart the record at a new EOF).
fn write_at_eof(file: &File, buf: &[u8]) -> io::Result<()> {
    let Ok(length) = u32::try_from(buf.len()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the feed line is longer than a Win32 write"));
    };
    let mut overlapped = OVERLAPPED {
        Anonymous: OVERLAPPED_0 { Anonymous: OVERLAPPED_0_0 { Offset: u32::MAX, OffsetHigh: u32::MAX } },
        ..Default::default()
    };
    let mut written: u32 = 0;
    if unsafe {
        WriteFile(file.as_raw_handle() as HANDLE, buf.as_ptr(), length, &mut written, &mut overlapped)
    } == 0
    {
        return Err(last_error());
    }
    if written != length {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            format!("a Win32 write at the end of the feed reported {written} of {length} bytes"),
        ));
    }
    Ok(())
}

/// Empty the file through the validated handle: `SetEndOfFile` sets the
/// physical size to the CURRENT POINTER, so the pointer is moved to 0 first.
/// The following append is EOF-positioned, so nothing has to seek back.
fn truncate_to_zero(file: &File) -> io::Result<()> {
    let handle = file.as_raw_handle() as HANDLE;
    if unsafe { SetFilePointerEx(handle, 0, null_mut(), FILE_BEGIN) } == 0 {
        return Err(last_error());
    }
    if unsafe { SetEndOfFile(handle) } == 0 {
        return Err(last_error());
    }
    Ok(())
}

/// One Windows append: refuse an unsupported mode before anything is
/// touched, create with the policy attached, or validate the existing
/// object and only then truncate/write. Every outcome is a message for the
/// caller's `eprintln!` — the same best-effort posture as Unix, minus the
/// silent branches.
pub(super) fn append(path: &Path, cap: u64, create_mode: u32, line: &[u8]) -> Result<(), String> {
    if !owner_only_mode(create_mode) {
        return Err(format!(
            "refusing create_mode {:o}: native Windows supports only 0o600 (a protected owner-only DACL) — \
             this host has no group mapping for the broker's group-shared feed, so {:#o} is unavailable, not narrowed",
            create_mode, create_mode
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err(format!("could not create the feed directory: {e}"));
            }
        }
    }

    let file = match create_new(path) {
        Ok(file) => file,
        Err(CreateError::Exists) => return append_to_existing(path, cap, line),
        Err(CreateError::Failed(e)) => return Err(format!("could not create the feed file: {e}")),
    };
    // The descriptor attached at creation is a REQUEST: a filesystem that
    // does not persist ACLs may quietly ignore it. So the object it just
    // made is read back through the same verification an existing file gets,
    // and a feed whose own policy is not honored is refused BEFORE the first
    // payload byte is written — no repair step, no tightening afterwards.
    match file_refusal(&file) {
        Ok(None) => {}
        Ok(Some(reason)) => {
            return Err(format!(
                "refusing the feed file this process just created: {reason} \
                 (this filesystem may not persist ACLs, and this module will not write a feed it cannot prove is owner-only)"
            ))
        }
        Err(e) => return Err(format!("could not read back the created feed file's security: {e}")),
    }
    write_at_eof(&file, line).map_err(|e| format!("could not write the feed file: {e}"))
}

/// The existing-file branch: validate first, and only a validated handle may
/// be seeked, truncated, or written to. A refusal here has touched nothing.
fn append_to_existing(path: &Path, cap: u64, line: &[u8]) -> Result<(), String> {
    let file = open_existing(path, APPEND_ACCESS, false).map_err(|e| format!("could not open the feed file: {e}"))?;
    match reject_reparse_point(&file) {
        Ok(None) => {}
        Ok(Some(reason)) => return Err(format!("refusing the feed file: {reason}")),
        Err(e) => return Err(format!("could not read the feed file's attributes: {e}")),
    }
    match file_refusal(&file) {
        Ok(None) => {}
        Ok(Some(reason)) => return Err(format!("refusing the feed file: {reason}")),
        Err(e) => return Err(format!("could not read the feed file's security: {e}")),
    }

    let mut size: i64 = 0;
    if unsafe { GetFileSizeEx(file.as_raw_handle() as HANDLE, &mut size) } == 0 {
        return Err(format!("could not size the feed file: {}", io::Error::last_os_error()));
    }
    if size as u64 >= cap {
        // Past the cap: empty the file through the same validated handle —
        // the truncate-in-place the Unix half does with O_TRUNC, after the
        // validation that just passed. The append below is EOF-positioned,
        // so nothing seeks back afterwards.
        truncate_to_zero(&file).map_err(|e| format!("could not truncate the feed file: {e}"))?;
    }
    write_at_eof(&file, line).map_err(|e| format!("could not write the feed file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner_only::{LocalAlloc, REQUIRED_ACCESS, security_facts, wide};
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, GENERIC_WRITE};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, GRANT_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        ACL, AllocateAndInitializeSid, FreeSid, GetLengthSid, InitializeSecurityDescriptor, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_WORLD_SID_AUTHORITY, SetSecurityDescriptorDacl,
    };
    use windows_sys::Win32::Storage::FileSystem::CREATE_NEW;
    use windows_sys::Win32::System::SystemServices::{SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID};

    fn tmp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("aoide-feed-win-{tag}-{}", std::process::id()))
    }

    /// The well-known "Everyone" SID, S-1-1-0 (`SECURITY_WORLD_SID_AUTHORITY`
    /// plus `SECURITY_WORLD_RID` — the NT authority would name S-1-5-0
    /// instead), copied out of the allocation `AllocateAndInitializeSid`
    /// makes and into an 8-byte-aligned buffer of our own, so the trustee
    /// outlives `FreeSid`. It is never the current token user, so a DACL
    /// holding only it must be refused.
    fn everyone_sid() -> Vec<u64> {
        let authority = SECURITY_WORLD_SID_AUTHORITY;
        let mut sid: PSID = null_mut();
        assert_ne!(
            unsafe {
                AllocateAndInitializeSid(
                    &authority,
                    1,
                    SECURITY_WORLD_RID as u32,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    &mut sid,
                )
            },
            0,
            "AllocateAndInitializeSid: {}",
            io::Error::last_os_error()
        );
        let length = unsafe { GetLengthSid(sid) } as usize;
        assert!(length > 0);
        let mut buffer = vec![0u64; length.div_ceil(8)];
        unsafe { std::ptr::copy_nonoverlapping(sid as *const u8, buffer.as_mut_ptr() as *mut u8, length) };
        unsafe { FreeSid(sid) };
        buffer
    }

    /// An allow entry for `sid` — the only ACE shape a test needs to build.
    fn allow_entry(permissions: u32, sid: PSID) -> EXPLICIT_ACCESS_W {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: permissions,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: sid as *mut u16,
                ..Default::default()
            },
        }
    }

    /// Create `path` with a DACL built from `entries` (an empty slice means
    /// an explicitly NULL, grant-all DACL), and no protection flag — the
    /// shape an ordinary file written by another program has.
    fn create_with_dacl(path: &Path, entries: &[EXPLICIT_ACCESS_W]) {
        let mut sd = SECURITY_DESCRIPTOR::default();
        let descriptor = &mut sd as *mut SECURITY_DESCRIPTOR as PSECURITY_DESCRIPTOR;
        assert_ne!(unsafe { InitializeSecurityDescriptor(descriptor, SECURITY_DESCRIPTOR_REVISION) }, 0);
        let acl_guard = if entries.is_empty() {
            // Present but NULL: grant-all, the descriptor that must never be
            // accepted as owner-only.
            assert_ne!(unsafe { SetSecurityDescriptorDacl(descriptor, 1, null(), 0) }, 0);
            LocalAlloc(null_mut())
        } else {
            let mut acl: *mut ACL = null_mut();
            let status = unsafe { SetEntriesInAclW(entries.len() as u32, entries.as_ptr(), null(), &mut acl) };
            assert_eq!(status, ERROR_SUCCESS, "SetEntriesInAclW: {}", io::Error::from_raw_os_error(status as i32));
            let guard = LocalAlloc(acl as *mut c_void);
            assert_ne!(unsafe { SetSecurityDescriptorDacl(descriptor, 1, acl as *const ACL, 0) }, 0);
            guard
        };
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: &mut sd as *mut SECURITY_DESCRIPTOR as *mut c_void,
            bInheritHandle: 0,
        };
        let wide_path = wide(path).unwrap();
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE, "CreateFileW: {}", io::Error::last_os_error());
        unsafe { CloseHandle(handle) };
        drop(acl_guard);
    }

    #[test]
    fn a_created_feed_reopens_for_reading_and_for_a_validated_second_append() {
        let path = tmp("reopen");
        let _ = std::fs::remove_file(&path);
        append(&path, 4096, 0o600, b"one\n").expect("create");
        // The follower's own read path: the creation DACL must cover the
        // read bits too, or the feed is unreadable by the process that wrote
        // it.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\n");
        // And the validated-existing branch reopens it for a second append.
        append(&path, 4096, 0o600, b"two\n").expect("validated second append");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_this_module_created_reads_back_as_a_protected_owner_only_dacl() {
        let path = tmp("created");
        let _ = std::fs::remove_file(&path);
        append(&path, 1024, 0o600, b"{\"event\":\"x\"}\n").expect("a fresh 0o600 append must succeed");
        let file = File::open(&path).unwrap();
        let facts = security_facts(&file).unwrap();
        assert!(facts.dacl_present, "the created file must carry a DACL");
        assert!(facts.dacl_protected, "the created DACL must be SE_DACL_PROTECTED");
        assert_eq!(facts.ace_count, 1, "exactly the one owner-only ACE");
        assert!(facts.all_aces_are_current_user_allows, "the only ACE must be an allow for the current user");
        assert!(facts.owner_is_current_user, "the owner must be the current user");
        assert_eq!(facts.covered_mask & REQUIRED_ACCESS, REQUIRED_ACCESS);
        assert!(
            facts.covers(REQUIRED_ACCESS),
            "the mask the refusal reader requires is the one an owner-only file covers"
        );
        drop(file);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_unsupported_mode_is_refused_before_anything_on_disk_is_touched() {
        let dir = std::env::temp_dir().join(format!("aoide-feed-win-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for mode in [0o640, 0o644, 0o400, 0o700] {
            let path = dir.join(format!("feed-{mode:o}"));
            let error = append(&path, 1024, mode, b"{\"event\":\"x\"}\n").expect_err("an unsupported mode must refuse");
            assert!(error.contains("0o600"), "the refusal must name the one supported mode: {error}");
            assert!(!path.exists(), "mode {mode:o} must not create the file");
        }
        assert!(!dir.exists(), "an unsupported mode must not even create the directory");
    }

    #[test]
    fn an_inherited_dacl_refuses_without_writing_or_truncating() {
        let path = tmp("inherited");
        let _ = std::fs::remove_file(&path);
        // An ordinary file, created the ordinary way: its DACL is inherited,
        // so it is neither protected nor owner-only.
        std::fs::write(&path, b"x").unwrap();
        let before = std::fs::read(&path).unwrap();
        // cap 0 would truncate if validation were skipped: the refusal must
        // leave the bytes exactly as they were.
        let error = append(&path, 0, 0o600, b"{\"event\":\"x\"}\n").expect_err("an inherited DACL must refuse");
        assert!(error.contains("refusing the feed file"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "a refusal must not write or truncate");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_null_dacl_refuses_without_writing() {
        let path = tmp("null-dacl");
        let _ = std::fs::remove_file(&path);
        create_with_dacl(&path, &[]);
        let before = std::fs::read(&path).unwrap();
        let error = append(&path, 0, 0o600, b"{\"event\":\"x\"}\n").expect_err("a null DACL must refuse");
        assert!(error.contains("refusing the feed file"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "a refusal must not write");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_foreign_trustee_refuses_even_when_the_mask_covers() {
        let path = tmp("foreign");
        let _ = std::fs::remove_file(&path);
        let sid = everyone_sid();
        create_with_dacl(&path, &[allow_entry(REQUIRED_ACCESS, sid.as_ptr() as PSID)]);
        let before = std::fs::read(&path).unwrap();
        let error = append(&path, 0, 0o600, b"{\"event\":\"x\"}\n").expect_err("a foreign trustee must refuse");
        assert!(error.contains("refusing the feed file"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "a refusal must not write or truncate");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_validated_file_appends_at_the_end_and_truncates_only_past_the_cap() {
        let path = tmp("append");
        let _ = std::fs::remove_file(&path);
        append(&path, 4096, 0o600, b"first\n").expect("first append");
        // Under the cap: the next line lands AFTER the existing bytes.
        append(&path, 4096, 0o600, b"second\n").expect("second append");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        // Past the cap: the validated handle is truncated, then written.
        append(&path, 4, 0o600, b"third\n").expect("over-cap append");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third\n");
        std::fs::remove_file(&path).ok();
    }
}
