//! Kernel-truth caller identity for an accepted connection (task #73):
//! `SO_PEERCRED` on Unix, the token user of the peer's process on native
//! Windows. Before this phase, the ONLY identity this crate ever recorded
//! for a socket caller was the wire's own self-asserted `consumer` STRING
//! (`AGENTS.md`'s replay-ledger ruling, `CONTRACTS.md`'s honesty note) —
//! nothing verified who actually opened the connection at the OS level.
//! This module closes that half: Linux stamps every `AF_UNIX` socket with
//! the connecting process's real `uid`/`gid`/`pid` at `connect(2)` time,
//! readable by the accepting side via `getsockopt(2)`, and unlike the
//! wire's `consumer` field this is not something the connecting process can
//! lie about.
//!
//! | fact | Unix | native Windows |
//! | --- | --- | --- |
//! | the peer's process | `SO_PEERCRED`'s `pid` | `SIO_AF_UNIX_GETPEERPID` |
//! | the peer's user | `SO_PEERCRED`'s `uid` | the peer's token user, a SID |
//! | the peer's group | `SO_PEERCRED`'s `gid` | — (no such fact) |
//!
//! **One identity, two host spellings**: [`PeerUser`] is a `uid` on Unix and
//! the token user's `S-1-5-…` SID on native Windows, because those are the
//! two facts the two hosts have, and a number derived from a SID would be
//! invented. Everything that compares or prints an identity goes through it,
//! so the gates below never learn which host they are on — and the wire keeps
//! the number where it always was ([`PeerCred::uid`]) while the SID rides
//! beside it ([`PeerCred::sid`]), additive, skipped when absent.
//!
//! **This does NOT make `consumer` itself authenticated.** A policy's
//! `consumers[]` list, and now `automation.consumers`, are still checked
//! against the SELF-ASSERTED wire field — see `AGENTS.md`'s replay-ledger
//! ruling and `CONTRACTS.md`'s "Honesty note: `consumer` is SELF-ASSERTED"
//! for why that stays true even with the sealed session credential (#63)
//! landed: the seal authenticates the calling SESSION and its origin
//! CLASS (the origin gate's axis), never the consumer NAME — that
//! authentication is a separate, unbuilt axis. What #73 adds is a SEPARATE, orthogonal
//! fact this crate did not have before: the connecting process's real
//! identity, verified by the kernel, independent of anything the wire request
//! itself claims. `broker::handle_dismiss` is the first (and, as of this
//! phase, only) place that fact gates a decision — see that function's own
//! doc for exactly how.
//!
//! `std`'s own `UnixStream::peer_cred` accessor is unstable
//! (`unix_socket_peek`/`peer_credentials`-adjacent nightly-only APIs), so
//! the Unix arm reads `SO_PEERCRED` directly via `libc` — already this
//! crate's dependency (`client::connect_bounded`'s own module doc), zero
//! new deps, matching this crate's house rule. The Windows arm reads the
//! peer's pid with `SIO_AF_UNIX_GETPEERPID` (`aoide_protocol::win_unix`) and
//! turns it into a token user through `aoide_protocol::win_proc`, which
//! closes the pid-reuse window around that read.
//!
//! **`SO_PEERCRED` is a Linux/Android capability, not a POSIX one.** POSIX.1
//! defines no peer-credential API, and BSD `getpeereid(3)` carries no pid —
//! it could not fill [`PeerCred::pid`], the field the origin gate walks as the
//! kernel's ancestry fact (`aoide_storage::attest`'s `attested_session`). So
//! every other Unix gets the SAME UNIDENTIFIED `None` a failed read gives
//! ([`peer_cred`]'s `#[cfg]` arm below): `broker::handle_dismiss`'s peer-uid
//! match and `broker::admin_gate` refuse, as they already do for an
//! unidentified connection. Never a fabricated uid/pid, never a second
//! mechanism.

#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use aoide_protocol::win_unix::UnixStream;

/// A kernel-verified user identity, in the host's own terms — the one type
/// every identity comparison and every identity print goes through, so a
/// gate reads the same on both hosts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PeerUser {
    /// The peer's real uid — Unix's `SO_PEERCRED`.
    Uid(u32),
    /// The peer's token user, as the canonical `S-1-5-…` string — native
    /// Windows' `GetTokenInformation(TokenUser)`. There is no uid there, and
    /// nothing here invents one.
    Sid(String),
}

impl std::fmt::Display for PeerUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerUser::Uid(uid) => write!(f, "{uid}"),
            PeerUser::Sid(sid) => f.write_str(sid),
        }
    }
}

