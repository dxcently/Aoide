//! The local socket, natively, on Windows: `AF_UNIX` (Winsock, Windows 10
//! 1803+) behind the same surface the Unix callers use from
//! `std::os::unix::net`.
//!
//! | caller's need | Unix (`std::os::unix::net`) | here |
//! | --- | --- | --- |
//! | the connect side | `UnixStream::connect` | `UnixStream::connect` (`connect`) |
//! | a bounded connect | hand-rolled `fcntl`+`connect`+`poll` | `UnixStream::connect_timeout` (`select`) |
//! | the accept side | `UnixListener::bind`/`accept`/`incoming` | the same (`bind`/`listen`/`accept`) |
//! | the read/write halves | `Read`/`Write` | `Read`/`Write` (`recv`/`send`) |
//! | a second handle | `try_clone` (`dup`) | `try_clone` (`WSADuplicateSocketW`+`WSASocketW`) |
//! | read/write deadlines | `set_read_timeout`/`set_write_timeout` | the same (`SO_RCVTIMEO`/`SO_SNDTIMEO`) |
//! | half-close | `shutdown(Shutdown)` | the same (`shutdown`/`SD_*`) |
//! | a connected pair | `UnixStream::pair` (`socketpair`) | `UnixStream::pair` (a private listener) |
//! | the caller's identity | `getsockopt(SO_PEERCRED)` | `UnixStream::peer_pid` (`SIO_AF_UNIX_GETPEERPID`) |
//!
//! **Not named pipes, and not a second discovery path.** The User's decision
//! (2026-09-26) is Windows `AF_UNIX`, so a caller keeps ONE code shape: the
//! type it names under `cfg(unix)` is `std`'s and under `cfg(windows)` is
//! this one, and nothing else in the caller changes. Neither arm has a
//! dialect of its own to discover.
//!
//! **This module owns the socket, not the policy.** Who may connect is the
//! socket's directory policy (`owner_only`), what the peer's *user* is is
//! `win_proc::process_user_sid`, and what a caller does with that identity
//! is the caller's gate. Here: the bytes and the pid.
//!
//! **The `sun_path` budget is refused by name, on bind AND on connect.**
//! Windows' `SOCKADDR_UN.sun_path` is 108 bytes *including* the terminator,
//! so a longer path is refused before the call that would have truncated it
//! — a truncated path names a DIFFERENT socket than the caller asked for.
//! The message carries both numbers (what was given, what fits), because
//! "path too long" without the budget is a caller guessing.
//!
//! **Winsock is started once per process** ([`startup`]), from whichever
//! call reaches it first — `WSAStartup` is refcounted by Winsock itself and
//! the version asked for is 2.2, the version `AF_UNIX` requires.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::Networking::WinSock::{
    accept, bind, closesocket, connect, getsockopt, listen, recv, select, send, setsockopt, shutdown, socket,
    WSAGetLastError, WSAIoctl, WSAStartup, AF_UNIX, FD_SET, INVALID_SOCKET, SOCKADDR, SOCKADDR_UN, SOCKET,
    SOCKET_ERROR, SOCK_STREAM, SOL_SOCKET, SO_ERROR, SO_RCVTIMEO, SO_SNDTIMEO, TIMEVAL, WSAEALREADY, WSAEINPROGRESS,
    WSAEINTR, WSAEWOULDBLOCK, WSADATA, SD_BOTH, SD_RECEIVE, SD_SEND,
};use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// `SIO_AF_UNIX_GETPEERPID` — **spelled here rather than taken from
/// `windows-sys`**, which ships this control code one field short.
///
/// The SDK's own `shared/afunix.h` (10.0.26100, and every version that has
/// the socket family) defines it as `_WSAIOR(IOC_VENDOR, 256)`, which is
/// `0x5800_0100`. `windows-sys` 0.61.2 has `0x5800_0000` — the same macro
/// with its low byte dropped — and this host's `WSAIoctl` answers that code
/// with success and an untouched buffer, which reads as "this socket has no
/// peer pid" and is why the value is not trusted. The local constant is the
/// header's.
const SIO_AF_UNIX_GETPEERPID: u32 = 0x5800_0100;

