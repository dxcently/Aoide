//! The Windows half of [`super`]'s feed primitive: the two facts Unix takes
//! from the filesystem, answered against the object's own handle.
//!
//! **Creation policy, attached AT creation.** Unix chmods after the open;
//! the Windows shape that merely looks equivalent — create with the
//! inherited default DACL, then tighten with `SetSecurityInfo` — is a race,
//! because another principal can open the brand-new file and keep the handle
//! in the window between the two steps. So [`super::FeedWriter::append`]
//! passes `SECURITY_ATTRIBUTES` whose `SECURITY_DESCRIPTOR` already holds an
//! EXPLICIT, `SE_DACL_PROTECTED`, single-ACE DACL for the current token user
//! ([`OWNER_ONLY_MASK`], `NO_INHERITANCE`), with the owner pinned to that
//! same user — nothing inheritable is applied, and there is no second step
//! to race. The owner is pinned rather than assumed because an elevated
//! token's DEFAULT owner is the Administrators group, and [`security_facts`]
//! asks for the current user.
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
//! foreign-trustee ACE refuses), with the union of the masks covering
//! [`REQUIRED_ACCESS`]. Only then may anything seek, truncate or write. A
//! reparse point is refused outright: this module will not write through a
//! link whose target it cannot name.
//!
//! **Identity** cannot be `(dev, ino)`: [`file_identity`] reads the 128-bit
//! `FILE_ID_INFO` through the handle (`FileIdInfo`, correct on ReFS too),
//! with `GetFileInformationByHandle` as a documented fallback on
//! `ERROR_INVALID_PARAMETER` only. A failure propagates, never "same file".
//!
//! Every resource — token, `SetEntriesInAclW`'s ACL, `GetSecurityInfo`'s
//! descriptor, the token buffer, each file handle — is owned by a guard, so
//! every error path releases it; the token buffer is a `Vec<u64>` (not a
//! misaligned `Vec<u8>` cast) so the `TOKEN_USER` read inside it is aligned.
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
use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INVALID_PARAMETER, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
    GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SE_FILE_OBJECT, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, EqualSid,
    GetAce, GetAclInformation, GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation,
    InitializeSecurityDescriptor, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    SECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_DACL_PROTECTED, SetSecurityDescriptorControl, SetSecurityDescriptorDacl,
    SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_BEGIN, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_READ_EA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, FileAttributeTagInfo, FileIdInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFileSizeEx, OPEN_EXISTING, READ_CONTROL, SYNCHRONIZE, SetEndOfFile,
    SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::IO::{OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0};
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, SECURITY_DESCRIPTOR_REVISION, SID_REVISION};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// What the one owner-only ACE grants: every bit `GENERIC_READ` and
/// `GENERIC_WRITE` resolve to on a file, so the object this module creates
/// can be reopened for reading (the [`super::Follower`], or any
/// `read_to_string`) and for the validated append — by this user and nobody
/// else. The bits are not decoration: the access check for a create runs
/// against the descriptor being attached, so a `GENERIC_WRITE` create whose
/// DACL omits `FILE_WRITE_EA`/`FILE_WRITE_ATTRIBUTES` fails, and a DACL
/// that omits the read bits is a feed its own follower cannot open.
const OWNER_ONLY_MASK: u32 =
    FILE_READ_DATA | FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_READ_EA | FILE_WRITE_EA | FILE_READ_ATTRIBUTES
        | FILE_WRITE_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE;

/// What the append handle asks for, every bit of it inside
/// [`OWNER_ONLY_MASK`] — so a file that validates always permits this open.
const APPEND_ACCESS: u32 =
    FILE_APPEND_DATA | FILE_WRITE_DATA | FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE;

/// What an existing file's DACL must cover before this module will append to
/// it: the whole [`OWNER_ONLY_MASK`], so a file this module created always
/// validates and a write-only file (one its own follower could not read)
/// never does.
const REQUIRED_ACCESS: u32 = OWNER_ONLY_MASK;

// ── guards: every allocation is released on every path ───────────────────

/// An `HLOCAL` allocation owned by `advapi32` (`SetEntriesInAclW`'s ACL,
/// `GetSecurityInfo`'s security descriptor). `LocalFree` tolerates a null
/// pointer, so the guard is safe to build before the call succeeds.
struct LocalAlloc(*mut c_void);

impl Drop for LocalAlloc {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0 as HLOCAL) };
        }
    }
}

/// A kernel handle from `CreateFileW`/`OpenProcessToken` that has not been
/// wrapped in a [`File`] yet — so an early return between the call and the
/// wrap still closes it.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

// ── the current token user, whose SID is the one trustee allowed ─────────

