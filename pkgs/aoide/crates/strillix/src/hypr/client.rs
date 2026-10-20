//! The Hyprland request socket: the client list (`j/clients`), the bounded connect
//! behind it, and the workspace a window reports. Every read of the compositor in the
//! desktop goes through here.

use aoide_conduct::graph::normalize_addr;
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// The Hyprland workspace id of the client whose `address` matches `want` in a
/// decoded `hyprctl clients -j` array (`workspace.id` in the client JSON).
/// Pure over the decoded JSON — unit-testable without a compositor. Address
/// comparison is `0x`/case-tolerant via [`normalize_addr`] (the stored
/// `windowAddress` and hyprctl can disagree on both). `None` when the window is
/// absent from the list or carries no numeric `workspace.id` — the caller then
/// leaves the stored `workspace` untouched (degrade gracefully, never a panic).
pub(crate) fn client_workspace_for_address(clients: &[Value], want: &str) -> Option<i64> {
    let want = normalize_addr(want);
    if want.is_empty() {
        return None;
    }
    clients.iter().find_map(|c| {
        let a = c.get("address").and_then(Value::as_str)?;
        if normalize_addr(a) != want {
            return None;
        }
        c.get("workspace")
            .and_then(|w| w.get("id"))
            .and_then(Value::as_i64)
    })
}

/// The compositor's client list, decoded into a JSON array. It asks Hyprland's
/// request socket, `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`,
/// for `j/clients` (the JSON `hyprctl clients -j` prints), and reads the reply to EOF.
/// No `hyprctl` binary and no PATH lookup: the socket is the only route, so the answer
/// does not depend on the unit's PATH. Returns `None` whenever the
/// compositor cannot be consulted authoritatively: no `HYPRLAND_INSTANCE_SIGNATURE`
/// or `XDG_RUNTIME_DIR`, no socket, a connect, request or reply that outlasts the 2 s
/// budget, or a reply that is not a JSON array. The connect, the request and the whole
/// reply share that one budget, so a listener that never accepts, or a reply that
/// trickles, cannot hold the caller past it. This is the single clients-reading seam
/// every desktop consumer shares (the window sweep, the terminal roster and the tests),
/// so they all degrade identically (never a panic, never a false result).
pub fn hyprctl_clients() -> Option<Vec<Value>> {
    use std::io::{Read, Write};
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let left = || {
        deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
    };
    let mut stream = connect_bounded(&socket, left()?)?;
    stream.set_write_timeout(Some(left()?)).ok()?;
    stream.write_all(b"j/clients").ok()?;
    let mut reply = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        stream.set_read_timeout(Some(left()?)).ok()?;
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => reply.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    match serde_json::from_slice(&reply) {
        Ok(Value::Array(a)) => Some(a),
        _ => None,
    }
}

/// Connects to the unix socket at `path`, returning within `budget` even when the
/// listener's accept backlog is full. `UnixStream::connect` has no bound of its own: on
/// a full backlog it blocks until a slot frees. Linux applies `SO_SNDTIMEO` to that
/// blocking connect, so the bound is set on the socket before the connect runs. `None`
/// when the path does not fit `sun_path`, the socket cannot be made, or the connect fails
/// or times out.
#[cfg(unix)]
fn connect_bounded(path: &std::path::Path, budget: std::time::Duration) -> Option<UnixStream> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::FromRawFd;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const CLOEXEC: libc::c_int = libc::SOCK_CLOEXEC;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    const CLOEXEC: libc::c_int = 0;
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: `sockaddr_un` is plain data, and all-zero is a valid value of it.
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= addr.sun_path.len() {
        return None;
    }
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, src) in addr.sun_path.iter_mut().zip(bytes) {
        *dst = *src as libc::c_char;
    }
    let len = std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1;
    // SAFETY: a plain syscall; a negative result is refused before the descriptor is used.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | CLOEXEC, 0) };
    if fd < 0 {
        return None;
    }
    // SAFETY: `fd` is a fresh descriptor this call owns, and the stream closes it on drop.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    let tv = libc::timeval {
        tv_sec: budget.as_secs() as libc::time_t,
        tv_usec: budget.subsec_micros() as libc::suseconds_t,
    };
    // SAFETY: `tv` outlives the call, and its size is passed with the pointer.
    let bounded = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_SNDTIMEO,
            (&tv as *const libc::timeval).cast(),
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    } == 0;
    // SAFETY: `addr` is a live `sockaddr_un`, and `len` covers its family, the path bytes and the NUL.
    let connected = bounded
        && unsafe {
            libc::connect(fd, (&addr as *const libc::sockaddr_un).cast(), len as libc::socklen_t)
        } == 0;
    connected.then_some(stream)
}

#[cfg(windows)]
fn connect_bounded(path: &std::path::Path, _budget: std::time::Duration) -> Option<UnixStream> {
    UnixStream::connect(path).ok()
}
