//! The bounded unix connect. `UnixStream::connect` waits forever when the
//! listener's connection backlog is full, so a caller that must not hang on
//! a peer connects through [`connect_bounded`]. On Windows it is `win_unix`'s
//! `connect_timeout`.

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use crate::win_unix::UnixStream;
use std::io;
use std::path::Path;
use std::time::Duration;

/// Put `fd` into (or out of) non-blocking mode, through `fcntl(F_GETFL)` and
/// `fcntl(F_SETFL)`. A failed `fcntl` is returned, never ignored: a connect
/// left blocking would have no bound.
#[cfg(unix)]
fn set_fd_nonblocking(fd: RawFd, nonblocking: bool) -> io::Result<()> {
    // SAFETY: `fd` is a fd this function's caller owns for the duration of
    // this call (a freshly created socket, never shared); `fcntl(F_GETFL)`/
    // `fcntl(F_SETFL)` are ordinary, always-defined operations on any fd
    // this process holds.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        let next = if nonblocking { flags | libc::O_NONBLOCK } else { flags & !libc::O_NONBLOCK };
        if libc::fcntl(fd, libc::F_SETFL, next) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Build a `sockaddr_un` for `path`, with the `socklen_t` the kernel reads
/// alongside it. Both target-dependent numbers are read off the struct: the
/// cap is `sun_path`'s own width (108 on Linux, 104 on the BSDs, 126 on
/// Haiku, 1023 on AIX), and the length is
/// `offsetof(sockaddr_un, sun_path)` + the path + its terminating NUL
/// (`SUN_LEN`) — never a remembered 108 or `size_of::<sa_family_t>()`. A NUL
/// in the path is refused and an empty path is the zero-length (unnamed)
/// address: the same answers `std::os::unix::net::SocketAddr::from_pathname`
/// gives, which the tests assert against `std` rather than restate.
///
/// BSD's `sockaddr_un` also opens with a `sun_len` byte. It is left zero, as
/// `std`'s own builder leaves it on those targets (`sun_len` appears nowhere
/// in std's source — it sets `sun_family` and nothing else), so this follows
/// the standard library's convention instead of inventing an ABI value for a
/// target with no runner here. `offset_of!` still accounts for the byte when
/// the struct has it.
#[cfg(unix)]
fn unix_sockaddr(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    let bytes = path.as_os_str().as_bytes();

    // A NUL in the path would end the kernel's view of the name there — a
    // connect to an address the caller never named. Refuse, as `std` does.
    if bytes.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "paths may not contain interior null bytes"));
    }

    // SAFETY: `sockaddr_un` is plain-old-data — all zeros is a valid value,
    // and writing only the fields below is the standard way to build one.
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };

    // This target's OWN `sun_path` width: a remembered 108 would pass a
    // 105..107-byte path onto a 104-byte BSD struct, zero-filled.
    if bytes.len() >= addr.sun_path.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path must be shorter than SUN_LEN"));
    }

    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, &src) in addr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = src as libc::c_char;
    }
    // The terminating zero is already in place: the struct was zeroed above
    // and only `bytes.len()` bytes of `sun_path` were written over.

    // The real offset of the field, whether or not the struct opens with
    // BSD's `sun_len` — plus the path and its NUL, or alone for the empty
    // path, which is the zero-length (unnamed) address.
    let path_offset = std::mem::offset_of!(libc::sockaddr_un, sun_path);
    let len = if bytes.is_empty() { path_offset } else { path_offset + bytes.len() + 1 };

    Ok((addr, len as libc::socklen_t))
}

/// Short sleep between retries of the raw `connect(2)` syscall on `EAGAIN`.
/// See [`connect_bounded`] for why `EAGAIN` is retried rather than polled.
#[cfg(unix)]
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(15);

