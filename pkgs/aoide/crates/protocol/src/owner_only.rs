//! The owner-only policy a native-Windows core attaches to a file or a
//! directory — one implementation, two consumers.
//!
//! Unix carries this policy in mode bits: `0o600` on a private file, `0o700`
//! on the directory that holds it. Native Windows has no mode bits; the
//! native shape is an explicit, `SE_DACL_PROTECTED`, single-ACE DACL for the
//! current token user, plus an owner pinned to that same user. This module is
//! that shape, and only that shape: it knows nothing about feeds, stage
//! files, or keypairs.
//!
//! **Policy is attached AT creation, never tightened afterwards.** The
//! Unix order — create with the process default, then `chmod` — is a race on
//! Windows: another principal can open the brand-new object and keep the
//! handle in the window between the two steps. So [`create_new`],
//! [`create_truncating`] and the directory half pass `SECURITY_ATTRIBUTES`
//! whose `SECURITY_DESCRIPTOR` already holds the ACL, and there is no second
//! step to race. The owner is pinned rather than assumed because an elevated
//! token's DEFAULT owner is the Administrators group, and every reader here
//! asks for the current user.
//!
//! **A creation is a request, not a fact.** A filesystem that does not
//! persist ACLs accepts the descriptor and ignores it, so every caller reads
//! the object back before the first payload byte — the feed through
//! [`file_refusal`] on the handle it just created, `aoide_storage::fs`'
//! private temp through [`file_privacy`] by path — and a link at either path
//! is refused AS ITSELF by the same readers ([`reject_reparse_point`]), never
//! read or written as the target. [`ensure_private_dir`] is the one place
//! that tightens after the fact, and it is only reached on a directory that
//! already exists (its alternative — refuse a directory another run left
//! world-readable — would regress the Unix contract, which locks an existing
//! directory down); a symbolic link or junction at that path is refused by
//! name, and the tightened directory's policy is read back before it returns.
//!
//! Who consumes it: `crate::feed`'s Windows writer (create-if-absent,
//! validate-before-append) and `aoide_storage::fs`'s private-write and
//! private-directory half. `pkgs/aoide/crates/AGENTS.md`'s "no cross-crate
//! copying" is why it lives here and is `pub`: `aoide-protocol` is the leaf
//! every domain crate already depends on, so exposing it adds no dependency
//! edge and creates no second copy.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_EXISTS, ERROR_SUCCESS, GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SE_FILE_OBJECT, SetEntriesInAclW, SetSecurityInfo, TRUSTEE_IS_SID,
    TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION,
    EqualSid, GetAce, GetAclInformation, GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetTokenInformation, InitializeSecurityDescriptor, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_DACL_PROTECTED,
    SetSecurityDescriptorControl, SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_READ_EA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_DELETE_CHILD, FILE_EXECUTE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, FileAttributeTagInfo, GetFileInformationByHandleEx,
    OPEN_EXISTING, READ_CONTROL, SYNCHRONIZE, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, SECURITY_DESCRIPTOR_REVISION, SID_REVISION};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// What the one owner-only ACE grants a file: every bit `GENERIC_READ` and
/// `GENERIC_WRITE` resolve to, so the object this module creates can be
/// reopened for reading (a follower, a `read_to_string`) and for a validated
/// append — by this user and nobody else. The bits are not decoration: the
/// access check for a create runs against the descriptor being attached, so
/// a `GENERIC_WRITE` create whose DACL omits `FILE_WRITE_EA`/
/// `FILE_WRITE_ATTRIBUTES` fails, and a DACL that omits the read bits is a
/// file its own reader cannot open. On a DIRECTORY the same bits are the
/// directory meanings — `FILE_READ_DATA` lists it, `FILE_WRITE_DATA` adds a
/// file to it, `FILE_APPEND_DATA` adds a subdirectory.
pub const OWNER_ONLY_MASK: u32 =
    FILE_READ_DATA | FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_READ_EA | FILE_WRITE_EA | FILE_READ_ATTRIBUTES
        | FILE_WRITE_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE;