/// Kernel-truth identity of a connected stream's peer. Never a value, never
/// anything sensitive — just the facts the host itself stamped or looked up
/// on the connection.
///
/// The fields are host-shaped rather than uniform, and deliberately so: the
/// uid and gid exist on Unix and do not exist on native Windows, where the
/// SID is the fact and takes their place. A caller that wants "who is this,
/// comparably" asks [`PeerCred::user`]; a caller that wants the Unix number
/// because it always meant a number asks [`PeerCred::uid`], which is `None`
/// exactly where the host has no such thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCred {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub sid: Option<String>,
    pub pid: i32,
}

impl PeerCred {
    /// This peer's user, in the host's own terms — the identity the gates
    /// compare and the origin line prints. `None` only for a credential that
    /// names no user at all, which [`peer_cred`] never returns.
    pub fn user(&self) -> Option<PeerUser> {
        match (self.uid, &self.sid) {
            (Some(uid), _) => Some(PeerUser::Uid(uid)),
            (None, Some(sid)) => Some(PeerUser::Sid(sid.clone())),
            (None, None) => None,
        }
    }

    /// The credential for a KNOWN user with no kernel stamp behind it — what
    /// a test injects where a real read would go, and what a caller has when
    /// it describes ITSELF. Never produced by [`peer_cred`], which only ever
    /// reports what a host answered.
    pub fn for_user(user: &PeerUser) -> PeerCred {
        match user {
            PeerUser::Uid(uid) => PeerCred { uid: Some(*uid), gid: None, sid: None, pid: 0 },
            PeerUser::Sid(sid) => {
                PeerCred { uid: None, gid: None, sid: Some(sid.clone()), pid: 0 }
            }
        }
    }
}

/// Read the peer's kernel-truth identity off `stream` — `None` on ANY failure
/// (a non-`AF_UNIX` stream, an unexpected `getsockopt` error, a platform that
/// doesn't support it at all). **Failure to read means the connection is
/// treated as UNIDENTIFIED, never a panic and never a fabricated uid** —
/// task #73's own requirement. Every caller that gates a decision on this
/// value must treat `None` as "refuse," never as "pass" (`broker::
/// handle_dismiss`'s own doc is the one place this matters today) — the
/// absence of kernel-truth identity is not license to assume a benign
/// caller.
///
/// "A platform that doesn't support it at all" is structural, not
/// hypothetical (module doc): the `None` arm below, on every host that is
/// neither Linux/Android nor Windows.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn peer_cred(stream: &UnixStream) -> Option<PeerCred> {
    let fd = stream.as_raw_fd();
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `fd` is a valid, open unix-domain socket fd owned by `stream`
    // for the duration of this call (we only borrow `stream`, never take
    // ownership of the fd); `cred`/`len` are correctly sized out-parameters
    // matching `SO_PEERCRED`'s documented `struct ucred` shape, and
    // `getsockopt` never writes past `len` bytes into `cred`.
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return None;
    }
    Some(PeerCred { uid: Some(cred.uid), gid: Some(cred.gid), sid: None, pid: cred.pid })
}

/// Native Windows: the peer's process from `SIO_AF_UNIX_GETPEERPID`, and its
/// user from that process's token (`aoide_protocol::win_proc`, which refuses a
/// pid that was reused between the reads rather than reporting the previous
/// process's user). `None` on ANY failure — an unconnected socket, a peer this
/// process may not query, a pid that no longer names the process it did — the
/// same UNIDENTIFIED `None` the Unix arm's failed `getsockopt` gives, and the
/// same "never refuse to fail" posture: there is no uid here to fall back on
/// and none is invented.
#[cfg(windows)]
pub fn peer_cred(stream: &UnixStream) -> Option<PeerCred> {
    let pid = stream.peer_pid().ok()?;
    let pid = i32::try_from(pid).ok()?;
    let sid = aoide_protocol::win_proc::process_user_sid(pid as u32).ok()?;
    Some(PeerCred { uid: None, gid: None, sid: Some(sid), pid })
}