/// A fresh `AF_UNIX` stream socket, close-on-exec. `UnixStream::connect` makes
/// its socket close-on-exec too, and a process that spawns children while it
/// connects (aoided does) must not hand them the descriptor. Linux and Android
/// take `SOCK_CLOEXEC` in `socket(2)`'s type, so the descriptor is never
/// inheritable, not even for an instant. Elsewhere `FD_CLOEXEC` is set straight
/// after `socket(2)`. Its `OwnedFd` closes the socket on drop, so no error path
/// can leak it.
#[cfg(unix)]
fn stream_socket() -> io::Result<OwnedFd> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let kind = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let kind = libc::SOCK_STREAM;

    // SAFETY: `socket(2)` takes no pointers. On success the descriptor is
    // fresh and owned by the caller alone.
    let fd = unsafe { libc::socket(libc::AF_UNIX, kind, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is the fresh socket made above, and nothing else holds it.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    // SAFETY: `owned` is the open socket; `F_SETFD` takes no pointer.
    if unsafe { libc::fcntl(owned.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(owned)
}

/// Bounded replacement for `UnixStream::connect`, over TWO genuinely
/// different failure shapes a nonblocking `connect(2)` to an `AF_UNIX` socket
/// can return. `std`'s `UnixStream` has no `connect_timeout` (unlike
/// `TcpStream`), so both are hand-rolled on `libc`:
///
/// - **`EINPROGRESS`**: the kernel accepted the attempt and queued it. A real
///   half-open state exists, so `poll(POLLOUT)` is the right primitive to wait
///   on it, then `SO_ERROR` says whether it actually succeeded.
/// - **`EAGAIN`**: on Linux, a saturated `AF_UNIX` listen backlog makes
///   `connect(2)` return `EAGAIN` IMMEDIATELY, never `EINPROGRESS`. There is no
///   half-open connection and no fd event to `poll()` for, only a rejected
///   ATTEMPT. Failing on the first `EAGAIN` would make a saturated backlog
///   worse than `UnixStream::connect`, which sleeps in the kernel until a slot
///   frees. So the `connect(2)` SYSCALL ITSELF is retried on a short interval
///   ([`CONNECT_RETRY_INTERVAL`]), bounded by the same overall `timeout`
///   budget. Polling a fd for an event that never arrives is not the fix.
///
/// The result is the `io::Result<UnixStream>` that `UnixStream::connect`
/// returns. A connect that runs out of `timeout` is `TimedOut`. When the
/// backlog is what held it up, the message says so. A `timeout` the clock
/// cannot hold is refused as `InvalidInput`, before any socket is made.
#[cfg(unix)]
pub fn connect_bounded(socket_path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout too large"))?;
    let (addr, addr_len) = unix_sockaddr(socket_path)?;

    // `fd` owns the socket from here on. The success paths hand it to
    // `UnixStream`, and every error path drops it, which closes it.
    let fd = stream_socket()?;
    set_fd_nonblocking(fd.as_raw_fd(), true)?;

    // Phase 1: attempt the syscall itself, retrying on `EAGAIN`/`EINTR`
    // (both are about the ATTEMPT, not a queued connection) until it
    // either succeeds outright, reports `EINPROGRESS` (a real half-open
    // state — falls out of this loop into phase 2 below), or fails for
    // real.
    loop {
        // SAFETY: `addr`/`addr_len` describe a valid, fully-initialized
        // `sockaddr_un` for this exact `fd`'s own address family.
        let rc = unsafe { libc::connect(fd.as_raw_fd(), &addr as *const libc::sockaddr_un as *const libc::sockaddr, addr_len) };
        if rc == 0 {
            set_fd_nonblocking(fd.as_raw_fd(), false)?;
            return Ok(UnixStream::from(fd));
        }

        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINPROGRESS) => break,
            Some(libc::EAGAIN) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("timed out after {timeout:?} connecting (the listener's connection backlog is saturated)"),
                    ));
                }
                std::thread::sleep(CONNECT_RETRY_INTERVAL.min(deadline - now));
                continue;
            }
            Some(libc::EINTR) => continue, // the syscall itself was interrupted — just retry it
            _ => return Err(err),
        }
    }

    // Phase 2: `EINPROGRESS` — a real half-open connection exists now, so
    // `poll(POLLOUT)` is the right wait primitive, bounded by whatever's
    // left of `timeout`. `EINTR` here means the `poll()` CALL itself was
    // interrupted (not the connection) — recompute the remaining budget
    // and poll again, rather than surfacing `Interrupted` to the caller.
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("timed out after {timeout:?}")));
        }
        let millis = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX).max(1);
        let mut pfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLOUT, revents: 0 };
        // SAFETY: `pfd` names exactly the one fd this function owns,
        // polled for exactly one event.
        let poll_rc = unsafe { libc::poll(&mut pfd, 1, millis) };
        if poll_rc == 0 {
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("timed out after {timeout:?}")));
        }
        if poll_rc < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        break;
    }

    // The connect finished one way or the other — SO_ERROR says which.
    let mut sock_err: libc::c_int = 0;
    let mut sock_err_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `sock_err`/`sock_err_len` are correctly sized, exclusively
    // owned out-params for `SO_ERROR` on this function's own `fd`.
    let rc = unsafe {
        libc::getsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            &mut sock_err as *mut libc::c_int as *mut libc::c_void,
            &mut sock_err_len,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    if sock_err != 0 {
        return Err(io::Error::from_raw_os_error(sock_err));
    }

    set_fd_nonblocking(fd.as_raw_fd(), false)?;
    Ok(UnixStream::from(fd))
}