/// What a READ-ONLY policy grants its owner: the bits needed to list the
/// directory, read the file, and read its policy — and nothing that adds,
/// writes, deletes or inherits. This is the native stand-in for `0o500`, used
/// where a test needs a directory this process cannot write to.
///
/// `WRITE_DAC` is in the mask on purpose, and it is not a data-write right: it
/// is the owner's ability to RE-LABEL the object, which `0o500` also leaves
/// intact on Unix (a mode is not ownership, and the owner may always `chmod`
/// it back). Without it the very state the caller arranged could not be
/// undone through this module — which is exactly how the first version of
/// this mask failed its own test.
pub const READ_ONLY_MASK: u32 =
    FILE_READ_DATA | FILE_READ_EA | FILE_READ_ATTRIBUTES | FILE_EXECUTE | READ_CONTROL | WRITE_DAC | SYNCHRONIZE;

/// What an existing object's DACL must cover before this module calls it
/// owner-only: the whole [`OWNER_ONLY_MASK`], so an object this module
/// created always validates and a write-only one (which its own reader could
/// not open) never does.
pub(crate) const REQUIRED_ACCESS: u32 = OWNER_ONLY_MASK;

/// The directory half of the private policy — the native spelling of `0o700`,
/// which on Unix is read + write + EXECUTE for the owner. Two of those three
/// bits have directory meanings this mask has to name explicitly:
///
/// - `FILE_EXECUTE` is `FILE_TRAVERSE`: the right to open anything INSIDE the
///   directory by path. `0o700` without it is a directory nobody can list
///   through, and the difference is observable, not theoretical — a directory
///   read back as private and then used as a parent fails the child's own
///   open.
/// - `FILE_DELETE_CHILD` is the other half of `w` on a directory: on Unix
///   removing an entry asks for write on the PARENT, and on Windows the
///   check is DELETE on the child or `FILE_DELETE_CHILD` on the parent. A
///   `0o700` directory whose owner could not remove its own entries would be
///   the analogue of a directory with `r` and `x` but no `w`.
///
/// Files keep [`OWNER_ONLY_MASK`] (`0o600` has no execute or delete-children
/// bit to map).
pub const PRIVATE_DIR_MASK: u32 = OWNER_ONLY_MASK | FILE_EXECUTE | FILE_DELETE_CHILD;

/// The access a validated open asks for on a FILE: enough to read its policy
/// and its bytes, and — for the create-if-absent caller — to append.
pub(crate) const APPEND_ACCESS: u32 =
    FILE_APPEND_DATA | FILE_WRITE_DATA | FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE;

/// What this module asks for when it opens an object only to read its policy
/// back. `WRITE_DAC` is in the mask because the reader of [`dir_privacy`] is
/// also the one that may tighten an existing directory
/// ([`ensure_private_dir`]) — and the owner of an object always holds it.
const PRIVACY_ACCESS: u32 = FILE_READ_ATTRIBUTES | READ_CONTROL | WRITE_DAC | SYNCHRONIZE;


pub(crate) struct LocalAlloc(pub(crate) *mut c_void);

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
pub(crate) fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut out: Vec<u16> = path.as_os_str().encode_wide().collect();
    if out.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the path contains an embedded NUL"));
    }
    out.push(0);
    Ok(out)
}

/// The last-error code of a failed call, as an `io::Error`.
pub(crate) fn last_error() -> io::Error {
    io::Error::last_os_error()
}

// ── creation with the policy already attached ────────────────────────────