/// The SID of the user this process's token is for — `SID_AND_ATTRIBUTES.Sid`
/// out of `TokenUser`. The backing buffer is an 8-byte-aligned `Vec<u64>`
/// (`TOKEN_USER` needs pointer alignment; a `Vec<u8>` cast would be
/// misaligned), and it is owned here because `sid` points INTO it.
struct TokenUserSid {
    _buffer: Vec<u64>,
    sid: PSID,
}

impl TokenUserSid {
    fn current() -> io::Result<Self> {
        let mut raw: HANDLE = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Handle(raw);

        // The documented two-call shape: the required length first, then the
        // value. A failure at either step is propagated, never guessed past.
        let mut needed: u32 = 0;
        unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed) };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer: Vec<u64> = vec![0; needed.div_ceil(8) as usize];
        if unsafe {
            GetTokenInformation(token.0, TokenUser, buffer.as_mut_ptr() as *mut c_void, needed, &mut needed)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user: TOKEN_USER = unsafe { std::ptr::read_unaligned(buffer.as_ptr() as *const TOKEN_USER) };
        if user.User.Sid.is_null() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the process token names no user SID"));
        }
        Ok(Self { _buffer: buffer, sid: user.User.Sid })
    }

    fn as_ptr(&self) -> PSID {
        self.sid
    }
}

/// A path as a NUL-terminated wide string. An embedded NUL is refused
/// rather than truncated: `CreateFileW` takes a C string, so a path carrying
/// one would silently name a DIFFERENT file than the caller asked for.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut out: Vec<u16> = path.as_os_str().encode_wide().collect();
    if out.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the feed path contains an embedded NUL"));
    }
    out.push(0);
    Ok(out)
}

/// The last-error code of a failed call, as an `io::Error`.
fn last_error() -> io::Error {
    io::Error::last_os_error()
}

// ── creation with the policy already attached ────────────────────────────

/// Build the one creation policy into `sd`: an explicit, protected,
/// single-ACE owner-only DACL, with the owner pinned to `sid`. The returned
/// guard owns the ACL the descriptor points at, so it must outlive `sd`'s
/// use in `CreateFileW`.
fn owner_only_descriptor(sd: &mut SECURITY_DESCRIPTOR, sid: &TokenUserSid) -> io::Result<LocalAlloc> {
    let descriptor = sd as *mut SECURITY_DESCRIPTOR as PSECURITY_DESCRIPTOR;
    if unsafe { InitializeSecurityDescriptor(descriptor, SECURITY_DESCRIPTOR_REVISION) } == 0 {
        return Err(last_error());
    }
    // Pinning the owner is not decoration: an elevated token's DEFAULT owner
    // is the Administrators group, so a created file can legitimately belong
    // to someone other than its creator — and [`check_owner_only`] asks for
    // the current user. Setting it here makes create and verify agree by
    // construction, and setting the owner to one's OWN token user needs no
    // privilege.
    if unsafe { SetSecurityDescriptorOwner(descriptor, sid.as_ptr(), 0) } == 0 {
        return Err(last_error());
    }

    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: OWNER_ONLY_MASK,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: 0, // NO_INHERITANCE
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: sid.as_ptr() as *mut u16,
            ..Default::default()
        },
    };
    let mut acl: *mut ACL = null_mut();
    let status = unsafe { SetEntriesInAclW(1, &entry, null(), &mut acl) };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let acl = LocalAlloc(acl as *mut c_void);

    // `bDaclPresent = TRUE` with a non-null ACL is the EXPLICIT DACL;
    // `bDaclDefaulted = FALSE` says so out loud, so no mechanism later
    // substitutes the token's default DACL for it.
    if unsafe { SetSecurityDescriptorDacl(descriptor, 1, acl.0 as *const ACL, 0) } == 0 {
        return Err(last_error());
    }
    // SE_DACL_PROTECTED: nothing inheritable from the parent is ever applied,
    // which is what makes "attached at creation" as strict as it sounds.
    if unsafe { SetSecurityDescriptorControl(descriptor, SE_DACL_PROTECTED, SE_DACL_PROTECTED) } == 0 {
        return Err(last_error());
    }
    Ok(acl)
}

/// Why a `CREATE_NEW` did not hand back a fresh file — the two outcomes the
/// caller distinguishes, so `ERROR_FILE_EXISTS` can fall through to the
/// validate-an-existing-file branch instead of being reported as a failure.
enum CreateError {
    Exists,
    Failed(io::Error),
}