/// The first call, once per process: `WSAStartup` 2.2, refcounted by Winsock
/// itself, with its failure reported as the Winsock error it was — every
/// socket call below is meaningless without it.
fn startup() -> io::Result<()> {
    static STARTED: OnceLock<i32> = OnceLock::new();
    let rc = *STARTED.get_or_init(|| {
        let mut data: WSADATA = zeroed();
        // SAFETY: `data` is a correctly sized, writable `WSADATA` for exactly
        // this call, which fills it and returns 0 or a Winsock error.
        unsafe { WSAStartup(0x0202, &mut data) }
    });
    if rc != 0 {
        return Err(ws_error(rc));
    }
    Ok(())
}

/// The last Winsock failure as an `io::Error` — `io::Error` knows the WSA
/// range (`WSAETIMEDOUT` -> `TimedOut`, `WSAECONNRESET` -> `ConnectionReset`,
/// …), so the kinds a Unix caller already matches on keep matching.
fn last_error() -> io::Error {
    ws_error(unsafe { WSAGetLastError() })
}

fn ws_error(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

/// The Win32 structs this module builds (`SOCKADDR_UN`, `WSADATA`,
/// `WSAPROTOCOL_INFOW`, `FD_SET`) are plain-old-data: an all-zero value is
/// valid and writing the named fields over it is the standard way to build
/// one.
fn zeroed<T>() -> T {
    // SAFETY: every type this is used for is a C struct of integers,
    // pointers and fixed arrays, for which all-zero is a valid value.
    unsafe { std::mem::zeroed() }
}

/// Windows' `SOCKADDR_UN.sun_path`, which is `CHAR[108]` — one byte of which
/// is the terminator — is the budget a path has to fit in. The number is read
/// off the struct so this stays true if the struct changes.
fn sun_path_budget() -> usize {
    let addr: SOCKADDR_UN = zeroed();
    addr.sun_path.len() - 1
}

/// Build the address for `path`, or refuse by name. Two refusals, both
/// `InvalidInput` and both naming the fact that failed: an interior NUL (the
/// kernel would see a shorter name than the caller did) and a path over
/// [`sun_path_budget`] (the kernel would see a shorter name, too — silently).
fn sockaddr(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
    let text = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("AF_UNIX addresses are byte strings: {} is not valid UTF-8", path.display()),
        )
    })?;
    let bytes = text.as_bytes();
    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "AF_UNIX paths may not contain interior NUL bytes",
        ));
    }
    let budget = sun_path_budget();
    if bytes.len() > budget {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "AF_UNIX path is {} bytes; a Windows AF_UNIX path is at most {budget} bytes \
                 (`sun_path` is 108 including its terminator): {}",
                bytes.len(),
                path.display()
            ),
        ));
    }
    let mut addr: SOCKADDR_UN = zeroed();
    addr.sun_family = AF_UNIX;
    for (dst, &src) in addr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = src as i8;
    }
    // The terminator is the zero the struct already carries; only
    // `bytes.len()` bytes of `sun_path` were written over.
    let len = std::mem::offset_of!(SOCKADDR_UN, sun_path) + bytes.len() + 1;
    Ok((addr, len as i32))
}

/// The address `accept` reports for an accepted connection. An `AF_UNIX`
/// peer has no address of its own — it is the *listening* path that names the
/// connection, and the connecting side is unnamed on both hosts — so this is
/// `None` for the ordinary accept and carries a path only if the host reports
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketAddr {
    path: Option<PathBuf>,
}

impl SocketAddr {
    /// Build an address from a path, with `std::os::unix::net`'s own
    /// conventions: an empty path is the unnamed address and an interior NUL
    /// is refused.
    pub fn from_pathname(path: &Path) -> io::Result<Self> {
        let text = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "AF_UNIX addresses are byte strings")
        })?;
        if text.as_bytes().contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "AF_UNIX paths may not contain interior NUL bytes",
            ));
        }
        Ok(Self { path: if text.is_empty() { None } else { Some(PathBuf::from(text)) } })
    }

    /// The bound path, or `None` for the unnamed address.
    pub fn as_pathname(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Is this the unnamed address?
    pub fn is_unnamed(&self) -> bool {
        self.path.is_none()
    }
}

impl std::fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(p) => write!(f, "{}", p.display()),
            None => f.write_str("<unnamed>"),
        }
    }
}