/// Build the one creation policy into `sd`: an explicit, protected,
/// single-ACE DACL granting `access` to the current user, with the owner
/// pinned to `sid`. The returned guard owns the ACL the descriptor points
/// at, so it must outlive `sd`'s use in `CreateFileW`/`CreateDirectoryW`.
fn owner_only_descriptor(sd: &mut SECURITY_DESCRIPTOR, sid: &TokenUserSid, access: u32) -> io::Result<LocalAlloc> {
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
        grfAccessPermissions: access,
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
pub enum CreateError {
    Exists,
    Failed(io::Error),
}

/// Create a file with the owner-only DACL already attached, or report that it
/// already exists. Nothing is tightened after the fact.
pub fn create_new(path: &Path) -> Result<File, CreateError> {
    create_with(path, CREATE_NEW, GENERIC_WRITE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
}

/// Create-or-empty a file with the owner-only DACL already attached: the
/// shape an atomic write's temp needs, where the path is unique per writer and
/// carries no content worth preserving. There is no `Exists` outcome to
/// report, so this is the plain `io::Result` half of [`create_new`].
pub fn create_truncating(path: &Path) -> io::Result<File> {
    create_with(path, CREATE_ALWAYS, GENERIC_WRITE | FILE_READ_ATTRIBUTES | SYNCHRONIZE).map_err(|e| match e {
        CreateError::Exists => io::Error::from_raw_os_error(ERROR_FILE_EXISTS as i32),
        CreateError::Failed(e) => e,
    })
}

/// The one creation call both file constructors share. `access` is checked
/// against the very descriptor being attached, so a caller asking for bits
/// the policy does not grant is refused by the kernel rather than by us.
fn create_with(path: &Path, disposition: u32, access: u32) -> Result<File, CreateError> {
    let sid = TokenUserSid::current().map_err(CreateError::Failed)?;
    let mut sd = SECURITY_DESCRIPTOR::default();
    let _acl = owner_only_descriptor(&mut sd, &sid, OWNER_ONLY_MASK).map_err(CreateError::Failed)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: &mut sd as *mut SECURITY_DESCRIPTOR as *mut c_void,
        bInheritHandle: 0,
    };

    let wide_path = wide(path).map_err(CreateError::Failed)?;
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            disposition,
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

// ── directories: the same policy, on the shape that holds entries ────────

/// Create `dir` with the policy attached at creation — `CreateDirectoryW`'s
/// security parameter is the directory half of `CreateFileW`'s. An existing
/// directory is `ERROR_ALREADY_EXISTS`; callers that need idempotence use
/// [`ensure_private_dir`].
fn create_dir(path: &Path, access: u32) -> io::Result<()> {
    let sid = TokenUserSid::current()?;
    let mut sd = SECURITY_DESCRIPTOR::default();
    let _acl = owner_only_descriptor(&mut sd, &sid, access)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: &mut sd as *mut SECURITY_DESCRIPTOR as *mut c_void,
        bInheritHandle: 0,
    };
    let wide_path = wide(path)?;
    if unsafe { CreateDirectoryW(wide_path.as_ptr(), &attributes) } == 0 {
        return Err(last_error());
    }
    Ok(())
}

/// Replace an EXISTING directory's DACL with a protected, single-ACE policy
/// granting `access` to the current user — the native stand-in for `chmod`,
/// and the one place this module changes a policy it did not create.
///
/// **The owner is pinned here too, and it has to be.** A directory this
/// module did not create may belong to someone else by the host's own
/// default: an elevated-context process's new objects are owned by the
/// Administrators group, not by its user, and [`dir_privacy`] asks for the
/// current user. Pinning the owner to one's OWN token user needs no
/// privilege (it is the same call creation-time pinning already makes), so a
/// tighten that left the owner alone would hand back a directory that fails
/// this module's own readback — a repair that reports the fault it was
/// called to fix. `WRITE_OWNER` is requested on the open for that reason; a
/// policy that does not grant it to its current owner cannot be repaired
/// through this module, and says so with an error rather than half a fix.
pub fn set_dir_access(path: &Path, access: u32) -> io::Result<()> {
    let dir = open_existing(path, WRITE_DAC | WRITE_OWNER | READ_CONTROL, true)?;
    // The same refusal the feed makes, for the same reason: the handle names
    // the LINK, so writing its policy would report a repair on a junction
    // while the directory every caller actually uses keeps the default it
    // had. Refused by name, before anything is written.
    if let Some(reason) = reject_reparse_point(&dir)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to set the policy of {}: {reason}", path.display()),
        ));
    }
    let sid = TokenUserSid::current()?;
    let mut sd = SECURITY_DESCRIPTOR::default();
    let _acl = owner_only_descriptor(&mut sd, &sid, access)?;
    let mut present: i32 = 0;
    let mut dacl: *mut ACL = null_mut();
    let mut defaulted: i32 = 0;
    if unsafe {
        GetSecurityDescriptorDacl(&mut sd as *mut SECURITY_DESCRIPTOR as PSECURITY_DESCRIPTOR, &mut present, &mut dacl, &mut defaulted)
    } == 0
    {
        return Err(last_error());
    }
    let status = unsafe {
        SetSecurityInfo(
            dir.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            sid.as_ptr(),
            null_mut(),
            dacl,
            null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

/// Create `dir` if it is absent, and leave it owner-only either way: the
/// Windows half of `0700` on the directory a private file lives in. An
/// existing directory that is already private is left exactly as it is; one
/// that is not is tightened to the same policy a fresh create would have
/// attached — never refused, because refusing would regress the Unix
/// contract, which locks an existing directory down.
///
/// **Absence is a state, not a failure.** The policy of an object that is not
/// there cannot be read, so a directory that does not exist yet is created
/// with the policy attached rather than opened-and-checked first — asking
/// `dir_privacy` about a missing path is a `NotFound` error, and treating it
/// as one would make "create it" unreachable.
///
/// The policy is read back before returning, so a filesystem that accepts the
/// descriptor and ignores it is reported as the failure it is rather than
/// leaving a directory entries can be listed from.
pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => create_dir(path, PRIVATE_DIR_MASK)?,
        Err(e) => return Err(e),
        // A link is refused by NAME, before any policy is looked at or
        // written, and the same goes for a directory JUNCTION: both are
        // reparse points, and this module attaches a policy to the object a
        // path NAMES, never to what a link points at. Windows is stricter
        // than the Unix arm here on purpose — `set_permissions` would follow
        // the link and repair the target — because the two are
        // indistinguishable by the time a caller reads a "success" back.
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "refusing {}: it is a symbolic link or a directory junction, and the policy this module \
                     attaches belongs to the object a path names rather than to what a link points at",
                    path.display()
                ),
            ))
        }
        Ok(meta) if !meta.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} exists and is not a directory", path.display()),
            ))
        }
        Ok(_) => {
            if dir_privacy(path)?.is_some() {
                set_dir_access(path, PRIVATE_DIR_MASK)?;
            }
        }
    }
    match dir_privacy(path)? {
        None => Ok(()),
        Some(reason) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "the directory {} is not owner-only after its policy was attached: {reason}",
                path.display()
            ),
        )),
    }
}