/// Create the feed file with the owner-only DACL already attached, or report
/// that it already exists. Nothing is tightened after the fact.
fn create_owner_only(path: &Path) -> Result<File, CreateError> {
    let sid = TokenUserSid::current().map_err(CreateError::Failed)?;
    let mut sd = SECURITY_DESCRIPTOR::default();
    let _acl = owner_only_descriptor(&mut sd, &sid).map_err(CreateError::Failed)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: &mut sd as *mut SECURITY_DESCRIPTOR as *mut c_void,
        bInheritHandle: 0,
    };

    let wide_path = wide(path).map_err(CreateError::Failed)?;
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_WRITE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    // `CreateFileW` reports failure as INVALID_HANDLE_VALUE, never as a
    // null-or-nonzero BOOL: the code is the comparison, and the error comes
    // from GetLastError.
    if handle == INVALID_HANDLE_VALUE {
        let error = last_error();
        return Err(match error.raw_os_error() {
            Some(code) if code == ERROR_FILE_EXISTS as i32 => CreateError::Exists,
            _ => CreateError::Failed(error),
        });
    }
    Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

// ── validating an existing file before it is used ────────────────────────

/// What the DACL and owner of one file actually are, read from the OBJECT's
/// own handle — never from the path, and never from a second open.
struct SecurityFacts {
    dacl_present: bool,
    dacl_protected: bool,
    ace_count: u32,
    /// Every ACE is an `ACCESS_ALLOWED_ACE_TYPE` for the current token user.
    all_aces_are_current_user_allows: bool,
    /// The union of the allow masks found.
    covered_mask: u32,
    owner_is_current_user: bool,
}

impl SecurityFacts {
    fn is_owner_only(&self) -> bool {
        self.dacl_present
            && self.dacl_protected
            && self.ace_count > 0
            && self.all_aces_are_current_user_allows
            && self.owner_is_current_user
            && self.covered_mask & REQUIRED_ACCESS == REQUIRED_ACCESS
    }
}

/// Read the facts above out of `file`'s handle. `GetSecurityInfo` returns a
/// `WIN32_ERROR` (zero is success), not a BOOL — its failure code is the
/// return value, never `GetLastError`.
fn security_facts(file: &File) -> io::Result<SecurityFacts> {
    let sid = TokenUserSid::current()?;
    let mut owner: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    // The descriptor (and the DACL inside it) is owned from here on, so the
    // pointers below stay valid for the whole read and are released on the
    // way out, error or not.
    let descriptor = LocalAlloc(descriptor);

    // A null DACL is grant-all access to everyone; an absent one is the
    // same. Neither is ever accepted, and neither reaches the ACE walk.
    if dacl.is_null() {
        return Ok(SecurityFacts {
            dacl_present: false,
            dacl_protected: false,
            ace_count: 0,
            all_aces_are_current_user_allows: false,
            covered_mask: 0,
            owner_is_current_user: false,
        });
    }

    let mut control: u16 = 0;
    let mut revision: u32 = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor.0 as PSECURITY_DESCRIPTOR, &mut control, &mut revision) } == 0 {
        return Err(last_error());
    }

    let mut info = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            dacl as *const ACL,
            &mut info as *mut ACL_SIZE_INFORMATION as *mut c_void,
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(last_error());
    }

    let mut all_aces_are_current_user_allows = true;
    let mut covered_mask: u32 = 0;
    for index in 0..info.AceCount {
        let mut ace: *mut c_void = null_mut();
        if unsafe { GetAce(dacl as *const ACL, index, &mut ace) } == 0 {
            return Err(last_error());
        }
        // The ACE TYPE and FLAGS are checked BEFORE anything in the ACE's
        // variable tail is touched: only an explicit, non-inherited allow
        // ACE even has a SID where this reader would look. `AceFlags != 0`
        // rejects an inherited or container-propagating ACE, which is what
        // an explicit single-trustee policy never carries.
        let header: ACE_HEADER = unsafe { std::ptr::read_unaligned(ace as *const ACE_HEADER) };
        let sid_offset = std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart) as u32;
        if header.AceType as u32 != ACCESS_ALLOWED_ACE_TYPE
            || header.AceFlags != 0
            || (header.AceSize as u32) < sid_offset + 8
        {
            all_aces_are_current_user_allows = false;
            continue;
        }
        let allowed = ace as *const ACCESS_ALLOWED_ACE;
        let mask = unsafe { std::ptr::read_unaligned(std::ptr::addr_of!((*allowed).Mask)) };
        let trustee: PSID = unsafe { std::ptr::addr_of!((*allowed).SidStart) as PSID };
        // A SID is a counted structure: validate it against its own header
        // (revision, sub-authority count) before comparing, and inside this
        // ACE's `AceSize`. Only the first two bytes are read here — the rest
        // is never copied, and no read happens past the ACE.
        let sid_header: [u8; 2] = unsafe { std::ptr::read_unaligned(trustee as *const [u8; 2]) };
        let sub_authorities = sid_header[1] as u32;
        let sid_bytes = 8 + 4 * sub_authorities;
        if sid_header[0] as u32 != SID_REVISION
            || sub_authorities > 15
            || sid_offset + sid_bytes > header.AceSize as u32
            || unsafe { GetLengthSid(trustee) } != sid_bytes
        {
            all_aces_are_current_user_allows = false;
            continue;
        }
        if unsafe { EqualSid(trustee, sid.as_ptr()) } == 0 {
            all_aces_are_current_user_allows = false;
            continue;
        }
        covered_mask |= mask;
    }

    Ok(SecurityFacts {
        dacl_present: control & SE_DACL_PRESENT != 0,
        dacl_protected: control & SE_DACL_PROTECTED != 0,
        ace_count: info.AceCount,
        all_aces_are_current_user_allows,
        covered_mask,
        owner_is_current_user: !owner.is_null() && unsafe { EqualSid(owner, sid.as_ptr()) } != 0,
    })
}