/// A connected `AF_UNIX` stream — the `cfg(windows)` arm of the type Unix
/// callers name as `std::os::unix::net::UnixStream`.
pub struct UnixStream {
    sock: SOCKET,
}

/// An `AF_UNIX` name unique per CALL: `<tag>-<pid>-<seq>`. The sequence
/// number is what a pid-keyed name was missing — one process runs its tests
/// in PARALLEL, so a name keyed by pid alone is shared by every caller in it,
/// and two of them racing over one rendezvous directory is a real failure
/// (measured: this module's own pair test failed once in a full-suite run,
/// having read the directory's policy while another caller was still
/// attaching it).
fn unique_rendezvous_name(tag: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("aoide-{tag}-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed))
}

impl UnixStream {
    /// Connect to the socket file at `path`. An unbounded blocking connect is
    /// the local-socket default: `AF_UNIX` connects to a listener on this
    /// host, so there is no network path to wait on.
    pub fn connect(path: &Path) -> io::Result<UnixStream> {
        startup()?;
        let (addr, len) = sockaddr(path)?;
        let sock = unsafe { socket(AF_UNIX as i32, SOCK_STREAM, 0) };
        if sock == INVALID_SOCKET {
            return Err(last_error());
        }
        let stream = UnixStream { sock };
        if unsafe { connect(stream.sock, &addr as *const SOCKADDR_UN as *const SOCKADDR, len) } == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(stream)
    }

    /// Connect with a bound on the attempt — the native arm of the Unix
    /// caller's hand-rolled nonblocking `connect(2)`, which exists only
    /// because `std`'s `UnixStream` has no `connect_timeout`.
    ///
    /// The attempt is put on a nonblocking socket, and the two outcomes that
    /// are about the ATTEMPT rather than the connection are waited on:
    /// `WSAEWOULDBLOCK`/`WSAEINPROGRESS` (the attempt is queued) selects on
    /// writability within what is left of `timeout`, then asks `SO_ERROR`
    /// whether it actually succeeded. `WSAEINTR` retries the call itself. A
    /// `WSAEALREADY` (the queued attempt is the one still in flight) is
    /// waited on the same way rather than treated as a new failure. Anything
    /// else is the attempt's own verdict and is returned as it is. Exhausting
    /// the budget is `TimedOut`, never a hang — the same verdict the Unix arm
    /// reaches from its own deadline.
    pub fn connect_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
        startup()?;
        let (addr, len) = sockaddr(path)?;
        let sock = unsafe { socket(AF_UNIX as i32, SOCK_STREAM, 0) };
        if sock == INVALID_SOCKET {
            return Err(last_error());
        }
        let stream = UnixStream { sock };
        stream.set_nonblocking(true)?;
        let deadline = Instant::now() + timeout;
        loop {
            let rc = unsafe { connect(stream.sock, &addr as *const SOCKADDR_UN as *const SOCKADDR, len) };
            if rc == 0 {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            match unsafe { WSAGetLastError() } {
                WSAEINTR => continue,
                WSAEWOULDBLOCK | WSAEINPROGRESS | WSAEALREADY => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() || !stream.wait_writable(remaining)? {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("timed out after {timeout:?} connecting to {}", path.display()),
                        ));
                    }
                    match stream.so_error()? {
                        0 => {
                            stream.set_nonblocking(false)?;
                            return Ok(stream);
                        }
                        // The queued attempt is still the one in flight: give
                        // it the rest of the budget.
                        code if code == WSAEINPROGRESS || code == WSAEWOULDBLOCK || code == WSAEALREADY => continue,
                        code => return Err(ws_error(code)),
                    }
                }
                _ => return Err(last_error()),
            }
        }
    }

    /// A connected pair, both ends live and this process's own — inside `dir`,
    /// which this CREATES and locks down (`owner_only::ensure_private_dir`)
    /// when it is not already there. The caller owns `dir` from then on: this
    /// deliberately never removes it, so a caller whose whole point is the
    /// rendezvous DIRECTORY (its policy read back, its name its own choice)
    /// keeps it. The pair is connected, not "almost": by the time this returns
    /// both directions are open and the rendezvous FILE is gone.
    ///
    /// Windows has no `socketpair(2)`, and an `AF_UNIX` pair needs a name to
    /// rendezvous on.
    pub fn pair_in(dir: &Path) -> io::Result<(UnixStream, UnixStream)> {
        startup()?;
        crate::owner_only::ensure_private_dir(dir)?;
        let path = dir.join(format!("{}.sock", unique_rendezvous_name("rendezvous")));
        // A rendezvous name outlives a crashed run, and `bind` refuses a name
        // that is already there: clear it so a stale file is never the reason
        // a pair cannot be made.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        let client = UnixStream::connect(&path)?;
        let (server, _) = listener.accept()?;
        // The rendezvous is over: unlink it (both ends are already open, so
        // the connection is unaffected).
        let _ = std::fs::remove_file(&path);
        Ok((client, server))
    }

    /// [`pair_in`] at a directory of this function's own choosing, unique per
    /// CALL (`aoide-pair-<pid>-<seq>`, never a pid-keyed name two parallel
    /// callers would share — see [`unique_rendezvous_name`]) and removed again
    /// on the way out: a directory this function invented is its own litter,
    /// not a caller's.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        startup()?;
        let dir = std::env::temp_dir().join(unique_rendezvous_name("pair"));
        let out = Self::pair_in(&dir);
        // The socket is already unlinked, so the directory is empty.
        let _ = std::fs::remove_dir(&dir);
        out
    }

    /// `SIO_AF_UNIX_GETPEERPID` — the pid at the other end of the connection,
    /// the one fact `AF_UNIX` gives about the peer.
    ///
    /// Answers on either end of a CONNECTED socket (both ends name the
    /// process on the far side) and refuses on a listening socket, which has
    /// no peer to name — the host says so itself, with `WSAEOPNOTSUPP`.
    ///
    /// **This host's provider does not report how many bytes it wrote**: the
    /// output buffer receives the `ULONG` and `lpcbBytesReturned` comes back
    /// zero. That field still has to be a valid pointer — `WSAIoctl` answers
    /// a null one with `WSAEFAULT` — so the success is the return value and
    /// the pid is the buffer. A zero pid is read as "nothing to report" and
    /// refused rather than returned: `0` is Windows' Idle pseudo-process,
    /// which no caller can be talking to (the same refusal
    /// `win_proc::is_alive` makes about the same pid).
    ///
    /// It is a point-in-time stamp, like `SO_PEERCRED`: read it at connection
    /// start, not later.
    pub fn peer_pid(&self) -> io::Result<u32> {
        let mut pid: u32 = 0;
        let mut written: u32 = 0;
        let rc = unsafe {
            WSAIoctl(
                self.sock,
                SIO_AF_UNIX_GETPEERPID,
                std::ptr::null(),
                0,
                &mut pid as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
                &mut written,
                std::ptr::null_mut(),
                None,
            )
        };
        if rc == SOCKET_ERROR {
            return Err(last_error());
        }
        if pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "this socket names no peer process (pid 0): only a connected AF_UNIX socket has one",
            ));
        }
        Ok(pid)
    }

    /// `try_clone` — a second handle on the same connection, for a caller
    /// that needs a reader and a writer at once. A Win32 socket IS a kernel
    /// handle, so this is `DuplicateHandle` against this process: both
    /// handles name the one connection, each closed on its own.
    pub fn try_clone(&self) -> io::Result<UnixStream> {
        let mut dup: HANDLE = std::ptr::null_mut();
        let ok = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.sock as HANDLE,
                GetCurrentProcess(),
                &mut dup,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(UnixStream { sock: dup as usize })
    }

    /// `set_read_timeout` — `SO_RCVTIMEO` on Windows takes milliseconds as a
    /// `DWORD`, and `0` means "no timeout", which is `None` here. A zero
    /// `Duration` is refused, as `std` refuses it on every host (it would be
    /// the same silent "no timeout" a caller asking for "return immediately"
    /// must not get).
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_timeout(SO_RCVTIMEO, timeout)
    }

    /// `set_write_timeout` — `SO_SNDTIMEO`, the write half of the above.
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_timeout(SO_SNDTIMEO, timeout)
    }

    fn set_timeout(&self, optname: i32, timeout: Option<Duration>) -> io::Result<()> {
        let millis: u32 = match timeout {
            None => 0,
            Some(d) if d.is_zero() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot set a 0 duration timeout: the socket would never wait for data",
                ))
            }
            Some(d) => u32::try_from(d.as_millis()).unwrap_or(u32::MAX),
        };
        let rc = unsafe {
            setsockopt(
                self.sock,
                SOL_SOCKET,
                optname,
                &millis as *const u32 as *const u8,
                std::mem::size_of::<u32>() as i32,
            )
        };
        if rc == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(())
    }

    /// `shutdown(2)` in `std`'s three-way spelling.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        if unsafe { shutdown(self.sock, how) } == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(())
    }

    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        use windows_sys::Win32::Networking::WinSock::{ioctlsocket, FIONBIO};
        let mut mode: u32 = u32::from(nonblocking);
        if unsafe { ioctlsocket(self.sock, FIONBIO, &mut mode) } == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(())
    }

    /// Wait until `sock` is writable or `timeout` runs out — `select`'s write
    /// set, which is Winsock's way to wait on a socket's state. `false` is
    /// the timeout, never an error.
    fn wait_writable(&self, timeout: Duration) -> io::Result<bool> {
        let mut write: FD_SET = zeroed();
        write.fd_count = 1;
        write.fd_array[0] = self.sock;
        let tv = TIMEVAL {
            tv_sec: i32::try_from(timeout.as_secs()).unwrap_or(i32::MAX),
            tv_usec: i32::try_from(timeout.subsec_micros()).unwrap_or(999_999),
        };
        let rc = unsafe { select(0, std::ptr::null_mut(), &mut write, std::ptr::null_mut(), &tv) };
        if rc == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(rc > 0)
    }

    /// `SO_ERROR`, then cleared by the read of it — Winsock's own "how did the
    /// connection actually go" report.
    fn so_error(&self) -> io::Result<i32> {
        let mut code: i32 = 0;
        let mut len = std::mem::size_of::<i32>() as i32;
        let rc = unsafe {
            getsockopt(
                self.sock,
                SOL_SOCKET,
                SO_ERROR,
                &mut code as *mut i32 as *mut u8,
                &mut len,
            )
        };
        if rc == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(code)
    }
}

