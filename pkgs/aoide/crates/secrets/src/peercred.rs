//! Caller identity for the secrets broker. The kernel-truth identity itself
//! (`SO_PEERCRED` / the peer's token user: [`PeerUser`], [`PeerCred`],
//! [`peer_cred`]) lives in `aoide_protocol::peercred`, the one copy every
//! identity gate in the workspace compares through. It is re-exported here so
//! the broker, the client and the server's own gate keep their paths.
//!
//! What stays in this module is the broker's own display: the username and the
//! process name an ask's origin line prints.

pub use aoide_protocol::peercred::{peer_cred, PeerCred, PeerUser};

/// The username behind a peer's identity, for the origin line a park renders
/// — `getpwuid_r(3)` on Unix (the thread-safe form: this broker is
/// thread-per-connection, `broker.rs`'s own module doc), `LookupAccountSidW`
/// on native Windows. `None` on any failure (no such account, a truncated or
/// malformed record, the lookup mechanism itself unavailable), never a panic,
/// same fail-to-`None` posture [`peer_cred`] itself holds.
///
/// This is DISPLAY DATA for a park's "origin" line (a P3 addition,
/// `park::AskOrigin`'s own doc) — it never gates anything the way the raw
/// identity itself does (`broker::handle_dismiss`'s peer check, module doc
/// above).
pub fn username_for(user: &PeerUser) -> Option<String> {
    match user {
        #[cfg(unix)]
        PeerUser::Uid(uid) => username_for_uid(*uid),
        // A SID cannot arise on a host with no tokens, so this arm is
        // unreachable there rather than a refusal that could fire.
        #[cfg(not(unix))]
        PeerUser::Uid(_) => None,
        #[cfg(windows)]
        PeerUser::Sid(sid) => username_for_sid(sid),
        #[cfg(not(windows))]
        PeerUser::Sid(_) => None,
    }
}

/// Best-effort `uid` -> username lookup — `None` on any failure, never a
/// panic. Unix only: a Windows caller has a SID, not a uid
/// ([`username_for`]).
#[cfg(unix)]
pub fn username_for_uid(uid: u32) -> Option<String> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf: Vec<libc::c_char> = vec![0; 4096];
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `pwd`/`buf`/`result` are correctly sized out-parameters
    // matching `getpwuid_r`'s documented contract; `buf`'s pointer/len are
    // passed together and `getpwuid_r` never writes past `buf.len()`.
    let ret = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result) };
    if ret != 0 || result.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    // SAFETY: `pwd.pw_name` is a valid NUL-terminated C string owned by
    // `buf` (still in scope) once `getpwuid_r` returns success with a
    // non-null `result`.
    let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) }.to_string_lossy().into_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// A SID's account name, `DOMAIN\name` as `whoami` prints it — native
/// Windows only, delegating to the one implementation of that lookup
/// (`aoide_protocol::win_proc`, which is also where the SID came from). A
/// lookup that fails is `None`, never the SID dressed up as a name.
#[cfg(windows)]
fn username_for_sid(sid: &str) -> Option<String> {
    aoide_protocol::win_proc::account_name_of_sid(sid).ok()
}

/// The short name of the process behind a peer identity — the name a human
/// reads on an origin line. `None` on any failure.
///
/// **Must be read AT PARK TIME, never later** — the caller's own doc on why
/// (a pid can exit and be reused well before an ask resolves or a popup
/// renders it). This is UNTRUSTED, process-controlled DISPLAY TEXT (a process
/// may name itself anything), rendered verbatim by a surface, never
/// interpreted as anything else, same posture the wire's self-asserted
/// `consumer`/`reason` fields already hold.
pub fn read_comm(pid: i32) -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let raw = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
    // Native Windows has no `comm`: the process table's image name is the
    // nearest fact, and it is the FULL file name where Linux truncates to 15
    // bytes — a difference in precision, not in kind.
    #[cfg(windows)]
    {
        let pid = u32::try_from(pid).ok()?;
        let table = aoide_protocol::win_proc::processes().ok()?;
        table.iter().find(|p| p.pid == pid).map(|p| p.exe.clone())
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn this_process() -> PeerUser {
        aoide_protocol::peercred::own_user().expect("this process's own identity")
    }

    // ── username_for / read_comm (P3: the ask origin line) ──────────────

    #[test]
    fn username_for_resolves_this_processs_own_identity() {
        assert!(
            username_for(&this_process()).is_some(),
            "this process's own identity must resolve to SOME username on any real host"
        );
    }

    #[test]
    fn username_for_uid_is_none_for_an_implausible_uid() {
        // A uid no account can have — Unix only, like the lookup itself.
        #[cfg(unix)]
        assert_eq!(username_for_uid(u32::MAX), None);
        // A SID that names nothing resolves to nothing, on the host that has
        // SIDs; elsewhere the arm is unreachable by construction.
        assert_eq!(username_for(&PeerUser::Sid("S-1-5-21-0-0-0-9999999999".to_string())), None);
    }

    /// Linux-only (like `read_comm`'s `/proc` read — "`/proc` unmounted" is
    /// the *normal* case on any other host) and its Windows sibling, which
    /// reads the process table's image name instead.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn read_comm_resolves_this_processs_own_comm() {
        assert!(read_comm(std::process::id() as i32).is_some(), "/proc/<this pid>/comm must be readable for this live process");
    }

    #[cfg(windows)]
    #[test]
    fn read_comm_resolves_this_processs_own_image_name() {
        let name = read_comm(std::process::id() as i32).expect("a live pid has a row in the process table");
        assert!(!name.is_empty(), "and that row names its executable");
    }

    #[test]
    fn read_comm_is_none_for_a_pid_that_does_not_exist() {
        assert_eq!(read_comm(i32::MAX), None);
    }
}