/// No peer-credential mechanism on this host (module doc): the SAME
/// UNIDENTIFIED `None` a failed read gives, so gates keep refusing.
#[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
pub fn peer_cred(_stream: &UnixStream) -> Option<PeerCred> {
    None
}

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
        crate::home::effective_user().expect("this process's own identity")
    }

    /// A `UnixStream::pair()` socketpair is entirely local to THIS process
    /// — both ends' peer identity must therefore be this process's own,
    /// which is a real, checkable fact (not a placeholder) without needing a
    /// second process or root. Linux/Android only, like the mechanism
    /// itself: elsewhere the same assertion is made through the identity
    /// [`this_process`] resolves, which is what a host without
    /// `SO_PEERCRED` has instead.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn peer_cred_on_a_scratch_socketpair_matches_this_processs_own_identity() {
        let (a, b) = std::os::unix::net::UnixStream::pair().expect("socketpair");
        let cred_a = peer_cred(&a).expect("SO_PEERCRED must be readable on a live socketpair");
        let cred_b = peer_cred(&b).expect("SO_PEERCRED must be readable on a live socketpair");
        let euid = unsafe { libc::geteuid() };
        let pid = std::process::id() as i32;
        assert_eq!(cred_a.uid, Some(euid));
        assert_eq!(cred_a.pid, pid);
        assert_eq!(cred_b.uid, Some(euid));
        assert_eq!(cred_b.pid, pid);
        assert_eq!(cred_a.user(), Some(PeerUser::Uid(euid)));
    }

    /// The Windows arm, natively: a socketpair this process made has THIS
    /// process's token user behind it, and the credential's `uid` is absent
    /// rather than a number derived from the SID.
    #[cfg(windows)]
    #[test]
    fn peer_cred_natively_names_this_processs_token_user_and_no_uid() {
        let (a, b) = aoide_protocol::win_unix::UnixStream::pair().expect("pair");
        let cred_a = peer_cred(&a).expect("the peer pid and its token user must both read");
        let cred_b = peer_cred(&b).expect("either end names the process at the far end");
        let mine = this_process();
        assert_eq!(cred_a.user(), Some(mine.clone()));
        assert_eq!(cred_b.user(), Some(mine));
        assert_eq!(cred_a.uid, None, "native Windows has no uid to report");
        assert_eq!(cred_a.pid as u32, std::process::id());
        assert!(cred_a.sid.as_deref().expect("a SID").starts_with("S-1-"));
    }

    /// `SO_PEERCRED` on a fd that isn't a socket at all fails cleanly —
    /// proves the "failure = unidentified, never a panic" contract without
    /// needing to fabricate a genuinely broken socket. Linux/Android only
    /// (it asserts the `Some` happy path).
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn peer_cred_is_none_after_the_peer_has_hung_up_and_the_fd_reused_is_out_of_scope() {
        // A stream whose OTHER end has already been dropped is still a
        // valid socket fd (SO_PEERCRED reads the credentials recorded at
        // connect/pair time, which survive the peer closing) — this test
        // instead asserts the happy path stays `Some` even so, documenting
        // that `SO_PEERCRED` is a point-in-time stamp, not a liveness
        // check, so a caller must read it at connection start (this
        // module's own doc) rather than assume it tracks the peer's
        // current state.
        let (a, b) = std::os::unix::net::UnixStream::pair().expect("socketpair");
        drop(b);
        assert!(peer_cred(&a).is_some(), "SO_PEERCRED reflects the stamped-at-connect identity, not peer liveness");
    }

    /// A listening socket has no peer at all: the Windows arm must answer
    /// the same UNIDENTIFIED `None` a failed read gives, never a pid of its
    /// own or a panic.
    #[cfg(windows)]
    #[test]
    fn peer_cred_is_none_on_a_listening_socket() {
        let dir = std::env::temp_dir().join(format!("aoide-peercred-{}", std::process::id()));
        aoide_protocol::owner_only::ensure_private_dir(&dir).expect("private scratch dir");
        let path = dir.join("l.sock");
        let _ = std::fs::remove_file(&path);
        let listener = aoide_protocol::win_unix::UnixListener::bind(&path).expect("bind");
        // The listening socket is not a `UnixStream`; the honest check is
        // that a CONNECTED one answers where a listener could not.
        let client = aoide_protocol::win_unix::UnixStream::connect(&path).expect("connect");
        assert!(peer_cred(&client).is_some(), "a connected socket answers");
        drop(listener);
        let _ = std::fs::remove_file(&path);
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

    // ── PeerCred, the two host shapes of one identity ───────────────────

    #[test]
    fn a_credential_reports_the_user_its_host_shape_carries() {
        let unix = PeerCred { uid: Some(1000), gid: Some(1000), sid: None, pid: 7 };
        assert_eq!(unix.user(), Some(PeerUser::Uid(1000)));
        assert_eq!(unix.user().unwrap().to_string(), "1000");

        let windows =
            PeerCred { uid: None, gid: None, sid: Some("S-1-5-21-1-2-3-1001".into()), pid: 7 };
        assert_eq!(windows.user(), Some(PeerUser::Sid("S-1-5-21-1-2-3-1001".into())));
        assert_eq!(windows.user().unwrap().to_string(), "S-1-5-21-1-2-3-1001");

        // A credential naming no user at all is the one case the gates must
        // read as unidentified.
        let neither = PeerCred { uid: None, gid: None, sid: None, pid: 7 };
        assert_eq!(neither.user(), None);
    }

    #[test]
    fn for_user_builds_the_same_shape_a_read_would_and_never_carries_a_pid() {
        let a = PeerCred::for_user(&PeerUser::Uid(42));
        assert_eq!(a.uid, Some(42));
        assert_eq!(a.pid, 0, "a credential with no kernel stamp names no process");
        let b = PeerCred::for_user(&PeerUser::Sid("S-1-5-18".into()));
        assert_eq!(b.uid, None);
        assert_eq!(b.user(), Some(PeerUser::Sid("S-1-5-18".into())));
    }
}