/// One `recv` into `buf`, off the raw socket — the body both `Read` impls
/// below share, because a `&UnixStream` cannot hand out a `&mut UnixStream`
/// for the owned impl to borrow.
fn recv_into(sock: SOCKET, buf: &mut [u8]) -> io::Result<usize> {
    if buf.is_empty() {
        return Ok(0);
    }
    let want = buf.len().min(i32::MAX as usize) as i32;
    let n = unsafe { recv(sock, buf.as_mut_ptr(), want, 0) };
    if n == SOCKET_ERROR {
        return Err(last_error());
    }
    // 0 is the orderly shutdown, exactly as `read(2)` reports it.
    Ok(n as usize)
}

/// One `send` out of `buf`, off the raw socket — `recv_into`'s write half.
fn send_from(sock: SOCKET, buf: &[u8]) -> io::Result<usize> {
    if buf.is_empty() {
        return Ok(0);
    }
    let want = buf.len().min(i32::MAX as usize) as i32;
    let n = unsafe { send(sock, buf.as_ptr(), want, 0) };
    if n == SOCKET_ERROR {
        return Err(last_error());
    }
    Ok(n as usize)
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        recv_into(self.sock, buf)
    }
}

/// `std` implements `Read`/`Write` for `&UnixStream` as well as for the owned
/// type (a shared socket handle needs no locking to read or write), and every
/// caller that splits a connection reads and writes against a borrowed half —
/// so the same two impls exist here.
impl Read for &UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        recv_into(self.sock, buf)
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        send_from(self.sock, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for &UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        send_from(self.sock, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for UnixStream {
    fn drop(&mut self) {
        unsafe { closesocket(self.sock) };
    }
}

/// The handle is the only thing here worth naming in a diagnostic — the same
/// shape `std`'s own socket `Debug` has.
impl std::fmt::Debug for UnixStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnixStream").field("socket", &self.sock).finish()
    }
}

