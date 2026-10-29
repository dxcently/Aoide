//! Kernel-truth caller identity for an accepted connection: `SO_PEERCRED` on
//! Linux/Android, the peer's token user on native Windows. [`own_user`] is this
//! process's side of every same-user comparison, in the same host terms.
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
//! invented. Every gate that compares or prints an identity goes through it,
//! so the gates never learn which host they are on. The credential keeps the
//! number where the host has one ([`PeerCred::uid`]) and the SID beside it
//! ([`PeerCred::sid`]), each skipped when absent.
//!
//! This is the kernel's fact about the connection, not the wire's: a request's
//! own self-asserted name (`consumer`, `sessionId`) is never authenticated
//! here. A gate that needs both checks the wire field against its own list and
//! this identity against the owner.
//!
//! `std`'s `UnixStream::peer_cred` is unstable, so the Unix arm reads
//! `SO_PEERCRED` through `libc`. `SO_PEERCRED` is a Linux/Android capability:
//! POSIX.1 defines no peer-credential API, and BSD `getpeereid(3)` carries no
//! pid. Every other host gets the same UNIDENTIFIED `None` a failed read gives,
//! so a gate refuses. The Windows arm reads the peer's pid with
//! `SIO_AF_UNIX_GETPEERPID` and turns it into a token user through
//! [`crate::win_proc`], which refuses a pid reused between the two reads.

#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use crate::win_unix::UnixStream;

/// A kernel-verified user identity, in the host's own terms: the one type every
/// identity comparison and every identity print goes through.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PeerUser {
    /// The user's real uid: Unix's `SO_PEERCRED`.
    Uid(u32),
    /// The token user as the canonical `S-1-5-…` string: native Windows'
    /// `GetTokenInformation(TokenUser)`. There is no uid there, and none is invented.
    Sid(String),
}

impl PeerUser {
    /// How a refusal names the user it refused, in the host's own words.
    pub fn label(&self) -> String {
        match self {
            PeerUser::Uid(uid) => format!("uid {uid}"),
            PeerUser::Sid(sid) => format!("user {sid}"),
        }
    }
}

impl std::fmt::Display for PeerUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerUser::Uid(uid) => write!(f, "{uid}"),
            PeerUser::Sid(sid) => f.write_str(sid),
        }
    }
}

/// Kernel-truth identity of a connected stream's peer. The fields are
/// host-shaped on purpose: the uid and gid exist on Unix and not on native
/// Windows, where the SID takes their place. A caller that wants "who is this,
/// comparably" asks [`PeerCred::user`]; one that wants the Unix number asks
/// [`PeerCred::uid`], which is `None` where the host has no such thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCred {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub sid: Option<String>,
    pub pid: i32,
}

impl PeerCred {
    /// This peer's user, in the host's own terms: what the gates compare.
    /// `None` only for a credential that names no user at all, which
    /// [`peer_cred`] never returns.
    pub fn user(&self) -> Option<PeerUser> {
        match (self.uid, &self.sid) {
            (Some(uid), _) => Some(PeerUser::Uid(uid)),
            (None, Some(sid)) => Some(PeerUser::Sid(sid.clone())),
            (None, None) => None,
        }
    }

    /// The credential for a KNOWN user with no kernel stamp behind it: what a
    /// test injects where a real read would go. Never produced by [`peer_cred`],
    /// which only reports what the host answered.
    pub fn for_user(user: &PeerUser) -> PeerCred {
        match user {
            PeerUser::Uid(uid) => PeerCred { uid: Some(*uid), gid: None, sid: None, pid: 0 },
            PeerUser::Sid(sid) => PeerCred { uid: None, gid: None, sid: Some(sid.clone()), pid: 0 },
        }
    }
}

/// THIS process's own user, in the host's own terms: the other side of every
/// identity gate. Unix asks `geteuid(2)`, which cannot fail. Native Windows asks
/// its own token's user SID. A host that cannot answer returns `None`, and its
/// caller refuses rather than admitting by default.
pub fn own_user() -> Option<PeerUser> {
    #[cfg(unix)]
    {
        Some(PeerUser::Uid(unsafe { libc::geteuid() } as u32))
    }
    #[cfg(windows)]
    {
        crate::win_proc::current_user_sid().ok().map(PeerUser::Sid)
    }
}

/// Read the peer's kernel-truth identity off `stream`. `None` on ANY failure (a
/// non-`AF_UNIX` stream, an unexpected `getsockopt` error) and on any host with no
/// such mechanism. Failure means UNIDENTIFIED, never a panic and never a
/// fabricated uid: every gate that decides on this value treats `None` as refuse.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn peer_cred(stream: &UnixStream) -> Option<PeerCred> {
    let fd = stream.as_raw_fd();
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `fd` is a valid, open unix-domain socket fd owned by `stream` for the
    // duration of this call (borrowed, never taken); `cred`/`len` are correctly sized
    // out-parameters matching `SO_PEERCRED`'s `struct ucred`, and `getsockopt` never
    // writes past `len` bytes into `cred`.
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