// ── validating an existing file before it is used ────────────────────────

/// What the DACL and owner of one object actually are, read from the OBJECT's
/// own handle — never from the path, and never from a second open.
pub(crate) struct SecurityFacts {
    pub(crate) dacl_present: bool,
    pub(crate) dacl_protected: bool,
    pub(crate) ace_count: u32,
    /// Every ACE is an `ACCESS_ALLOWED_ACE_TYPE` for the current token user.
    pub(crate) all_aces_are_current_user_allows: bool,
    /// The union of the allow masks found.
    pub(crate) covered_mask: u32,
    pub(crate) owner_is_current_user: bool,
}

impl SecurityFacts {

    /// Does every fact hold for the mask `required`? The refusal reader asks
    /// this with the mask of the OBJECT it is looking at — a file's
    /// [`OWNER_ONLY_MASK`], a directory's [`PRIVATE_DIR_MASK`] — so the same
    /// walk answers for both without a second implementation.
    pub(crate) fn covers(&self, required: u32) -> bool {
        self.dacl_present
            && self.dacl_protected
            && self.ace_count > 0
            && self.all_aces_are_current_user_allows
            && self.owner_is_current_user
            && self.covered_mask & required == required
    }
}

/// Read the facts above out of `file`'s handle. `GetSecurityInfo` returns a
/// `WIN32_ERROR` (zero is success), not a BOOL — its failure code is the
/// return value, never `GetLastError`.
pub(crate) fn security_facts(file: &File) -> io::Result<SecurityFacts> {
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

/// The named refusal when an existing object is not the owner-only object a
/// caller is willing to use. `None` means it is, and only then may a caller
/// seek, truncate, write, or list. `required` is the mask that object's own
/// kind must cover — [`OWNER_ONLY_MASK`] for a file, [`PRIVATE_DIR_MASK`] for
/// a directory — never a weaker one.
fn refusal(file: &File, required: u32) -> io::Result<Option<String>> {
    let facts = security_facts(file)?;
    if facts.covers(required) {
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
            "its DACL mask {:#x} does not cover the owner's own read/write bits {required:#x}",
            facts.covered_mask
        )
    };
    Ok(Some(reason))
}