/// The listening half — the `cfg(windows)` arm of `UnixListener`.
pub struct UnixListener {
    sock: SOCKET,
}

impl UnixListener {
    /// Bind a listening socket to the socket file at `path`. The file is the
    /// kernel's; it is NOT removed on drop (`std`'s Unix listener does not
    /// either — a caller that owns the path removes the stale file before
    /// binding, which is what makes a restart work).
    pub fn bind(path: &Path) -> io::Result<UnixListener> {
        startup()?;
        let (addr, len) = sockaddr(path)?;
        let sock = unsafe { socket(AF_UNIX as i32, SOCK_STREAM, 0) };
        if sock == INVALID_SOCKET {
            return Err(last_error());
        }
        let listener = UnixListener { sock };
        if unsafe { bind(listener.sock, &addr as *const SOCKADDR_UN as *const SOCKADDR, len) } == SOCKET_ERROR {
            return Err(last_error());
        }
        // `SOMAXCONN`-shaped: the backlog the accept loop below is designed
        // to drain, with the per-connection thread each entry spawns.
        if unsafe { listen(listener.sock, 128) } == SOCKET_ERROR {
            return Err(last_error());
        }
        Ok(listener)
    }

    /// Accept one connection. The address an `AF_UNIX` accept can report is
    /// the listening path rather than a peer path — the connecting side is
    /// unnamed — so a caller that ignores it (every caller here does) loses
    /// nothing.
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let mut addr: SOCKADDR_UN = zeroed();
        let mut len = std::mem::size_of::<SOCKADDR_UN>() as i32;
        let sock = unsafe {
            accept(
                self.sock,
                &mut addr as *mut SOCKADDR_UN as *mut SOCKADDR,
                &mut len,
            )
        };
        if sock == INVALID_SOCKET {
            return Err(last_error());
        }
        Ok((UnixStream { sock }, sockaddr_of(&addr, len)))
    }

    /// An iterator over accepted connections — `for conn in listener.incoming()`
    /// is the accept loop every caller writes, and an error here ends that
    /// error's connection only, never the loop (`std` reports it the same way).
    pub fn incoming(&self) -> Incoming<'_> {
        Incoming { listener: self }
    }
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        unsafe { closesocket(self.sock) };
    }
}