/// The named refusal when an existing file is not the owner-only object this
/// module is willing to append to. `None` means it is, and only then may a
/// caller seek, truncate, or write.
fn owner_only_refusal(file: &File) -> io::Result<Option<String>> {
    let facts = security_facts(file)?;
    if facts.is_owner_only() {
        return Ok(None);
    }
    let reason = if !facts.dacl_present {
        "its DACL is absent or null (grant-all), which is never accepted".to_string()
    } else if !facts.dacl_protected {
        "its DACL is not SE_DACL_PROTECTED, so inherited ACEs apply".to_string()
    } else if facts.ace_count == 0 {
        "its DACL is empty".to_string()
    } else if !facts.all_aces_are_current_user_allows {
        "its DACL grants someone other than the current user (a foreign trustee, a deny ACE, or a non-allow ACE)".to_string()
    } else if !facts.owner_is_current_user {
        "its owner is not the current user".to_string()
    } else {
        format!(
            "its DACL mask {:#x} does not cover the append/write bits {:#x}",
            facts.covered_mask, REQUIRED_ACCESS
        )
    };
    Ok(Some(reason))
}

/// Open an existing feed file for the validated append: no truncation, no
/// directory creation, and `FILE_FLAG_OPEN_REPARSE_POINT` so a link at the
/// path is refused as itself instead of being silently followed.
fn open_existing(path: &Path) -> io::Result<File> {
    let wide_path = wide(path)?;
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            APPEND_ACCESS,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

/// Refuse a reparse point: this module validates the object it holds, and a
/// link's target is a different object than the link the path names.
fn reject_reparse_point(file: &File) -> io::Result<Option<String>> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileAttributeTagInfo,
            &mut info as *mut FILE_ATTRIBUTE_TAG_INFO as *mut c_void,
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    } == 0
    {
        return Err(last_error());
    }
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Ok(Some(format!("the feed path is a reparse point (tag {:#x})", info.ReparseTag)));
    }
    Ok(None)
}

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

    let file = match create_owner_only(path) {
        Ok(file) => file,
        Err(CreateError::Exists) => return append_to_existing(path, cap, line),
        Err(CreateError::Failed(e)) => return Err(format!("could not create the feed file: {e}")),
    };
    // The descriptor attached at creation is a REQUEST: a filesystem that
    // does not persist ACLs may quietly ignore it. So the object it just
    // made is read back through the same verification an existing file gets,
    // and a feed whose own policy is not honored is refused BEFORE the first
    // payload byte is written — no repair step, no tightening afterwards.
    match owner_only_refusal(&file) {
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
    let file = open_existing(path).map_err(|e| format!("could not open the feed file: {e}"))?;
    match reject_reparse_point(&file) {
        Ok(None) => {}
        Ok(Some(reason)) => return Err(format!("refusing the feed file: {reason}")),
        Err(e) => return Err(format!("could not read the feed file's attributes: {e}")),
    }
    match owner_only_refusal(&file) {
        Ok(None) => {}
        Ok(Some(reason)) => return Err(format!("refusing the feed file: {reason}")),
        Err(e) => return Err(format!("could not read the feed file's security: {e}")),
    }

    let mut size: i64 = 0;
    if unsafe { GetFileSizeEx(file.as_raw_handle() as HANDLE, &mut size) } == 0 {
        return Err(format!("could not size the feed file: {}", last_error()));
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
    use windows_sys::Win32::Security::{
        AllocateAndInitializeSid, FreeSid, SECURITY_WORLD_SID_AUTHORITY,
    };
    use windows_sys::Win32::System::SystemServices::SECURITY_WORLD_RID;

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
            last_error()
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
        assert_ne!(handle, INVALID_HANDLE_VALUE, "CreateFileW: {}", last_error());
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
        assert!(facts.is_owner_only());
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