/// The bounded connect, natively: `win_unix`'s own `connect_timeout`, which
/// is the Windows arm of the very thing the Unix arm above hand-rolls. The
/// CONTRACT is the one above, whole: an `io::Result<UnixStream>` bounded by
/// `timeout`, `TimedOut` when the budget runs out, and the socket's own verdict
/// otherwise. The mechanism differs by host, and that is the point of the seam
/// — `EAGAIN`/`EINPROGRESS`/`POLLOUT` are the Linux kernel's spelling of what a
/// Winsock socket says with `WSAEWOULDBLOCK` and a writability wait, so neither
/// host pretends to run the other's code.
#[cfg(windows)]
pub fn connect_bounded(socket_path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    UnixStream::connect_timeout(socket_path, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use crate::win_unix::UnixListener;

    fn tmp_socket_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aoide-local-socket-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs `f` on its own thread and panics unless it returns within `limit`.
    /// A panic in `f` panics here too. `aoide-test-support`'s `finishes_within`
    /// cannot be used: it depends on this crate.
    fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(limit) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("did not return within {limit:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("the call panicked"),
        }
    }

    /// A path too long for `sun_path` is a clean, immediate `Err` — never a
    /// panic, and never reached via a raw slice-index that could.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn unix_sockaddr_rejects_a_path_too_long_for_sun_path() {
        let long = "/tmp/".to_string() + &"x".repeat(200);
        let err = unix_sockaddr(Path::new(&long)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    /// This target's own `sun_path` capacity — read off the struct in the
    /// test too, so the same assertions cover a 104-byte BSD/Haiku-width
    /// buffer, not only this runner's 108.
    #[cfg(unix)]
    fn sun_path_capacity() -> usize {
        // SAFETY: `sockaddr_un` is POD; an all-zero one is a valid value.
        unsafe { std::mem::zeroed::<libc::sockaddr_un>() }.sun_path.len()
    }

    /// The cap is the struct's own `sun_path` width, and the ACCEPTED
    /// maximum is that width minus the terminating zero: one byte under is
    /// taken (path bytes intact, terminator present), exactly the width is
    /// the clean `InvalidInput` refusal. `std`'s `SocketAddr::from_pathname`
    /// is asserted alongside so this is the shared contract, not a private
    /// restatement of it.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn unix_sockaddr_accepts_sun_path_minus_one_and_rejects_the_width() {
        let cap = sun_path_capacity();
        let offset = std::mem::offset_of!(libc::sockaddr_un, sun_path);
        assert_eq!(cap + offset, std::mem::size_of::<libc::sockaddr_un>(), "sun_path is the struct's last field");

        let max = "x".repeat(cap - 1);
        let (addr, len) = unix_sockaddr(Path::new(&max)).unwrap();
        assert_eq!(len as usize, offset + cap, "the accepted maximum fills sun_path and its terminator");
        let written: Vec<u8> = addr.sun_path.iter().map(|&c| c as u8).collect();
        assert_eq!(&written[..cap - 1], max.as_bytes(), "the accepted maximum's bytes must land verbatim");
        assert_eq!(written[cap - 1], 0, "the terminating zero must be in place at the cap");
        assert!(std::os::unix::net::SocketAddr::from_pathname(&max).is_ok());

        let at_cap = "x".repeat(cap);
        assert_eq!(unix_sockaddr(Path::new(&at_cap)).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(std::os::unix::net::SocketAddr::from_pathname(&at_cap).is_err());
    }

    /// The length handed to `connect(2)` is `offsetof(sun_path)` + the path
    /// + its terminating NUL, and the bytes in the buffer are the caller's
    /// path followed by zeros — checked both directly and by round-tripping
    /// the same path through `std`, which reads the buffer back the way the
    /// kernel does.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn unix_sockaddr_reports_offset_plus_path_plus_terminator() {
        let path = Path::new("/tmp/aoide-secrets-test.sock");
        let bytes = path.as_os_str().as_bytes();
        let offset = std::mem::offset_of!(libc::sockaddr_un, sun_path);

        let (addr, len) = unix_sockaddr(path).unwrap();
        assert_eq!(len as usize, offset + bytes.len() + 1);
        assert_eq!(addr.sun_family, libc::AF_UNIX as libc::sa_family_t);
        let written: Vec<u8> = addr.sun_path.iter().map(|&c| c as u8).collect();
        assert_eq!(&written[..bytes.len()], bytes);
        assert_eq!(written[bytes.len()], 0, "the terminating zero must follow the path");
        assert!(written[bytes.len() + 1..].iter().all(|&b| b == 0), "nothing but the terminator follows the path");
        assert_eq!(std::os::unix::net::SocketAddr::from_pathname(path).unwrap().as_pathname(), Some(path));
    }

    /// A NUL anywhere in the path is refused (never a silently truncated
    /// address), and an EMPTY path is the zero-length address rather than an
    /// error — in both cases the same answer `std`'s own
    /// `SocketAddr::from_pathname` gives for the same input.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn unix_sockaddr_null_and_empty_paths_match_std() {
        let nul = Path::new("a\u{0}b");
        let ours = unix_sockaddr(nul).unwrap_err();
        let theirs = std::os::unix::net::SocketAddr::from_pathname(nul).unwrap_err();
        assert_eq!(ours.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(theirs.kind(), io::ErrorKind::InvalidInput);

        let (addr, len) = unix_sockaddr(Path::new("")).unwrap();
        assert_eq!(len as usize, std::mem::offset_of!(libc::sockaddr_un, sun_path), "the empty path is the zero-length address");
        assert_eq!(addr.sun_path[0], 0);
        assert!(std::os::unix::net::SocketAddr::from_pathname("").unwrap().as_pathname().is_none());
    }

    /// Not only arithmetic: a real listener actually binds at a path exactly
    /// `sun_path.len() - 1` bytes long, and `connect_bounded` actually
    /// reaches it — the boundary accepted, over a real socket.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn connect_bounded_reaches_a_max_length_pathname() {
        let cap = sun_path_capacity();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let base = format!("{}/aoide-cap-{}-{}", std::env::temp_dir().display(), std::process::id(), nanos);
        assert!(base.len() < cap - 1, "temp dir too long to build a max-length pathname ({base})");
        let mut name = base;
        while name.len() < cap - 1 {
            name.push('x');
        }
        let sock = Path::new(&name).to_path_buf();
        assert_eq!(sock.as_os_str().as_bytes().len(), cap - 1);

        let _listener = UnixListener::bind(&sock).unwrap();
        let path = sock.clone();
        let stream = within(Duration::from_secs(10), move || connect_bounded(&path, Duration::from_secs(5))).unwrap();
        drop(stream);
        std::fs::remove_file(&sock).ok();
    }

    /// The ordinary case: a real listener at a real path connects well
    /// within the bound — proves the happy path never pays the poll/
    /// timeout machinery's cost (an immediate `connect()` success returns
    /// straight away, no `poll()` call at all).
    #[test]
    fn connect_bounded_succeeds_against_a_real_listener_fast() {
        let dir = tmp_socket_dir("ok");
        let sock = dir.join("s.sock");
        let _listener = UnixListener::bind(&sock).unwrap();

        let start = std::time::Instant::now();
        let stream = within(Duration::from_secs(10), move || connect_bounded(&sock, Duration::from_secs(5))).unwrap();
        assert!(start.elapsed() < Duration::from_millis(500), "a live listener must connect near-instantly");
        drop(stream);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A nonexistent path fails FAST with a real `NotFound`-shaped error —
    /// proves `connect_bounded` doesn't secretly block on the negative path
    /// either (the immediate `connect()` syscall itself returns `ENOENT`,
    /// never reaching the `poll()` branch at all).
    #[test]
    fn connect_bounded_fails_fast_against_a_nonexistent_socket() {
        let dir = tmp_socket_dir("dead");
        let dead = dir.join("nothing-here.sock");
        let start = std::time::Instant::now();
        let err = within(Duration::from_secs(10), move || connect_bounded(&dead, Duration::from_secs(5))).unwrap_err();
        assert!(start.elapsed() < Duration::from_millis(500), "a dead path must fail near-instantly, not wait out the bound");
        assert_ne!(err.kind(), io::ErrorKind::TimedOut, "ENOENT is not a timeout");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Hand-rolled listener + a single unaccepted connection.
    /// `UnixListener::bind` hardcodes a backlog of 128, far too large to
    /// saturate cheaply in a test, so this builds the listener directly with
    /// `libc::listen(fd, 1)` — the same raw-socket construction
    /// `connect_bounded`/`unix_sockaddr` already use in production code,
    /// reused here for the test's own
    /// setup. Returns the listening fd and the one filler fd occupying
    /// the single backlog slot; asserts the backlog is GENUINELY
    /// saturated (a probe connect must observe a real `EAGAIN`) before
    /// handing control to the caller, so this is never a simulated
    /// condition.
    #[cfg(unix)]
    fn saturate_backlog_of_one(sock: &Path) -> (RawFd, Vec<RawFd>) {
        let (addr, addr_len) = unix_sockaddr(sock).unwrap();

        // SAFETY: a fresh listening socket this test exclusively owns.
        let listen_fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        assert!(listen_fd >= 0, "socket: {}", io::Error::last_os_error());
        // SAFETY: `addr`/`addr_len` describe a valid `sockaddr_un` for
        // this exact fd's own address family, matching `connect_bounded`'s
        // own construction of the same struct.
        let bind_rc =
            unsafe { libc::bind(listen_fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr, addr_len) };
        assert_eq!(bind_rc, 0, "bind: {}", io::Error::last_os_error());
        // SAFETY: `listen_fd` is this test's own fd, backlog requested at
        // 1 — but Linux's actual accept-queue capacity for a given
        // `listen()` argument is a kernel implementation detail (commonly
        // rounded up by one, or more, for historical BSD-compat reasons),
        // so this is a REQUEST, not a hard guarantee of exactly one slot.
        let listen_rc = unsafe { libc::listen(listen_fd, 1) };
        assert_eq!(listen_rc, 0, "listen: {}", io::Error::last_os_error());

        // Fill the backlog with unaccepted connections until a connect
        // attempt genuinely observes `EAGAIN` — never assume the queue
        // holds exactly `listen()`'s own argument; PROVE saturation by
        // continuing to fill until the kernel itself refuses one, capped
        // so a kernel that (for whatever reason) never saturates fails
        // the test loudly instead of hanging.
        let mut fillers = Vec::new();
        loop {
            assert!(fillers.len() < 256, "backlog never saturated after 256 connects — test assumption invalid on this kernel");
            // SAFETY: a fresh client-side socket this test exclusively
            // owns, pushed into `fillers` (and closed by the caller) on
            // every path except the terminal EAGAIN below, where it is
            // the rejected attempt itself and closed immediately.
            let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
            assert!(fd >= 0, "socket: {}", io::Error::last_os_error());
            set_fd_nonblocking(fd, true).unwrap();
            // SAFETY: same address-struct contract as the bind above.
            let rc = unsafe { libc::connect(fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr, addr_len) };
            if rc == 0 {
                fillers.push(fd);
                continue;
            }
            let e = io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EINPROGRESS) => {
                    fillers.push(fd);
                }
                Some(libc::EAGAIN) => {
                    // SAFETY: this attempt was rejected — never queued,
                    // never handed to anything else.
                    unsafe { libc::close(fd) };
                    break;
                }
                _ => panic!("unexpected connect error while saturating the backlog: {e}"),
            }
        }
        assert!(!fillers.is_empty(), "the backlog accepted zero connections before EAGAIN — test setup invalid");

        (listen_fd, fillers)
    }

    /// The headline proof of the retry: `connect_bounded` must actually RETRY
    /// through a saturated backlog and succeed once a slot frees, not fail on
    /// the first `EAGAIN`. A background thread frees
    /// the one occupied slot (by accepting the filler connection) ~80ms
    /// in; `connect_bounded`'s own retry interval is 15ms, so it must
    /// notice well within its 3s budget — and the elapsed time must be
    /// LONG ENOUGH to prove it actually waited (not a lucky race past a
    /// backlog that was never really full).
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn connect_bounded_retries_through_a_saturated_backlog_until_a_slot_frees() {
        let dir = tmp_socket_dir("saturated-frees");
        let sock = dir.join("s.sock");
        let (listen_fd, fillers) = saturate_backlog_of_one(&sock);

        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            // SAFETY: accepting exactly one of the queued connections
            // `saturate_backlog_of_one` created; frees exactly one slot —
            // enough for `connect_bounded`'s own retry to take. The
            // LISTENER itself stays open (closing it would refuse every
            // further connect outright, which is not the condition this
            // test is proving) — closed by the main thread once
            // `connect_bounded` has already succeeded, below.
            let accepted = unsafe { libc::accept(listen_fd, std::ptr::null_mut(), std::ptr::null_mut()) };
            assert!(accepted >= 0, "accept: {}", io::Error::last_os_error());
            // SAFETY: `accepted` was never handed to anything else.
            unsafe { libc::close(accepted) };
        });

        let start = std::time::Instant::now();
        let stream = within(Duration::from_secs(10), move || connect_bounded(&sock, Duration::from_secs(3))).unwrap();
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(60), "should have genuinely waited for the slot to free: {elapsed:?}");
        assert!(elapsed < Duration::from_secs(1), "should succeed well within the 3s bound once the slot frees: {elapsed:?}");

        handle.join().unwrap();
        drop(stream);
        // SAFETY: none of these fds were ever handed to anything else.
        unsafe { libc::close(listen_fd) };
        for fd in fillers {
            unsafe { libc::close(fd) };
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The deadline half: when the backlog stays
    /// saturated for good (nothing ever accepts), `connect_bounded`'s
    /// `EAGAIN` retry loop must still respect the overall bound rather
    /// than retrying forever.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm's own body
    // (`unix_sockaddr` builds a `libc::sockaddr_un`; the backlog tests
    // call `libc::socket`/`connect` directly), and the Windows side is
    // `aoide_protocol::win_unix`, tested beside it. A gate with the
    // reason beats a rewritten test asserting a different mechanism.
    #[cfg(unix)]
    fn connect_bounded_times_out_when_the_backlog_stays_saturated() {
        let dir = tmp_socket_dir("saturated-stuck");
        let sock = dir.join("s.sock");
        let (listen_fd, fillers) = saturate_backlog_of_one(&sock);

        let start = std::time::Instant::now();
        let err = within(Duration::from_secs(10), move || connect_bounded(&sock, Duration::from_millis(200))).unwrap_err();
        let elapsed = start.elapsed();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");
        assert!(elapsed < Duration::from_millis(500), "must not overrun the bound by much: {elapsed:?}");

        // SAFETY: none of these fds were ever handed to anything else.
        unsafe { libc::close(listen_fd) };
        for fd in fillers {
            unsafe { libc::close(fd) };
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `UnixStream::connect` makes its socket close-on-exec, and so does this
    /// connect: a process that spawns children while it connects must not
    /// hand them the descriptor.
    #[test]
    #[cfg(unix)]
    fn a_bounded_connect_is_close_on_exec() {
        use std::os::unix::io::AsRawFd;
        let dir = tmp_socket_dir("cloexec");
        let sock = dir.join("s.sock");
        let _listener = UnixListener::bind(&sock).unwrap();

        let stream = within(Duration::from_secs(10), move || connect_bounded(&sock, Duration::from_secs(5))).unwrap();
        // SAFETY: `stream` owns its descriptor for as long as it is alive.
        let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0, "F_GETFD: {}", io::Error::last_os_error());
        assert_ne!(flags & libc::FD_CLOEXEC, 0, "a bounded connect's socket must be close-on-exec");
        drop(stream);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A zero timeout against a full backlog is `TimedOut` at once: the first
    /// refused attempt is already past the deadline.
    #[test]
    #[cfg(unix)]
    fn a_zero_timeout_against_a_full_backlog_times_out() {
        let dir = tmp_socket_dir("zero");
        let sock = dir.join("s.sock");
        let (listen_fd, fillers) = saturate_backlog_of_one(&sock);

        let err = within(Duration::from_secs(1), move || connect_bounded(&sock, Duration::ZERO)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");

        // SAFETY: none of these fds were ever handed to anything else.
        unsafe { libc::close(listen_fd) };
        for fd in fillers {
            unsafe { libc::close(fd) };
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A timeout the clock cannot hold is refused as `InvalidInput` before any
    /// socket is made. `Instant + Duration::MAX` panics, so this pins the
    /// checked add.
    #[test]
    #[cfg(unix)]
    fn a_timeout_past_the_clock_is_refused() {
        let dir = tmp_socket_dir("clock");
        let sock = dir.join("s.sock");
        let _listener = UnixListener::bind(&sock).unwrap();

        let err = within(Duration::from_secs(1), move || connect_bounded(&sock, Duration::MAX)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