impl std::fmt::Debug for UnixListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnixListener").field("socket", &self.sock).finish()
    }
}

/// The iterator [`UnixListener::incoming`] returns.
pub struct Incoming<'a> {
    listener: &'a UnixListener,
}

impl Iterator for Incoming<'_> {
    type Item = io::Result<UnixStream>;

    fn next(&mut self) -> Option<io::Result<UnixStream>> {
        Some(self.listener.accept().map(|(stream, _)| stream))
    }
}

/// Read a returned `SOCKADDR_UN` back into an address: the path is whatever
/// the kernel wrote between the family and the length it reported.
fn sockaddr_of(addr: &SOCKADDR_UN, len: i32) -> SocketAddr {
    let offset = std::mem::offset_of!(SOCKADDR_UN, sun_path) as i32;
    if addr.sun_family != AF_UNIX || len <= offset {
        return SocketAddr { path: None };
    }
    let path_len = (len - offset) as usize;
    let path_len = if path_len > addr.sun_path.len() { addr.sun_path.len() } else { path_len };
    let mut bytes: Vec<u8> = addr.sun_path[..path_len].iter().map(|&b| b as u8).collect();
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    SocketAddr { path: if text.is_empty() { None } else { Some(PathBuf::from(text)) } }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique directory per test, owner-only, under the process's temp dir —
    /// the same discipline `UnixStream::pair` uses. Uniqueness has to survive
    /// the PROCESS, not just the run: `bind` refuses a name that is already
    /// there, and a socket file outlives the test process that made it, so a
    /// per-process counter alone would hand the next run the last one's name
    /// and a `WSAEADDRINUSE` it did nothing to earn.
    fn scratch(name: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aoide-winunix-{}-{}-{}",
            name,
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        crate::owner_only::ensure_private_dir(&dir).expect("a private scratch directory");
        dir
    }

    /// The one place this module overrides its own dependency, pinned so a
    /// crate bump cannot quietly re-break it in either direction: the SDK's
    /// control code is the one this host's Winsock accepts, and the value
    /// `windows-sys` 0.61.2 ships is rejected outright.
    #[test]
    fn the_peer_pid_control_code_is_the_one_this_host_accepts() {
        use windows_sys::Win32::Networking::WinSock::WSAIoctl;
        let (a, _b) = UnixStream::pair().expect("pair");
        let call = |code: u32| {
            let mut pid: u32 = 0;
            let mut written: u32 = 0;
            let rc = unsafe {
                WSAIoctl(
                    a.sock,
                    code,
                    std::ptr::null(),
                    0,
                    &mut pid as *mut u32 as *mut core::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                    None,
                )
            };
            (rc, pid, unsafe { WSAGetLastError() })
        };
        let (rc, pid, _) = call(SIO_AF_UNIX_GETPEERPID);
        assert_eq!(rc, 0, "the header's control code must be accepted");
        assert_eq!(pid, std::process::id(), "and must name the process at the other end");
        // The provider leaves the returned LENGTH at zero while filling the
        // buffer — the reason `peer_pid` reads the buffer and not the count.
        assert_eq!(call(SIO_AF_UNIX_GETPEERPID).1, std::process::id(), "and it does so every time");

        let (rc, _, err) = call(0x5800_0000);
        assert_eq!(rc, SOCKET_ERROR, "the crate's value is not a code this host knows");
        assert_eq!(err, 10045, "and the host says so with WSAEOPNOTSUPP, not silently");
    }

    #[test]
    fn startup_is_idempotent_and_succeeds_on_this_host() {
        startup().expect("WSAStartup 2.2 must succeed on a Windows host");
        startup().expect("and every call after the first must too");
    }

    #[test]
    fn a_listener_and_a_client_exchange_bytes_and_either_end_names_the_peer_pid() {
        let dir = scratch("roundtrip");
        let path = dir.join("s.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let mut client = UnixStream::connect(&path).expect("connect");
        let (mut server, _) = listener.accept().expect("accept");

        // The peer pid is the kernel's stamp on the connection: either end
        // names the process at the far end, and both ends here are us.
        assert_eq!(server.peer_pid().expect("accepted side peer pid"), std::process::id());
        assert_eq!(client.peer_pid().expect("connecting side peer pid"), std::process::id());

        client.write_all(b"ping\n").expect("write");
        let mut buf = [0u8; 5];
        server.read_exact(&mut buf).expect("read");
        assert_eq!(&buf, b"ping\n");

        server.write_all(b"pong\n").expect("write");
        client.read_exact(&mut buf).expect("read");
        assert_eq!(&buf, b"pong\n");
    }

    #[test]
    fn incoming_yields_a_usable_connection() {
        let dir = scratch("incoming");
        let path = dir.join("s.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let mut client = UnixStream::connect(&path).expect("connect");
        let mut accepted = listener.incoming().next().expect("one connection").expect("no accept error");
        client.write_all(b"x").expect("write");
        let mut byte = [0u8; 1];
        accepted.read_exact(&mut byte).expect("read");
        assert_eq!(byte[0], b'x');
    }

    #[test]
    fn pair_is_connected_both_ways_and_leaves_no_socket_file() {
        let (mut a, mut b) = UnixStream::pair().expect("pair");
        a.write_all(b"one").expect("write");
        let mut buf = [0u8; 3];
        b.read_exact(&mut buf).expect("read");
        assert_eq!(&buf, b"one");
        b.write_all(b"two").expect("write");
        a.read_exact(&mut buf).expect("read");
        assert_eq!(&buf, b"two");
        // `b` is the accepted end of the rendezvous, so it is the side that
        // carries the peer pid.
        assert_eq!(b.peer_pid().expect("pid"), std::process::id());
    }

    #[test]
    fn try_clone_shares_the_same_connection() {
        let (a, mut b) = UnixStream::pair().expect("pair");
        let message = b"via the clone";
        let mut clone = a.try_clone().expect("try_clone");
        clone.write_all(message).expect("write");
        let mut buf = vec![0u8; message.len()];
        b.read_exact(&mut buf).expect("read");
        assert_eq!(&buf, message);
    }

    #[test]
    fn shutdown_of_the_write_half_reaches_the_peer_as_end_of_file() {
        let (a, mut b) = UnixStream::pair().expect("pair");
        a.shutdown(Shutdown::Write).expect("shutdown");
        let mut buf = [0u8; 8];
        assert_eq!(b.read(&mut buf).expect("read"), 0, "the peer's write half is closed");
    }

    #[test]
    fn a_read_deadline_ends_a_read_that_has_no_data() {
        let (mut a, _b) = UnixStream::pair().expect("pair");
        a.set_read_timeout(Some(Duration::from_millis(50))).expect("set_read_timeout");
        let err = a.read(&mut [0u8; 1]).expect_err("a read with no data must time out");
        assert!(
            matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut),
            "SO_RCVTIMEO expiry is WouldBlock or TimedOut, got {err:?}"
        );
        // And the socket is still usable: a deadline is not a close.
        a.set_read_timeout(None).expect("clear the deadline");
    }

    #[test]
    fn a_zero_duration_timeout_is_refused_rather_than_read_as_no_timeout() {
        let (a, _b) = UnixStream::pair().expect("pair");
        let err = a.set_read_timeout(Some(Duration::ZERO)).expect_err("zero is refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn connect_timeout_connects_when_the_listener_is_there() {
        let dir = scratch("connect-timeout");
        let path = dir.join("s.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let stream = UnixStream::connect_timeout(&path, Duration::from_secs(5)).expect("bounded connect");
        let (_server, _) = listener.accept().expect("accept");
        drop(stream);
    }

    #[test]
    fn connect_timeout_reports_a_missing_socket_rather_than_hanging() {
        let dir = scratch("connect-missing");
        let path = dir.join("nobody-is-listening.sock");
        let err = UnixStream::connect_timeout(&path, Duration::from_secs(2)).expect_err("no listener");
        assert!(
            !matches!(err.kind(), io::ErrorKind::TimedOut),
            "a missing socket file is an immediate verdict, not a timeout: {err:?}"
        );
    }

    /// The budget is a property of the host's `SOCKADDR_UN`, and both sides
    /// that build one refuse the same way: by name, with the numbers.
    #[test]
    fn a_path_over_the_sun_path_budget_is_refused_by_name_on_bind_and_on_connect() {
        let budget = sun_path_budget();
        let dir = scratch("budget");
        let long = dir.join("x".repeat(budget));
        let given = long.to_str().expect("a UTF-8 scratch path").len();
        assert!(given > budget, "this fixture has to be over the budget, not under it");
        let named = format!("AF_UNIX path is {given} bytes; a Windows AF_UNIX path is at most {budget} bytes");

        let err = UnixListener::bind(&long).expect_err("bind must refuse an over-budget path");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains(&named), "the refusal names the length given and the budget: {err}");

        let err = UnixStream::connect(&long).expect_err("connect must refuse it too");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains(&named), "the same refusal on the connect side: {err}");

        let err = UnixStream::connect_timeout(&long, Duration::from_secs(1))
            .expect_err("and on the bounded connect");
        assert!(err.to_string().contains(&named), "the same refusal, before any wait: {err}");

        // The budget itself is accepted by the same builder, so the refusal
        // is a boundary and not "long paths never work".
        let (addr, len) = sockaddr(&PathBuf::from("/".repeat(budget))).unwrap_or_else(|e| {
            panic!("a path of exactly {budget} bytes must fit: {e}");
        });
        assert_eq!(addr.sun_family, AF_UNIX);
        assert_eq!(len as usize, std::mem::offset_of!(SOCKADDR_UN, sun_path) + budget + 1);
    }

    #[test]
    fn a_path_carrying_an_interior_nul_is_refused() {
        let path = PathBuf::from("a\0b");
        let err = match sockaddr(&path) {
            Ok(_) => panic!("an interior NUL must be refused"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("interior NUL"), "{err}");
    }

    /// An owner-only directory is what makes the socket file's own policy
    /// irrelevant (this module's caller locks the parent down); this asserts
    /// a pair really does rendezvous inside one rather than trusting the
    /// helper's name — and the directory is this test's OWN, so nothing else
    /// running in parallel can be attaching its policy underneath the read.
    #[test]
    fn a_pair_rendezvouses_inside_an_owner_only_directory() {
        let dir = std::env::temp_dir().join(super::unique_rendezvous_name("pair-test"));
        let _ = UnixStream::pair_in(&dir).expect("pair");
        assert_eq!(
            crate::owner_only::dir_privacy(&dir).expect("read back the policy"),
            None,
            "the rendezvous directory must read back as owner-only"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