/// Native Windows: the peer's process from `SIO_AF_UNIX_GETPEERPID`, and its user
/// from that process's token. `None` on ANY failure: an unconnected socket, a peer
/// this process may not query, a pid that no longer names the process it did.
#[cfg(windows)]
pub fn peer_cred(stream: &UnixStream) -> Option<PeerCred> {
    let pid = stream.peer_pid().ok()?;
    let pid = i32::try_from(pid).ok()?;
    let sid = crate::win_proc::process_user_sid(pid as u32).ok()?;
    Some(PeerCred { uid: None, gid: None, sid: Some(sid), pid })
}

/// No peer-credential mechanism on this host: the same UNIDENTIFIED `None` a
/// failed read gives.
#[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
pub fn peer_cred(_stream: &UnixStream) -> Option<PeerCred> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `UnixStream::pair()` socketpair is entirely local to this process, so
    /// both ends must report this process's own uid and pid. A real, checkable
    /// fact, without a second process or root.
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
        assert_eq!(cred_a.sid, None, "no SID is invented where the host has uids");
        assert_eq!(cred_a.user(), Some(PeerUser::Uid(euid)));
        assert_eq!(own_user(), Some(PeerUser::Uid(euid)));
    }

    /// The Windows arm, natively: a socketpair this process made has this process's
    /// token user behind it, and the credential carries no uid.
    #[cfg(windows)]
    #[test]
    fn peer_cred_natively_names_this_processs_token_user_and_no_uid() {
        let (a, b) = crate::win_unix::UnixStream::pair().expect("pair");
        let cred_a = peer_cred(&a).expect("the peer pid and its token user must both read");
        let cred_b = peer_cred(&b).expect("either end names the process at the far end");
        let mine = own_user().expect("this process's own identity");
        assert_eq!(cred_a.user(), Some(mine.clone()));
        assert_eq!(cred_b.user(), Some(mine));
        assert_eq!(cred_a.uid, None, "native Windows has no uid to report");
        assert_eq!(cred_a.pid as u32, std::process::id());
        assert!(cred_a.sid.as_deref().expect("a SID").starts_with("S-1-"));
    }

    /// `SO_PEERCRED` is a point-in-time stamp taken at connect, not a liveness check:
    /// it still answers after the peer has hung up.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn peer_cred_survives_the_peer_hanging_up() {
        let (a, b) = std::os::unix::net::UnixStream::pair().expect("socketpair");
        drop(b);
        assert!(peer_cred(&a).is_some(), "SO_PEERCRED reflects the stamped-at-connect identity, not peer liveness");
    }

    /// A connected socket answers where a listener could not. The listening socket
    /// has no peer, so the check is made on the client end.
    #[cfg(windows)]
    #[test]
    fn peer_cred_answers_on_a_connected_socket() {
        let dir = std::env::temp_dir().join(format!("aoide-peercred-{}", std::process::id()));
        crate::owner_only::ensure_private_dir(&dir).expect("private scratch dir");
        let path = dir.join("l.sock");
        let _ = std::fs::remove_file(&path);
        let listener = crate::win_unix::UnixListener::bind(&path).expect("bind");
        let client = crate::win_unix::UnixStream::connect(&path).expect("connect");
        assert!(peer_cred(&client).is_some(), "a connected socket answers");
        drop(listener);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_credential_reports_the_user_its_host_shape_carries() {
        let unix = PeerCred { uid: Some(1000), gid: Some(1000), sid: None, pid: 7 };
        assert_eq!(unix.user(), Some(PeerUser::Uid(1000)));
        assert_eq!(unix.user().unwrap().to_string(), "1000");

        let windows = PeerCred { uid: None, gid: None, sid: Some("S-1-5-21-1-2-3-1001".into()), pid: 7 };
        assert_eq!(windows.user(), Some(PeerUser::Sid("S-1-5-21-1-2-3-1001".into())));
        assert_eq!(windows.user().unwrap().to_string(), "S-1-5-21-1-2-3-1001");

        // A credential naming no user at all is the one case the gates must read as unidentified.
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

    #[test]
    fn a_user_labels_itself_in_the_host_words_a_refusal_uses() {
        assert_eq!(PeerUser::Uid(1000).label(), "uid 1000");
        assert_eq!(PeerUser::Sid("S-1-5-18".into()).label(), "user S-1-5-18");
    }

    #[test]
    fn this_process_has_an_own_identity_on_this_host() {
        assert!(own_user().is_some(), "every real host can say who this process is");
    }
}