/// A file's refusal, with the file mask: what `feed` reads back after it
/// creates or opens one.
pub(crate) fn file_refusal(file: &File) -> io::Result<Option<String>> {
    refusal(file, REQUIRED_ACCESS)
}

/// Is the file at `path` owner-only? `None` is the yes; a `Some` is the named
/// reason, so a caller can report WHY rather than only that it refused. The
/// object's own handle answers — never the path, never a second open — and a
/// reparse point is refused AS ITSELF: a link's own policy is not the target
/// file's, and this module will not certify the one while the caller acts on
/// the other.
pub fn file_privacy(path: &Path) -> io::Result<Option<String>> {
    let file = open_existing(path, PRIVACY_ACCESS, false)?;
    if let Some(reason) = reject_reparse_point(&file)? {
        return Ok(Some(reason));
    }
    refusal(&file, REQUIRED_ACCESS)
}

/// The directory half of [`file_privacy`], opened with
/// `FILE_FLAG_BACKUP_SEMANTICS` (without it `CreateFileW` refuses a
/// directory) and read the same way: a protected, non-empty, current-user-only
/// allow DACL whose mask covers the directory's own read/write/list/traverse
/// bits, with the owner pinned to the current user.
///
/// A DIRECTORY JUNCTION (or any other reparse point) is refused by name, the
/// same way the feed refuses one: `FILE_FLAG_OPEN_REPARSE_POINT` means the
/// handle names the LINK, so the policy read back would be the link's own
/// while every caller acts through the path — which resolves to the target.
/// That asymmetry is exactly the kind of silent mis-answer this module is
/// supposed to refuse instead of return.
pub fn dir_privacy(path: &Path) -> io::Result<Option<String>> {
    let dir = open_existing(path, PRIVACY_ACCESS, true)?;
    if let Some(reason) = reject_reparse_point(&dir)? {
        return Ok(Some(reason));
    }
    refusal(&dir, PRIVATE_DIR_MASK)
}

/// Open an existing object with the given access: no creation, no
/// truncation, and `FILE_FLAG_OPEN_REPARSE_POINT` so a link at the path is
/// refused as itself instead of being silently followed. `dir` adds
/// `FILE_FLAG_BACKUP_SEMANTICS`, which is what lets the same call open a
/// directory handle.
pub(crate) fn open_existing(path: &Path, access: u32, dir: bool) -> io::Result<File> {
    let wide_path = wide(path)?;
    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT;
    if dir {
        flags |= FILE_FLAG_BACKUP_SEMANTICS;
    }
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            flags,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

/// Refuse a reparse point: a caller validates the object it holds, and a
/// link's target is a different object than the link the path names.
pub(crate) fn reject_reparse_point(file: &File) -> io::Result<Option<String>> {
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
        return Ok(Some(format!("the path is a reparse point (tag {:#x})", info.ReparseTag)));
    }
    Ok(None)
}
