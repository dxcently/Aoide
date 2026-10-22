//! The `secrets exec` client — RELEASE TO CLIENT, the plan's one subtle
//! decision (this crate's README's "Release-to-client flow"). The broker
//! never execs the agent's command: it runs as the secrets uid (wrong cwd/
//! env, and the child would inherit secrets privileges). Instead THIS
//! process — running as the CALLING uid — resolves the secret over the
//! socket, then execs the wrapped command itself, `Stdio::inherit()`
//! throughout.
//!
//! The resolved value exists ONLY as a local `String` here, from
//! [`resolve`]'s return to the `.env(...)` call inside
//! [`spawn_with_secret`] — never argv, never an `Outcome`/JSON envelope,
//! never either audit log (both audit lines are written BROKER-side,
//! before the value is ever released — see `broker`'s module doc).
//! [`resolve`] extracts the value straight out of the reply's
//! `serde_json::Value` into that local `String`; there is no
//! `#[derive(Serialize)]` struct anywhere in this crate with a `value`
//! field for it to land on (this crate's `AGENTS.md` invariant) — a
//! `serde_json::Value` read at the use site, not a named reusable type, is
//! the shape that honors it.
//!
//! **`put` (P-V4c, `secrets put <name>`) is the write-side mirror, and
//! flows the OTHER direction**: [`run_put`] reads the value from THIS
//! process's own stdin (stdin-only intake — never argv, never a `--value`
//! flag) into a local `String`, hands it straight to [`put`], which builds
//! the wire request with `serde_json::json!` at the point of use (same
//! rule as `resolve`'s request — never a `#[derive(Serialize)]` struct)
//! and sends it. `put`'s reply carries no value at all (just `{"ok":true}`
//! or `{"ok":false,"error":...}`), so unlike `exec`, `secrets put` is a
//! PLAIN registered handler (`commands::handle_secrets_put`), not a
//! `cli`-crate `special`-hook case: nothing about its control flow needs
//! to bypass the generic `Outcome` envelope or return a spawned child's
//! own exit code (`commands.rs`'s own module doc justifies this choice
//! next to `serve`/`exec`/`enroll`'s).
//!
//! **P-V4e: `run_put` prompts and hides input when stdin is a terminal.**
//! A piped/redirected stdin (`printf %s hunter2 | aoide secrets put t`,
//! the historical shape, and every existing test/script) is BYTE-IDENTICAL
//! to before — [`stdin_is_tty`] is false in that case and `run_put` falls
//! straight through to the old `read_to_string` path, untouched. Only when
//! stdin IS a terminal ([`stdin_is_tty`] true — `libc::isatty` on fd 0,
//! already a dependency via `enroll::local_hostname`'s `gethostname`, no
//! new crate) does [`read_hidden_line`] take over, which prompts and hides
//! input on STDERR (never stdout — stdout stays clean for scripting).
//! **P-I1: the hiding mechanism is `aoide_protocol::pick::hidden_input`**
//! (`inquire::Password`, ONBOARD.md's prompt substrate section) — this
//! function used to clear `ECHO` on stdin's own `termios` by hand and
//! restore it unconditionally afterward; that hand-rolled dance moved into
//! `aoide-protocol`, the ONE crate this workspace lets depend on `inquire`
//! directly, and `read_hidden_line`'s own name/signature/call sites are
//! untouched by the move.
//!
//! **P-67: `run_put` warns and confirms before an overwrite.** The
//! existence check is BROKER-SIDE — the client never fetches a value to
//! find out (that would be a `resolve`-shaped leak on an op that isn't
//! `resolve`, and a client-side file peek is impossible anyway, since the
//! client never runs as the secrets uid). [`put`] now takes an `overwrite`
//! bool and reports whether the broker's reply carries the distinct
//! `{"exists":true}` refusal via [`PutError::Exists`] — never inferred by
//! matching the `error` string's prose. `run_put`'s flow:
//! - The value is read from stdin EXACTLY as before (tty-hidden or piped),
//!   held in a local `String`, and the first `put` attempt sends
//!   `overwrite: force` (the new `--force` flag, `commands::
//!   handle_secrets_put`) — `--force` therefore skips the confirmation on
//!   BOTH a tty and a pipe, storing on the first round trip either way.
//! - A [`PutError::Exists`] refusal (only reachable when `force` was
//!   false) branches on [`stdin_is_tty`] a SECOND time: on a tty, it
//!   prints a `y/N` confirmation prompt (unhidden — a yes/no answer isn't
//!   sensitive) to stderr and reads one line; `y`/`yes` (case-insensitive)
//!   re-sends the SAME in-memory value with `overwrite: true` (the caller
//!   is never asked to retype it), anything else — including EOF, `read_line`
//!   returning `Ok(0)` — aborts with an "unchanged" message. On a non-tty
//!   stdin, there is no one to ask, so it refuses outright
//!   ([`PutFailure::NeedsForce`]; the command layer teaches the `--force`
//!   spelling).
//! - The value NEVER touches argv, a log line, or a cache at any point in
//!   this flow — it exists only as `run_put`'s own local `String`, exactly
//!   as before this feature, just potentially handed to [`put`] TWICE
//!   instead of once.

use aoide_protocol::output::{io_cause, Fix, Kind, Refusal};
use aoide_protocol::Invocation;
use serde_json::{json, Value};
use std::io;
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::{FromRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use aoide_protocol::win_unix::UnixStream;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

/// The broker's reply for a secret no policy names.
pub const NOT_REGISTERED: &str = "secret not found";

/// The refusal text for a value that is empty or only whitespace, from the
/// broker's `put` and echoed by the command layer.
pub const EMPTY_VALUE: &str = "nothing arrived on stdin: the value is empty or only whitespace";

/// Markers [`describe_connect_error`]'s messages carry, read by `teach::classify`.
pub(crate) const NO_ACCESS: &str = "`aoide-secrets-access` group yet";
pub(crate) const NOT_RUNNING: &str = "doesn't look like it's running";

/// Map a `UnixStream::connect` failure against the broker socket into an
/// actionable message — pure (an injected [`io::Error`], no real socket),
/// so this is fully unit-tested without a live broker. This IS the exact
/// wall the User hit live (this task's own report): a bare "Permission
/// denied (os error 13)" with zero indication of what to do about it.
///
/// [`io::ErrorKind::PermissionDenied`]: the caller's own login session
/// isn't in the broker's `aoide-secrets-access` group yet — group
/// membership is login-scoped (`README.md`'s "Deployment" section), so a
/// `usermod -aG`/nix-module rebuild done from *this* shell never applies
/// until either a fresh login or an `sg` re-exec picks it up. Both fixes
/// are taught, since either genuinely works and which is more convenient
/// depends on the caller.
///
/// [`io::ErrorKind::NotFound`]/[`io::ErrorKind::ConnectionRefused`] — and, on
/// native Windows, `WSAENETDOWN`, which is what an `AF_UNIX` connect to a
/// path whose own directory is absent reports ([`nothing_is_listening`] owns
/// that fact): nothing is listening at `socket_path` at all — the broker
/// isn't running, or the resolved path doesn't match the deployed one
/// (`socket.rs`'s module doc: `AOIDE_SECRETS_SOCKET`, or its
/// `/run/aoide-secrets/secrets.sock` default).
///
/// Every other `io::ErrorKind` (a transient `EMFILE`, an unreadable
/// destination directory, ...) rides through with just the socket path
/// prefixed — unchanged from before this function existed — rather than
/// guessing at a fix this function has no evidence for.
pub(crate) fn describe_connect_error(socket_path: &Path, err: &io::Error, reinvoke: &str) -> String {
    match err.kind() {
        io::ErrorKind::PermissionDenied => format!(
            "connecting to the secrets broker at {}: permission denied — this session isn't in the \
             `aoide-secrets-access` group yet (group membership is login-scoped: joining the group \
             doesn't apply to an already-open shell). Fix: run `sg aoide-secrets-access -c '{reinvoke}'` \
             in THIS session, or log out and back in so a fresh session picks up the group.",
            socket_path.display()
        ),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => format!(
            "connecting to the secrets broker at {}: {err} — the broker doesn't look like it's running \
             (or the socket path is wrong). Check `systemctl status aoide-secrets-serve`, or set \
             AOIDE_SECRETS_SOCKET if this host's broker socket lives somewhere else.",
            socket_path.display()
        ),
        _ if nothing_is_listening(err) => format!(
            "connecting to the secrets broker at {}: {err} — the broker doesn't look like it's running \
             (or the socket path is wrong). Check `systemctl status aoide-secrets-serve`, or set \
             AOIDE_SECRETS_SOCKET if this host's broker socket lives somewhere else.",
            socket_path.display()
        ),
        _ => format!("connecting to the secrets broker at {}: {err}", socket_path.display()),
    }
}

/// [`describe_connect_error`] for an ADMIN op (`put`, the admin commands): a
/// refused connect there is taught as the `sudo -u aoide-secrets` line, since
/// the group's `sg` borrow would only lead into the broker's admin refusal.
/// Every other failure reads exactly as [`describe_connect_error`] says it.
pub(crate) fn describe_admin_connect_error(socket_path: &Path, err: &io::Error, reinvoke: &str) -> String {
    if err.kind() != io::ErrorKind::PermissionDenied {
        return describe_connect_error(socket_path, err, reinvoke);
    }
    format!(
        "connecting to the secrets broker at {}: permission denied — this session isn't in the \
         `aoide-secrets-access` group yet, and an admin command runs as the broker user anyway. \
         Fix: run `sudo -u aoide-secrets {reinvoke}`.",
        socket_path.display()
    )
}

/// **"Nothing is listening there" is ONE question, with one spelling per
/// host.** Unix spells a missing socket path `ENOENT`/`ECONNREFUSED`, which
/// Rust surfaces as [`io::ErrorKind::NotFound`]/
/// [`io::ErrorKind::ConnectionRefused`]. Native Windows spells the same fact
/// two ways: `ECONNREFUSED` for a path with nobody behind it, and —
/// `WSAENETDOWN` (10050, "a socket operation encountered a dead network") —
/// when the path's own DIRECTORY does not exist, a code Rust has no
/// `ErrorKind` for (`NetworkDown` is still unstable). Unread, an absent
/// broker on Windows becomes a hard failure instead of the "nobody is
/// listening" answer every caller's fallback is keyed on.
#[cfg(unix)]
fn nothing_is_listening(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
}

#[cfg(windows)]
fn nothing_is_listening(err: &io::Error) -> bool {
    /// `WSAENETDOWN` (`WinError.h`) — see this function's own doc for why it
    /// answers this question rather than a network-outage report.
    const WSAENETDOWN: i32 = 10050;
    matches!(err.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
        || err.raw_os_error() == Some(WSAENETDOWN)
}

/// The bound on the CONNECT half of every socket op this module makes
/// (rider task, alongside #75/#81/#82). `UnixStream::connect` alone can
/// block indefinitely if the broker's accept BACKLOG is saturated — every
/// `resolve` on a `requireTotp` secret can legitimately hold its own
/// connection parked for up to `park::park_timeout()` (default 300s), so a
/// burst of callers hitting an already-busy broker can queue at the kernel
/// listen-backlog level, before the broker's own thread-per-connection
/// accept loop (`broker.rs`'s module doc) ever gets a chance to shed load.
/// Every read this module already bounds (`resolve_bounded`'s
/// `set_read_timeout`) or leaves unbounded (`resolve`/`put`/`pending`/
/// `approve`/`dismiss` — an interactive human is expected to wait for
/// those); this constant closes the ONE gap none of them closed: the
/// connect itself. Fixed, no env override (unlike `BACKEND_TIMEOUT_ENV`/
/// `PARK_TIMEOUT_ENV`) — this is a defensive bound against a saturated
/// backlog, not an operational knob anyone has needed to tune yet; add one
/// the same tolerant-parsing way if that changes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The bound on the WHOLE `secrets status` round trip — the read half set
/// via `UnixStream::set_read_timeout`/`set_write_timeout` before the request
/// is written, on top of the [`CONNECT_TIMEOUT`] bound every op here already
/// carries. Unlike `resolve`/`put`/`pending`/`approve`/`dismiss` (which
/// deliberately leave the read unbounded — an interactive human is expected
/// to wait for those), `status` is a plain, read-only inventory question
/// with no human in the loop and no legitimate reason to take long: it
/// reads one `policy.json` the broker already has on disk, runs no backend,
/// and parks on nothing. A broker that cannot answer within this window
/// (a wedged/overloaded daemon, a half-open connection) is reported as an
/// honest failure instead of hanging the caller forever. Fixed, no env
/// override — same posture [`CONNECT_TIMEOUT`] documents.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(unix)]
/// Put `fd` into (or out of) non-blocking mode — same `fcntl(F_GETFL)`/
/// `fcntl(F_SETFL)` idiom `backend::set_nonblocking` already uses for a
/// backend child's output pipes, applied here to a socket fd instead.
#[cfg(unix)]
fn set_fd_nonblocking(fd: RawFd, nonblocking: bool) {
    // SAFETY: `fd` is a fd this function's caller owns for the duration of
    // this call (a freshly created socket, never shared); `fcntl(F_GETFL)`/
    // `fcntl(F_SETFL)` are ordinary, always-defined operations on any fd
    // this process holds.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags >= 0 {
            let next = if nonblocking { flags | libc::O_NONBLOCK } else { flags & !libc::O_NONBLOCK };
            libc::fcntl(fd, libc::F_SETFL, next);
        }
    }
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

/// Short sleep between retries of the raw `connect(2)` syscall itself,
/// on `EAGAIN` (review-bounce fix, this commit — see [`connect_bounded`]'s
/// own doc for why `EAGAIN` gets a retry loop rather than `poll()`).
#[cfg(unix)]
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(15);

/// Bounded replacement for `UnixStream::connect`, over TWO genuinely
/// different failure shapes a nonblocking `connect(2)` to an `AF_UNIX`
/// socket can return — `std`'s `UnixStream` has no `connect_timeout`
/// (unlike `TcpStream`), so both are hand-rolled on top of `libc` (already
/// a dependency, `Cargo.toml`'s own doc comment — zero new deps, the house
/// rule):
///
/// - **`EINPROGRESS`**: the kernel accepted the attempt and queued it — a
///   real half-open state exists, so `poll(POLLOUT)` is the right
///   primitive to wait on it, then `SO_ERROR` says whether it actually
///   succeeded.
/// - **`EAGAIN`**: on Linux, a saturated `AF_UNIX` listen backlog makes
///   `connect(2)` return `EAGAIN` IMMEDIATELY, never `EINPROGRESS` —
///   there is no half-open connection and no fd event to `poll()` for,
///   only a rejected ATTEMPT (review-bounce fix, this commit: the first
///   version of this function only special-cased `EINPROGRESS` and fell
///   through everything else, `EAGAIN` included, straight to an immediate
///   hard error — making the exact saturated-backlog scenario this
///   function exists for WORSE than the old blocking `UnixStream::connect`,
///   which would have slept in the kernel's `unix_wait_for_node()` and
///   succeeded once a slot freed). The fix is to retry the `connect(2)`
///   SYSCALL ITSELF on a short interval ([`CONNECT_RETRY_INTERVAL`]),
///   bounded by the same overall `timeout` budget — not to poll a fd for
///   an event that will never arrive.
///
/// Every caller in this module gets the identical `io::Result<UnixStream>`
/// shape `UnixStream::connect` already returned, so every existing
/// `.map_err(describe_connect_error(...))` call site needed no change
/// beyond the function name.
#[cfg(unix)]
fn connect_bounded(socket_path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let (addr, addr_len) = unix_sockaddr(socket_path)?;

    // SAFETY: a fresh AF_UNIX/SOCK_STREAM fd this function exclusively owns
    // from here on — handed to `UnixStream::from_raw_fd` on every success
    // path below, `libc::close`d on every error path, never both.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    set_fd_nonblocking(fd, true);

    let deadline = std::time::Instant::now() + timeout;

    // Phase 1: attempt the syscall itself, retrying on `EAGAIN`/`EINTR`
    // (both are about the ATTEMPT, not a queued connection) until it
    // either succeeds outright, reports `EINPROGRESS` (a real half-open
    // state — falls out of this loop into phase 2 below), or fails for
    // real.
    loop {
        // SAFETY: `addr`/`addr_len` describe a valid, fully-initialized
        // `sockaddr_un` for this exact `fd`'s own address family.
        let rc = unsafe { libc::connect(fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr, addr_len) };
        if rc == 0 {
            set_fd_nonblocking(fd, false);
            // SAFETY: `fd` is connected and owned solely by this function
            // up to this point; handing it to `UnixStream` transfers that
            // ownership exactly once.
            return Ok(unsafe { UnixStream::from_raw_fd(fd) });
        }

        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINPROGRESS) => break,
            Some(libc::EAGAIN) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    // SAFETY: `fd` was never handed to anything else.
                    unsafe { libc::close(fd) };
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("timed out after {timeout:?} connecting (the broker's connection backlog is saturated)"),
                    ));
                }
                std::thread::sleep(CONNECT_RETRY_INTERVAL.min(deadline - now));
                continue;
            }
            Some(libc::EINTR) => continue, // the syscall itself was interrupted — just retry it
            _ => {
                // SAFETY: `fd` was never handed to anything else.
                unsafe { libc::close(fd) };
                return Err(err);
            }
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
            // SAFETY: `fd` was never handed to anything else.
            unsafe { libc::close(fd) };
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("timed out after {timeout:?}")));
        }
        let millis = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX).max(1);
        let mut pfd = libc::pollfd { fd, events: libc::POLLOUT, revents: 0 };
        // SAFETY: `pfd` names exactly the one fd this function owns,
        // polled for exactly one event.
        let poll_rc = unsafe { libc::poll(&mut pfd, 1, millis) };
        if poll_rc == 0 {
            // SAFETY: `fd` was never handed to anything else.
            unsafe { libc::close(fd) };
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("timed out after {timeout:?}")));
        }
        if poll_rc < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            // SAFETY: `fd` was never handed to anything else.
            unsafe { libc::close(fd) };
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
            fd,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            &mut sock_err as *mut libc::c_int as *mut libc::c_void,
            &mut sock_err_len,
        )
    };
    if rc < 0 {
        let e = io::Error::last_os_error();
        // SAFETY: `fd` was never handed to anything else.
        unsafe { libc::close(fd) };
        return Err(e);
    }
    if sock_err != 0 {
        // SAFETY: `fd` was never handed to anything else.
        unsafe { libc::close(fd) };
        return Err(io::Error::from_raw_os_error(sock_err));
    }

    set_fd_nonblocking(fd, false);
    // SAFETY: same as the immediate-success path above.
    Ok(unsafe { UnixStream::from_raw_fd(fd) })
}

/// The bounded connect, natively — `aoide_protocol::win_unix`'s own
/// `connect_timeout`, which is the Windows arm of the very thing the Unix arm
/// above hand-rolls. The CONTRACT is the one above, whole: an
/// `io::Result<UnixStream>` bounded by `timeout`, `TimedOut` when the budget
/// runs out, and the socket's own verdict otherwise. The mechanism differs by
/// host, and that is the point of the seam — `EAGAIN`/`EINPROGRESS`/
/// `POLLOUT` are the Linux kernel's spelling of what a Winsock socket says
/// with `WSAEWOULDBLOCK` and a writability wait, so neither host pretends to
/// run the other's code.
#[cfg(windows)]
fn connect_bounded(socket_path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    UnixStream::connect_timeout(socket_path, timeout)
}

/// Parsed `secrets exec` arguments — pure, no I/O, fully unit-testable
/// without a running broker.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecArgs {
    pub consumer: String,
    pub secret: String,
    pub var: String,
    pub cmd: Vec<String>,
    /// Optional, self-asserted, DISPLAY-ONLY context for a popup/prompt
    /// surface to show ("release `db-prod` for: sudo nixos-rebuild switch")
    /// — `--reason` when the caller gave one, else [`derive_reason`]'s own
    /// auto-derivation from `cmd` (same discipline `argv0` already holds:
    /// never a value, never an env var, never gates anything).
    pub reason: Option<String>,
}

/// The env-var name a bare `--secret <name>` (no explicit `:VAR`) injects
/// under: the secret name, uppercased, `-` -> `_` (`db-prod` -> `DB_PROD`)
/// — the "documented default derivation" the phase brief calls for.
pub fn default_var_name(secret: &str) -> String {
    secret.to_uppercase().replace('-', "_")
}

/// Parse `aoide secrets exec --as <consumer> --secret <name>[:VAR] -- <cmd>`
/// out of an already-parsed [`Invocation`]. `inv.args` is exactly the wrapped
/// command + its args — `aoide_protocol::door::parse` already treats a bare
/// `--` as ending flag parsing, so everything after it arrives here verbatim
/// as positionals (that module's own doc). The registry entry declares
/// `--as`, `--secret` and the command required, so `Command::check` has
/// already refused their absence. A `--totp` is refused first of all: a code
/// never travels in argv, the parked ask asks for it.
pub fn parse_exec_args(inv: &Invocation) -> Result<ExecArgs, Refusal> {
    let fix = Fix::Run("the same exec without --totp: a parked ask asks for the code itself, at a prompt or from one piped line".to_string());
    if let Some(refusal) = totp_in_argv(inv, fix) {
        return Err(refusal);
    }
    let consumer = inv.flags["as"].clone();
    let secret_flag = inv.flags["secret"].clone();
    let (secret, var) = match secret_flag.split_once(':') {
        Some((n, v)) if !v.is_empty() => (n.to_string(), v.to_string()),
        _ => {
            let n = secret_flag.trim_end_matches(':').to_string();
            let derived = default_var_name(&n);
            (n, derived)
        }
    };
    if !crate::policy::valid_secret_name(&secret) {
        return Err(crate::teach::invalid_name(inv, "secret", &secret));
    }
    let cmd = inv.args.clone();
    let reason = inv.flags.get("reason").cloned().or_else(|| derive_reason(&cmd));
    Ok(ExecArgs { consumer, secret, var, cmd, reason })
}

/// Auto-derived when `--reason` is omitted: the wrapped command's own argv,
/// space-joined, truncated to ~60 chars — the SAME display-only discipline
/// `argv0` already holds on the wire (never a value, never an env var,
/// never gates anything; a popup/prompt surface simply shows it — see
/// `ExecArgs::reason`'s own doc). `None` only when `cmd` itself is empty
/// (never reached in practice — `parse_exec_args` already refuses an empty
/// `cmd` before this is called — but this function stays honest on its own
/// rather than assuming that invariant from outside).
fn derive_reason(cmd: &[String]) -> Option<String> {
    if cmd.is_empty() {
        return None;
    }
    let joined = cmd.join(" ");
    const MAX_CHARS: usize = 60;
    if joined.chars().count() <= MAX_CHARS {
        Some(joined)
    } else {
        let truncated: String = joined.chars().take(MAX_CHARS.saturating_sub(1)).collect();
        Some(format!("{truncated}\u{2026}"))
    }
}

/// Connect to `socket_path`, send ONE `resolve` request, read the wire's
/// FINAL reply line ([`read_final_reply`] — zero or more interim lines may
/// come first, P-N2c FIX 1), and return the value or a value-free error
/// message.
pub fn resolve(
    socket_path: &Path,
    secret: &str,
    consumer: &str,
    totp: Option<&str>,
    argv0: Option<&str>,
    reason: Option<&str>,
) -> Result<String, String> {
    resolve_with(socket_path, secret, consumer, totp, argv0, reason, &mut announce_interim)
}

fn resolve_with(
    socket_path: &Path,
    secret: &str,
    consumer: &str,
    totp: Option<&str>,
    argv0: Option<&str>,
    reason: Option<&str>,
    on_interim: &mut dyn FnMut(&Value),
) -> Result<String, String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT).map_err(|e| {
        describe_connect_error(
            socket_path,
            &e,
            &format!("aoide secrets exec --as {consumer} --secret {secret} -- ..."),
        )
    })?;

    let mut req = json!({ "op": "resolve", "secret": secret, "consumer": consumer });
    if let Some(t) = totp {
        req["totp"] = Value::String(t.to_string());
    }
    if let Some(a) = argv0 {
        req["argv0"] = Value::String(a.to_string());
    }
    // Optional, self-asserted, DISPLAY-ONLY context for why this ask exists
    // (`ExecArgs::reason`'s own doc) — a popup/prompt surface shows it
    // alongside the parked ask; the broker never gates on it.
    if let Some(r) = reason {
        req["reason"] = Value::String(r.to_string());
    }
    let mut line = req.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let reply = read_final_reply_with(&mut reader, on_interim)?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        reply
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "the secrets broker's reply had no `value`".to_string())
    } else {
        Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string())
    }
}

/// Read wire lines until the FINAL (non-interim) reply — the framing
/// contract P-N2c FIX 1 establishes (`CONTRACTS.md`'s Transport paragraph,
/// `broker.rs`'s module doc): "one request line -> zero or more interim
/// lines (`"interim":true`) -> exactly one final reply line." Every
/// interim line is surfaced via [`announce_interim`] (STDERR only, never
/// stdout — stdout stays clean for scripting) and then discarded; the
/// first line WITHOUT `"interim":true` is the final reply this function
/// returns. Only [`resolve`] can ever receive an interim line today (the
/// only op that parks) — kept as its own function rather than inlined so a
/// future op gaining interim lines reuses this loop instead of
/// re-deriving it.
fn read_final_reply(reader: &mut impl BufRead) -> Result<Value, String> {
    read_final_reply_with(reader, &mut announce_interim)
}

fn read_final_reply_with(reader: &mut impl BufRead, on_interim: &mut dyn FnMut(&Value)) -> Result<Value, String> {
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| io_cause("read from", Path::new("the secrets broker connection"), &e).why)?;
        if line.trim().is_empty() {
            return Err("the secrets broker closed the connection with no reply".to_string());
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("the secrets broker sent an unparseable reply: {e}"))?;
        if value.get("interim").and_then(Value::as_bool) == Some(true) {
            on_interim(&value);
            continue;
        }
        return Ok(value);
    }
}

/// Resolve a secret's value BOUNDED — an explicit socket read `timeout`
/// PLUS `wait:false` on the wire — for a caller with no human to type a
/// TOTP code and that must never hang waiting for one (task #84: the A2A
/// door's inbound bearer check, and its outbound client's per-node bearer
/// presentation — see `crates/server/src/a2a.rs`'s consumers of this
/// function). Two independent bounds, not one:
///
/// - `wait:false` is the wire's OWN documented escape hatch for exactly
///   this caller shape (`CONTRACTS.md`'s "Secrets wire" section) — a
///   `requireTotp` secret with no `automation`-open exemption for the
///   asserted `consumer` denies IMMEDIATELY instead of parking, so the
///   deployed, automation-open happy path never even reaches the timeout
///   below at all.
/// - `timeout` (via `UnixStream::set_read_timeout`, set BEFORE the request
///   is written) caps the socket READ regardless of why the broker might
///   still be slow to answer — a defense-in-depth second bound, not the
///   primary mechanism.
///
/// **NO CACHING**: every call is a fresh connect → one request → one reply.
/// Nothing this function returns is ever stored anywhere by it; the caller
/// owns the value for exactly as long as its own request needs it — this
/// crate's "NO CACHE, EVER" invariant, extended to every caller of this
/// function exactly as it already binds [`resolve`]/`secrets exec`.
pub fn resolve_bounded(
    socket_path: &Path,
    secret: &str,
    consumer: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT).map_err(|e| {
        describe_connect_error(
            socket_path,
            &e,
            &format!("aoide secrets exec --as {consumer} --secret {secret} -- ..."),
        )
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| io_cause("set a read timeout on", socket_path, &e).why)?;

    let req = json!({ "op": "resolve", "secret": secret, "consumer": consumer, "wait": false });
    let mut line = req.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let reply = read_final_reply(&mut reader)?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        reply
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "the secrets broker's reply had no `value`".to_string())
    } else {
        Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string())
    }
}

/// Surface ONE interim line to the human at the terminal — STDERR only,
/// never stdout (module doc's own discipline). Today the only interim
/// shape the broker ever sends is a park announcement
/// (`{"interim":true,"parked":true,"id":...,"timeoutSecs":...}`) — this
/// only prints for THAT shape; an interim line with a different shape (a
/// future mode) is still safely consumed by [`read_final_reply`]'s loop
/// even when this function has nothing to say about it yet.
fn announce_interim(value: &Value) {
    if value.get("parked").and_then(Value::as_bool) == Some(true) {
        let id = value.get("id").and_then(Value::as_str).unwrap_or("?");
        let timeout_secs = value.get("timeoutSecs").and_then(Value::as_u64).unwrap_or(0);
        let expires_at = value.get("expiresAt").and_then(Value::as_u64);
        eprintln!("{}", park_hint(id, timeout_secs, expires_at));
    }
}

/// A [`put`] failure, distinguishing the P-67 "already has a stored value"
/// refusal from every other error — via the wire's `exists` FLAG, never by
/// matching text out of the `error` string (the task's own requirement:
/// "distinguishable WITHOUT string-matching prose").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutError {
    /// `overwrite` was false/absent and the secret already has a stored
    /// value (the broker's `{"ok":false,"exists":true,...}` reply).
    Exists,
    /// Every other denial/error — connect failures, "secret not found", a
    /// backend problem, ... — value-free, same as before this feature.
    Other(String),
}

impl std::fmt::Display for PutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PutError::Exists => write!(f, "secret already has a stored value"),
            PutError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

/// Connect to `socket_path`, send ONE `put` request carrying `value` and
/// `overwrite`, read ONE reply line, and return whether an existing value
/// was REPLACED (`true`) or this was a first-ever store (`false`) — or a
/// [`PutError`] otherwise. Mirrors [`resolve`]'s one-shot socket shape;
/// `overwrite` only rides the wire when `true` (absent means false — wire
/// compat, `broker.rs`'s module doc), same discipline `resolve`'s optional
/// `totp`/`argv0` fields already hold.
pub fn put(socket_path: &Path, secret: &str, value: &str, overwrite: bool) -> Result<bool, PutError> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT).map_err(|e| {
        PutError::Other(describe_admin_connect_error(socket_path, &e, &format!("aoide secrets put {secret}")))
    })?;

    let mut req = json!({ "op": "put", "secret": secret, "value": value });
    if overwrite {
        req["overwrite"] = Value::Bool(true);
    }
    let mut line = req.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| PutError::Other(io_cause("write to", socket_path, &e).why))?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader
        .read_line(&mut reply_line)
        .map_err(|e| PutError::Other(io_cause("read from", socket_path, &e).why))?;
    if reply_line.trim().is_empty() {
        return Err(PutError::Other("the secrets broker closed the connection with no reply".to_string()));
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| PutError::Other(format!("the secrets broker sent an unparseable reply: {e}")))?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(reply.get("replaced").and_then(Value::as_bool).unwrap_or(false))
    } else if reply.get("exists").and_then(Value::as_bool) == Some(true) {
        Err(PutError::Exists)
    } else {
        Err(PutError::Other(
            reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("the secrets broker denied the request")
                .to_string(),
        ))
    }
}

/// Task #79: whether [`admin_request`]'s CONNECT attempt hit "nothing is
/// listening" ([`nothing_is_listening`]: `io::ErrorKind::NotFound` — no
/// socket file at all; `ConnectionRefused` — a stale socket file with nothing
/// behind it; and, on native Windows, `WSAENETDOWN` for a path whose
/// directory is absent, the same fact in that host's own spelling) — the
/// ONLY cases `commands.rs`'s admin commands fall back to their
/// direct-write path on ([`NoSocket`](AdminError::NoSocket)). Every other
/// failure — a different connect error, a write/read failure, an
/// unparseable reply, or the broker's own `{"ok":false}` domain denial
/// (a bad admin-identity peer uid, "no policy for secret x", a poisoned
/// `policy.json`) — is [`Other`](AdminError::Other) and MUST be reported,
/// never silently downgraded to a direct write: a live-but-sick daemon (a
/// permission error, a saturated backlog `connect_bounded` gave up
/// waiting on, or — the case this gate exists for — an authoritative
/// denial from the single-writer daemon) must never be bypassed into a
/// TOCTOU race against a direct write landing underneath it. This is the
/// SAME two-way split [`PutError`] already draws for `put`'s own
/// `exists`-vs-everything-else distinction, extended here for a different
/// pair of cases.
pub enum AdminError {
    NoSocket,
    Other(String),
}

/// Task #79: connect to `socket_path`, send ONE `{"op":"admin",...}`
/// request (`req` already carries `op` and `command` — every field
/// `commands.rs`'s admin commands need to send, this function adds none of
/// its own), read ONE reply line, and return the parsed reply `Value` on
/// `{"ok":true}` — the caller (`commands.rs`) reads `message`/`changed`
/// off it exactly the way it would from a [`crate::admin::AdminOutcome`]
/// on the direct-write path, so the two paths report through the same
/// shape. See [`AdminError`] for the fallback-vs-report split; this is the
/// ONE place that split is decided; a new admin command added later sends its
/// own `req` through this SAME function, never a hand-rolled write/read
/// pair.
pub fn admin_request(socket_path: &Path, req: Value) -> Result<Value, AdminError> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT).map_err(|e| {
        if nothing_is_listening(&e) {
            AdminError::NoSocket
        } else {
            AdminError::Other(describe_admin_connect_error(socket_path, &e, "aoide secrets <admin command> ..."))
        }
    })?;

    let mut line = req.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| AdminError::Other(io_cause("write to", socket_path, &e).why))?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader
        .read_line(&mut reply_line)
        .map_err(|e| AdminError::Other(io_cause("read from", socket_path, &e).why))?;
    if reply_line.trim().is_empty() {
        return Err(AdminError::Other("the secrets broker closed the connection with no reply".to_string()));
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| AdminError::Other(format!("the secrets broker sent an unparseable reply: {e}")))?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(reply)
    } else {
        Err(AdminError::Other(
            reply.get("error").and_then(Value::as_str).unwrap_or("the secrets broker denied the request").to_string(),
        ))
    }
}

/// One parked ask, as `secrets pending` lists it — id/secret/consumer/
/// requestedAt/peerUid ONLY, never a value (mirrors the wire's own
/// `pending` reply shape, `broker.rs`'s module doc's wire table).
/// `peer_uid` (task #73) is the kernel-truth `SO_PEERCRED` uid stamped at
/// park time — additive over the pre-#73 wire shape, `None` both when the
/// field is entirely absent (an older broker) and when the broker sent an
/// explicit `null` (an unidentified connection at park time) — a caller of
/// this struct has no need to tell those two apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAsk {
    pub id: String,
    pub secret: String,
    pub consumer: String,
    pub requested_at: u64,
    pub peer_uid: Option<u32>,
    /// Additive over `peer_uid` (the wire shape keeps the Unix number where
    /// it always was, so an old reader needs no change): native Windows'
    /// identity, the peer's token-user SID in its `S-1-5-…` form. `None`
    /// on every host that has uids, and ABSENT from the wire there rather
    /// than null.
    pub peer_sid: Option<String>,
    /// Additive over the pre-P3 shape (`peer_uid`'s own precedent, task
    /// #73) — the wire's optional, self-asserted, display-only
    /// `resolve.reason`, `None` when the caller sent none.
    pub reason: Option<String>,
    /// Additive again — the argv0 the resolve named, clipped at park time: the
    /// program the requester says receives the value. A claim, shown as one.
    pub argv0: Option<String>,
    /// Additive again — best-effort "who/where this ask came from"
    /// (`park::AskOrigin`'s own doc), captured broker-side at park time.
    pub origin: PendingOrigin,
}

/// The wire-parsed mirror of `park::AskOrigin` — every field best-effort and
/// DISPLAY-ONLY (that struct's own doc); kept as this crate's own client-side
/// type rather than reusing `park::AskOrigin` directly since a wire reply is
/// parsed data, not the broker's own in-memory registry row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingOrigin {
    pub username: Option<String>,
    pub pid: Option<i64>,
    pub comm: Option<String>,
    pub hostname: Option<String>,
}

/// Connect to `socket_path`, send ONE `pending` request, read ONE reply
/// line, and return every parked ask — value-free by construction (the
/// wire's `pending` reply never carries one; this simply reads the fields
/// that ARE there).
pub fn pending(socket_path: &Path) -> Result<Vec<PendingAsk>, String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT)
        .map_err(|e| describe_connect_error(socket_path, &e, "aoide secrets pending"))?;

    let line = json!({ "op": "pending" }).to_string() + "\n";
    stream.write_all(line.as_bytes()).map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader.read_line(&mut reply_line).map_err(|e| io_cause("read from", socket_path, &e).why)?;
    if reply_line.trim().is_empty() {
        return Err("the secrets broker closed the connection with no reply".to_string());
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| format!("the secrets broker sent an unparseable reply: {e}"))?;

    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string());
    }
    let asks = reply
        .get("pending")
        .and_then(Value::as_array)
        .ok_or_else(|| "the secrets broker's reply had no `pending` array".to_string())?;
    asks.iter()
        .map(|a| {
            Ok(PendingAsk {
                id: a.get("id").and_then(Value::as_str).ok_or("a pending entry had no `id`")?.to_string(),
                secret: a
                    .get("secret")
                    .and_then(Value::as_str)
                    .ok_or("a pending entry had no `secret`")?
                    .to_string(),
                consumer: a
                    .get("consumer")
                    .and_then(Value::as_str)
                    .ok_or("a pending entry had no `consumer`")?
                    .to_string(),
                requested_at: a
                    .get("requestedAt")
                    .and_then(Value::as_u64)
                    .ok_or("a pending entry had no `requestedAt`")?,
                // #73: additive — absent (an older broker) and an explicit
                // `null` (unidentified at park time) both read as `None`.
                peer_uid: a.get("peerUid").and_then(Value::as_u64).map(|u| u as u32),
                peer_sid: a.get("peerSid").and_then(Value::as_str).map(str::to_string),
                // P3: additive again — same absent-or-null tolerance.
                reason: a.get("reason").and_then(Value::as_str).map(str::to_string),
                argv0: a.get("argv0").and_then(Value::as_str).map(str::to_string),
                origin: a
                    .get("origin")
                    .map(|o| PendingOrigin {
                        username: o.get("username").and_then(Value::as_str).map(str::to_string),
                        pid: o.get("pid").and_then(Value::as_i64),
                        comm: o.get("comm").and_then(Value::as_str).map(str::to_string),
                        hostname: o.get("hostname").and_then(Value::as_str).map(str::to_string),
                    })
                    .unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, &str>>()
        .map_err(str::to_string)
}

/// Connect to `socket_path`, send ONE `approve` request carrying `id` and
/// `totp`, read ONE reply line. Success is `{"ok":true}` only — never a
/// value (the value went down the ORIGINAL parked connection, broker-side;
/// this function's own reply can't carry it because the wire reply it reads
/// never has one, `handle_approve`'s own doc). An invalid/expired code, or
/// an unknown id, comes back as a value-free `Err`.
pub fn approve(socket_path: &Path, id: &str, totp: &str) -> Result<(), String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT)
        .map_err(|e| describe_connect_error(socket_path, &e, &format!("aoide secrets approve {id}")))?;

    let line = json!({ "op": "approve", "id": id, "totp": totp }).to_string() + "\n";
    stream.write_all(line.as_bytes()).map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader.read_line(&mut reply_line).map_err(|e| io_cause("read from", socket_path, &e).why)?;
    if reply_line.trim().is_empty() {
        return Err("the secrets broker closed the connection with no reply".to_string());
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| format!("the secrets broker sent an unparseable reply: {e}"))?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string())
    }
}

/// Connect to `socket_path`, send ONE `dismiss` request carrying `id`, read
/// ONE reply line. An unknown id is a taught error (`handle_dismiss`'s own
/// doc), value-free either way.
pub fn dismiss(socket_path: &Path, id: &str) -> Result<(), String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT)
        .map_err(|e| describe_connect_error(socket_path, &e, &format!("aoide secrets dismiss {id}")))?;

    let line = json!({ "op": "dismiss", "id": id }).to_string() + "\n";
    stream.write_all(line.as_bytes()).map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader.read_line(&mut reply_line).map_err(|e| io_cause("read from", socket_path, &e).why)?;
    if reply_line.trim().is_empty() {
        return Err("the secrets broker closed the connection with no reply".to_string());
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| format!("the secrets broker sent an unparseable reply: {e}"))?;

    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string())
    }
}

/// One secret's value-free metadata row, as `secrets status` reports it —
/// the wire's own `status` row, parsed field by field into this crate's own
/// type rather than passed through as a `serde_json::Value`. That
/// parsing is the point, not ceremony: every field below is a NAMED,
/// value-free policy fact, so a reply carrying anything else (an older or
/// future broker's `key`, a backend's `has`/`get` template, a value) simply
/// has nowhere to land here and can never reach a caller's `--json` — the
/// same "a `serde_json::Value` read at the use site, never a named field a
/// value could land on" discipline this crate's `AGENTS.md` holds for
/// `resolve`'s reply, applied in reverse (nothing here is ever a value, so
/// the type is a plain struct; the invariant it protects is that no extra
/// wire field is forwarded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSecret {
    /// The secret's policy nickname.
    pub name: String,
    /// The named backend the policy routes this secret's value through.
    pub backend: String,
    /// The policy's `requireTotp` bit (an access-gate fact — NOT whether an
    /// enrollment exists on this host, and never a TOTP byte).
    pub require_totp: bool,
    /// The policy's `consumers[]` allow-list (empty means "any consumer").
    pub consumers: Vec<String>,
    /// The automation gate's pair of facts.
    pub automation: StatusAutomation,
    /// Hosts the secret is shared to (empty until the mesh phase lands).
    pub shared_with: Vec<String>,
    /// Whether the secret may ever be released over a NON-LOCAL entry point.
    pub remote: bool,
    /// Whether a positively remote-origin caller may resolve it.
    pub allow_remote_origin: bool,
}

/// [`StatusSecret`]'s automation half — whether listed consumers may skip a
/// fresh TOTP code, and who is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusAutomation {
    pub enabled: bool,
    pub consumers: Vec<String>,
}

/// What ONE `status` reply said: the home the BROKER read `policy.json`
/// from (echoed back by the broker, never re-derived here — see [`status`])
/// and the inventory it found there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub home: String,
    pub secrets: Vec<StatusSecret>,
}

/// Pull a required string field off one `status` row, or a taught error
/// naming the row and the missing field — the same "a malformed reply is an
/// `Err`, never a silent default" discipline [`pending`]'s own parsing
/// holds.
fn row_str(row: &Value, field: &str, what: &str) -> Result<String, String> {
    row.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("the secrets broker's status reply had no `{field}` for {what}"))
}

/// [`row_str`]'s boolean half.
fn row_bool(row: &Value, field: &str, what: &str) -> Result<bool, String> {
    row.get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("the secrets broker's status reply had no `{field}` for {what}"))
}

/// [`row_str`]'s list half — every element must be a string; anything else
/// is a malformed reply, not a list this function quietly shortens.
fn row_str_list(row: &Value, field: &str, what: &str) -> Result<Vec<String>, String> {
    let items = row
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("the secrets broker's status reply had no `{field}` array for {what}"))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("the secrets broker's status reply had a non-string entry in `{field}` for {what}"))
        })
        .collect()
}

/// Parse ONE `status` row. `what` names the row in every error, so a
/// malformed reply says WHICH secret it choked on.
fn parse_status_secret(row: &Value) -> Result<StatusSecret, String> {
    let name = row_str(row, "name", "a status row")?;
    let what = format!("secret `{name}`");
    let automation = match row.get("automation") {
        Some(a) => StatusAutomation {
            enabled: row_bool(a, "enabled", &format!("{what}'s automation"))?,
            consumers: row_str_list(a, "consumers", &format!("{what}'s automation"))?,
        },
        None => return Err(format!("the secrets broker's status reply had no `automation` for {what}")),
    };
    Ok(StatusSecret {
        name,
        backend: row_str(row, "backend", &what)?,
        require_totp: row_bool(row, "requireTotp", &what)?,
        consumers: row_str_list(row, "consumers", &what)?,
        automation,
        shared_with: row_str_list(row, "sharedWith", &what)?,
        remote: row_bool(row, "remote", &what)?,
        allow_remote_origin: row_bool(row, "allowRemoteOrigin", &what)?,
    })
}

/// Connect to `socket_path`, send ONE `status` request, read ONE reply line,
/// and return the broker's value-free policy inventory — the read side of
/// `secrets status` (the CLI-only operator surface `commands::
/// handle_secrets_status` wraps).
///
/// **The broker does the reading, never this process.** A client-side peek
/// at `policy.json` would break the uid boundary outright (the operator
/// running this CLI usually cannot read the broker's home at all —
/// `AGENTS.md`'s own "the client doesn't run as the secrets uid" note), so
/// there is NO direct-home fallback here: an unreachable broker is an
/// `Err`, never a locally-fabricated inventory. When the broker DOES answer,
/// `home` is the path the BROKER resolved and read (`AOIDE_SECRETS_HOME` is
/// each process's own — a client whose env points elsewhere must never
/// relabel the broker's rows as its own home's contents), echoed in the
/// reply for exactly that reason.
///
/// Both socket timeouts are finite ([`STATUS_TIMEOUT`]): a wedged broker is
/// an honest error within seconds, never a hang.
pub fn status(socket_path: &Path) -> Result<StatusReport, String> {
    let mut stream = connect_bounded(socket_path, CONNECT_TIMEOUT)
        .map_err(|e| describe_connect_error(socket_path, &e, "aoide secrets status"))?;
    stream
        .set_read_timeout(Some(STATUS_TIMEOUT))
        .map_err(|e| io_cause("set a read timeout on", socket_path, &e).why)?;
    stream.set_write_timeout(Some(STATUS_TIMEOUT)).map_err(|e| io_cause("set a write timeout on", socket_path, &e).why)?;

    let line = json!({ "op": "status" }).to_string() + "\n";
    stream
        .write_all(line.as_bytes())
        .map_err(|e| io_cause("write to", socket_path, &e).why)?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader.read_line(&mut reply_line).map_err(|e| match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => format!(
            "the secrets broker at {} did not answer `status` within {:?} — reporting that instead of hanging",
            socket_path.display(),
            STATUS_TIMEOUT
        ),
        _ => io_cause("read from", socket_path, &e).why,
    })?;
    if reply_line.trim().is_empty() {
        return Err("the secrets broker closed the connection with no reply".to_string());
    }
    let reply: Value = serde_json::from_str(reply_line.trim())
        .map_err(|e| format!("the secrets broker sent an unparseable reply: {e}"))?;

    // `{"ok":false}` is the broker's own refusal — an OLDER broker that does
    // not know this op at all ("unknown op `status`"), a `policy.json` that
    // would not load, a peer-cred refusal. All of them are reported as what
    // they are; none of them is ever read as "the inventory is empty".
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the secrets broker denied the request")
            .to_string());
    }
    let home = reply
        .get("home")
        .and_then(Value::as_str)
        .ok_or_else(|| "the secrets broker's status reply had no `home`".to_string())?
        .to_string();
    let rows = reply
        .get("secrets")
        .and_then(Value::as_array)
        .ok_or_else(|| "the secrets broker's status reply had no `secrets` array".to_string())?;
    let secrets = rows.iter().map(parse_status_secret).collect::<Result<Vec<_>, String>>()?;
    Ok(StatusReport { home, secrets })
}

/// Is stdin a terminal? `libc::isatty` on fd 0 — the branch point between
/// the historical pipe path and P-V4e's hidden-input prompt (module doc).
/// `pub(crate)` since `watch.rs` (this crate's line-mode broker-event
/// surface) needs the SAME tty branch point to decide narration-only vs.
/// prompting — widened per `pkgs/aoide/crates/AGENTS.md`'s "no cross-crate
/// copying" rule, applied in-crate: reach into the existing seam, never
/// fork a second `isatty` call site.
pub(crate) fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(0) != 0 }
}

/// Strip exactly ONE trailing `\n`, never a blanket `.trim_end()` — moved
/// to `aoide_protocol::dialog::strip_one_trailing_newline` at P-P5 (F5,
/// pure extraction: a SECOND consumer, `aoide-client`'s own popup arm,
/// needed the identical trim without duplicating it). `pub(crate)` shim:
/// `watch.rs`'s `--popup` dialog-output reader — now living inside
/// `aoide_protocol::dialog::run_entry_dialog` itself — trims through the
/// SAME function; this re-export keeps every existing call at this path
/// ([`read_hidden_line`] no longer needs it itself, P-I1, but this stays
/// the one seam any OTHER stdout-line trim in this crate reaches for)
/// byte-identical. `run_entry_dialog` itself moved too (F5), so its own
/// call now reaches the function directly, unqualified, inside
/// `aoide_protocol::dialog` — leaving no live call site AT this path
/// today; kept anyway as the shim the "no cross-crate copying" discipline
/// asks for, for the next stdout-line trim this crate reaches for.
#[allow(unused_imports)]
pub(crate) use aoide_protocol::dialog::strip_one_trailing_newline;

/// Read one line of hidden input on the real terminal — the tty half of
/// [`run_put`]'s prompt (module doc). Retrofit (ONBOARD.md's prompt
/// substrate section, P-I1) onto `aoide_protocol::pick::hidden_input`
/// (`inquire::Password`, hidden display mode, no confirmation — the
/// crate's own AGENTS.md invariant that `inquire` never enters this crate
/// directly holds: the dependency lives in `aoide-protocol` alone), which
/// replaced the hand-rolled `libc::termios` echo-disable this function used
/// to do itself. The name, `pub(crate)` visibility, and every call site are
/// UNCHANGED — this is the one seam `run_put` and `watch.rs`'s approve
/// prompt already reused VERBATIM, so retrofitting its body is the whole
/// fix; neither caller needed an edit.
pub(crate) fn read_hidden_line(prompt: &str) -> Result<String, String> {
    aoide_protocol::pick::hidden_input(prompt)
}

/// Prompt `y/N` on stderr and read ONE line from stdin, unhidden (a yes/no
/// answer isn't sensitive, unlike the value itself). `true` only for
/// `y`/`yes` (case-insensitive, surrounding whitespace trimmed); EOF
/// (`read_line` returning `Ok(0)`) and every other input default to `false`
/// — the task's own "default No" requirement.
fn confirm_overwrite(secret: &str) -> Result<bool, String> {
    eprint!("secret `{secret}` already has a stored value — overwrite? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line).map_err(|e| io_cause("read", Path::new("stdin"), &e).why)?;
    Ok(read > 0 && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

/// The full `secrets put <name>` client flow: read the value from THIS
/// process's own stdin (stdin-only intake, module doc) — prompting with
/// echo hidden when stdin is a terminal (P-V4e), reading straight through
/// unchanged when it's piped/redirected (the historical shape) — then
/// [`put`] it over `socket_path`. The value exists only as this function's
/// own local `String`, from the stdin read to the `put()` call(s) — never
/// returned, never logged, never touching argv.
///
/// **P-67:** `force` (the CLI's `--force` flag) rides as `overwrite` on the
/// FIRST attempt — a forced put always succeeds in one round trip, tty or
/// not. Only when `force` is false and the broker refuses with
/// [`PutError::Exists`] does this branch on [`stdin_is_tty`] a second time:
/// a tty gets [`confirm_overwrite`]'s `y/N` prompt and, on yes, a SECOND
/// `put` with the SAME value and `overwrite: true` — the caller is never
/// asked to retype it; a non-tty stdin gets [`PutFailure::NeedsForce`]. An
/// empty or whitespace-only value is [`PutFailure::Empty`], refused before the
/// broker is asked. Returns the human-facing success message ("stored" vs.
/// "replaced", so the CLI's own `Outcome` can say which happened) or a
/// value-free [`PutFailure`].
pub fn run_put(secret: &str, socket_path: &Path, force: bool) -> Result<String, PutFailure> {
    if let Some(refusal) = not_the_socket_owner(&crate::home::effective_user(), crate::home::owner_of(socket_path), socket_path) {
        return Err(PutFailure::Broker(refusal));
    }
    let typed = stdin_is_tty();
    let value = if typed {
        read_hidden_line(&format!("value for `{secret}` (input hidden): ")).map_err(PutFailure::Broker)?
    } else {
        use std::io::Read;
        let mut value = String::new();
        std::io::stdin()
            .read_to_string(&mut value)
            .map_err(|e| PutFailure::Broker(io_cause("read", Path::new("stdin"), &e).why))?;
        value
    };
    if value.trim().is_empty() {
        return Err(PutFailure::Empty { typed });
    }

    match put(socket_path, secret, &value, force) {
        Ok(true) => Ok(format!("replaced secret `{secret}`'s stored value")),
        Ok(false) => Ok(format!("stored secret `{secret}`")),
        Err(PutError::Other(e)) => Err(PutFailure::Broker(e)),
        Err(PutError::Exists) => {
            if !stdin_is_tty() {
                return Err(PutFailure::NeedsForce);
            }
            if !confirm_overwrite(secret).map_err(PutFailure::Broker)? {
                return Err(PutFailure::Declined);
            }
            match put(socket_path, secret, &value, true) {
                Ok(true) => Ok(format!("replaced secret `{secret}`'s stored value")),
                Ok(false) => Ok(format!("stored secret `{secret}`")),
                Err(PutError::Other(e)) => Err(PutFailure::Broker(e)),
                Err(PutError::Exists) => Err(PutFailure::Broker(format!(
                    "secret `{secret}`: the broker refused the confirmed overwrite unexpectedly"
                ))),
            }
        }
    }
}

/// [`run_put`]'s early refusal, before a value is asked for: the broker accepts
/// `put` only from its own user, and that user owns its socket, so a caller
/// that is not the socket's owner is refused here instead of after typing a
/// secret that would be thrown away. The decision is the admin gate's own
/// ([`crate::home::admin_identity_error`]); the broker stays the authority, so
/// an absent or unreadable socket (`owner` is `None`) passes and the connect
/// reports it.
fn not_the_socket_owner(me: &Option<crate::peercred::PeerUser>, owner: Option<crate::peercred::PeerUser>, socket_path: &Path) -> Option<String> {
    let (me, owner) = (me.as_ref()?, owner?);
    crate::home::admin_identity_error(me, &owner, socket_path, "put")
}

/// Why [`run_put`] stored nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutFailure {
    /// The value was empty or only whitespace; `typed` is whether a person
    /// was at the prompt (as against a pipe).
    Empty { typed: bool },
    /// The secret has a value, stdin is not a terminal, and `--force` was not given.
    NeedsForce,
    /// The person answered no to the overwrite prompt.
    Declined,
    /// The broker or the terminal said no, in its own words.
    Broker(String),
}

/// Spawn `cmd`, `var`=`value` injected, `Stdio::inherit()` throughout
/// (never captured — aoide never holds the child's bytes, so there is
/// nothing here that could redact wrong), and return the CHILD's own exit
/// code — never aoide's own exit-code vocabulary, a wrapped command's exit
/// code is its own signal.
fn spawn_with_secret(cmd: &[String], var: &str, value: &str) -> Result<i32, String> {
    let status = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .env(var, value)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| io_cause("run", Path::new(&cmd[0]), &e).why)?;
    Ok(status.code().unwrap_or(1))
}

/// The full `secrets exec` client flow — parse, resolve over `socket_path`,
/// spawn with the value injected. `Ok` is the process exit code to hand
/// back from `main`: the CHILD's own exit code, never aoide's own
/// vocabulary. `Err` is a taught refusal for a bad invocation, a denied
/// resolve or a spawn failure; the caller renders it through the envelope.
pub fn run_exec(inv: &Invocation, socket_path: &Path) -> Result<i32, Refusal> {
    let args = parse_exec_args(inv)?;
    let at = crate::teach::Where { inv, socket: socket_path, secret: Some(&args.secret) };
    let value = resolve_for_exec(socket_path, &args, &mut RawStdin, stdin_is_tty(), &mut io::stderr())
        .map_err(|e| crate::teach::from_broker(&at, &e))?;
    spawn_with_secret(&args.cmd, &args.var, &value).map_err(|e| {
        Refusal::new(
            Kind::Failed,
            format!("could not start `{}` with the secret injected", args.cmd[0]),
            e,
            Fix::Set("check that the command exists and is executable on PATH".to_string()),
        )
    })
}

/// Standard input read one byte per `read(2)`, with no buffer in between: a
/// byte this process does not take stays unread for the child that inherits
/// the descriptor (`secrets exec`'s parked pipe path).
pub(crate) struct RawStdin;

#[cfg(unix)]
impl io::Read for RawStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
            if n >= 0 {
                return Ok(n as usize);
            }
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                return Err(err);
            }
        }
    }
}

/// No unbuffered descriptor read is wired on Windows: std's stdin reads
/// ahead, so a piped child can miss bytes past its park line. A known gap.
#[cfg(windows)]
impl io::Read for RawStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut io::stdin(), buf)
    }
}

/// The most bytes a code line may hold, newline excluded. A TOTP code is six
/// digits; the cap only stops a writer that never sends a newline from growing
/// the buffer without bound.
const MAX_CODE_LINE: usize = 256;

/// One line, read a byte at a time, so nothing past its newline is taken from
/// `input`. The newline is not part of the line; end of input gives what
/// arrived before it. A line past [`MAX_CODE_LINE`] bytes is refused at the
/// byte that passes the cap, so nothing beyond it is read.
pub(crate) fn read_line_unbuffered(input: &mut impl io::Read) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match input.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) if line.len() == MAX_CODE_LINE => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("the line is longer than {MAX_CODE_LINE} bytes, so it is not a code"),
                ));
            }
            Ok(_) => line.push(byte[0]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(String::from_utf8_lossy(&line).trim_end_matches('\r').to_string())
}

/// The code a parked ask waits for: a no-echo prompt on a terminal, else one
/// line of `input`. `Ok(None)` when nothing arrived (end of input or a blank
/// line), so the caller decides what an absent code means to it.
pub(crate) fn read_park_code(input: &mut impl io::Read, tty: bool, prompt: &str) -> Result<Option<String>, String> {
    let typed = if tty {
        read_hidden_line(prompt)?
    } else {
        read_line_unbuffered(input).map_err(|e| io_cause("read the code from", Path::new("stdin"), &e).why)?
    };
    let code = typed.trim().to_string();
    Ok((!code.is_empty()).then_some(code))
}

/// The park line a person reads: where the code is completed, and when the
/// ask times out. `expires_at` is the broker's instant for that; a broker
/// older than the field does not send it, and the line then gives the span.
pub(crate) fn park_hint(id: &str, timeout_secs: u64, expires_at: Option<u64>) -> String {
    let until = match expires_at {
        Some(at) => format!("times out in {timeout_secs}s (expires at unix {at})"),
        None => format!("times out in {timeout_secs}s"),
    };
    format!("parked as ask {id} — complete with: aoide secrets approve {id}  (or dismiss {id}); {until}")
}

/// What `approve` shows before it reads a code. The secret, consumer, reason
/// argv0 and origin are the requester's claims. The peer uid is the kernel's word,
/// and so is the origin's pid, as read at park time (`park::AskOrigin`'s doc).
/// `comm` is process-controlled text, shown as data and never trusted.
pub(crate) fn approve_context(ask: &PendingAsk) -> String {
    let peer = match (ask.peer_uid, &ask.peer_sid) {
        (Some(uid), _) => format!("uid {uid}"),
        (None, Some(sid)) => format!("sid {sid}"),
        (None, None) => "unidentified".to_string(),
    };
    let origin = match crate::watch::format_origin_line(&ask.origin) {
        Some(line) => format!(
            "  the requester claims (unverified): {line}\n  the pid is the kernel's word at park time; comm is process-controlled text, shown as data\n"
        ),
        None => String::new(),
    };
    format!(
        "parked ask {}\n  the requester claims (unverified): secret {}, consumer {}, reason {}\n  the requester claims (unverified): the program that receives the value is {}\n{origin}  the kernel stamped the peer: {peer}\n",
        ask.id,
        ask.secret,
        ask.consumer,
        ask.reason.as_deref().unwrap_or("none given"),
        ask.argv0.as_deref().unwrap_or("none given"),
    )
}

/// The refusal for a `--totp` in argv: any process of this uid can read argv,
/// and a code is not bound to a park, so a reader could spend it first.
pub(crate) fn totp_in_argv(inv: &Invocation, fix: Fix) -> Option<Refusal> {
    inv.flags.contains_key("totp").then(|| {
        Refusal::new(
            Kind::Usage,
            "`--totp` puts the TOTP code in argv",
            "any process of this uid can read argv, and a code is not bound to a park, so a reader can spend it on its own request first",
            fix,
        )
    })
}

/// `secrets approve <id>`: shows the parked ask, then reads one code (a no-echo
/// prompt on a terminal, else one stdin line) and sends it. The code is never
/// an argument. Returns the id it completed.
pub(crate) fn approve_with(
    inv: &Invocation,
    socket_path: &Path,
    input: &mut impl io::Read,
    out: &mut impl io::Write,
    tty: bool,
) -> Result<String, Refusal> {
    let id = inv.args[0].clone();
    let at = crate::teach::Where { inv, socket: socket_path, secret: None };
    let fix = Fix::Run(format!("aoide secrets approve {id}, then type the code (a program pipes one line)"));
    if let Some(refusal) = totp_in_argv(inv, fix) {
        return Err(refusal);
    }
    let asks = pending(socket_path).map_err(|e| crate::teach::from_broker(&at, &e))?;
    let Some(ask) = asks.iter().find(|a| a.id == id) else {
        return Err(Refusal::new(
            Kind::Refused,
            format!("no parked ask `{id}`"),
            "only a parked ask takes a code, so nothing was asked for one",
            Fix::Run("aoide secrets pending".to_string()),
        ));
    };
    let _ = write!(out, "{}", approve_context(ask));
    let code = read_park_code(input, tty, &format!("TOTP code for ask {id}: "))
        .map_err(|e| Refusal::new(Kind::Failed, "could not read the code", e, Fix::Run(format!("aoide secrets approve {id}"))))?
        .ok_or_else(|| {
            Refusal::new(
                Kind::Usage,
                "no code arrived",
                "nothing was typed or piped, so no code was sent",
                Fix::Run(format!("aoide secrets approve {id}, typing the code at a terminal or piping one line")),
            )
        })?;
    approve(socket_path, &id, &code).map_err(|e| crate::teach::from_broker(&at, &e))?;
    Ok(id)
}

/// `secrets exec` for a parked ask. The park line and the ask's context (what
/// the code releases, as `approve` shows it) are shown, then one code is read
/// (a no-echo prompt on a terminal, else one stdin line) and approved on a
/// SECOND connection; the value still returns on the first. A wrong code is
/// reported and the wait goes on. End of input prints the approve line and
/// waits until the park times out.
pub(crate) fn resolve_for_exec(
    socket_path: &Path,
    args: &ExecArgs,
    input: &mut impl io::Read,
    tty: bool,
    out: &mut impl io::Write,
) -> Result<String, String> {
    let mut on_interim = |interim: &Value| {
        if interim.get("parked").and_then(Value::as_bool) != Some(true) {
            return;
        }
        let id = interim.get("id").and_then(Value::as_str).unwrap_or("?");
        let timeout_secs = interim.get("timeoutSecs").and_then(Value::as_u64).unwrap_or(0);
        let expires_at = interim.get("expiresAt").and_then(Value::as_u64);
        let _ = writeln!(out, "{}", park_hint(id, timeout_secs, expires_at));
        // F4: what the code releases is shown from the park's own row, the same
        // context `approve` shows, before any code is read. A row that cannot be
        // read asks for no code.
        let Some(ask) = pending(socket_path).ok().and_then(|asks| asks.into_iter().find(|a| a.id == id)) else {
            let _ = writeln!(
                out,
                "no code asked for: the details of ask {id} could not be read; still waiting, complete with: aoide secrets approve {id}"
            );
            return;
        };
        let _ = write!(out, "{}", approve_context(&ask));
        match read_park_code(input, tty, &format!("TOTP code for ask {id}: ")) {
            Ok(Some(code)) => {
                if let Err(e) = approve(socket_path, id, &code) {
                    let _ = writeln!(out, "{e}");
                }
            }
            Ok(None) => {
                let _ = writeln!(out, "no code given: still waiting; complete with: aoide secrets approve {id}");
            }
            Err(e) => {
                let _ = writeln!(out, "{e}");
            }
        }
    };
    resolve_with(
        socket_path,
        &args.secret,
        &args.consumer,
        None,
        args.cmd.first().map(String::as_str),
        args.reason.as_deref(),
        &mut on_interim,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── the POSIX-shell fixture class (gated, with the reason) ───────────
    //
    // Every test below carrying `#[cfg(unix)]` drives the backend TEMPLATE
    // mechanism with a POSIX fixture: an `sh -c` command line, or a
    // `#!/bin/sh` shim script on `PATH`. The mechanism itself is portable and
    // HAS a native arm — `sh -c` on Unix, `cmd /C` on native Windows
    // (`backend::run_backend_command`'s own doc) — so these gates name the
    // FIXTURE, never the code under test. The Windows arm of the same path is
    // exercised natively by `backend::tests::a_native_windows_template_*`.
    use aoide_protocol::Door;
    use std::collections::BTreeMap;

    fn inv(flags: &[(&str, &str)], args: &[&str]) -> Invocation {
        let mut flag_map = BTreeMap::new();
        for (k, v) in flags {
            flag_map.insert(k.to_string(), v.to_string());
        }
        Invocation {
            path: vec!["secrets".to_string(), "exec".to_string()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flag_map,
            door: Door::Cli,
        }
    }

    // ── derive_reason / parse_exec_args's --reason (P3) ──────────────────

    #[test]
    fn derive_reason_joins_a_short_command_verbatim() {
        let cmd = vec!["psql".to_string(), "-U".to_string(), "app".to_string()];
        assert_eq!(derive_reason(&cmd), Some("psql -U app".to_string()));
    }

    #[test]
    fn derive_reason_truncates_a_long_command_to_sixty_chars_with_an_ellipsis() {
        let cmd = vec!["sh".to_string(), "-c".to_string(), "a".repeat(100)];
        let reason = derive_reason(&cmd).unwrap();
        assert_eq!(reason.chars().count(), 60);
        assert!(reason.ends_with('\u{2026}'), "{reason:?}");
        assert!(reason.starts_with("sh -c "), "{reason:?}");
    }

    #[test]
    fn derive_reason_is_none_for_an_empty_command() {
        assert_eq!(derive_reason(&[]), None);
    }

    #[test]
    fn parse_exec_args_derives_reason_from_the_command_when_omitted() {
        let i = inv(&[("as", "m"), ("secret", "db-prod")], &["psql", "-U", "app"]);
        let args = parse_exec_args(&i).unwrap();
        assert_eq!(args.reason.as_deref(), Some("psql -U app"));
    }

    #[test]
    fn parse_exec_args_an_explicit_reason_wins_over_the_derived_one() {
        let i = inv(&[("as", "m"), ("secret", "db-prod"), ("reason", "nightly backup")], &["psql", "-U", "app"]);
        let args = parse_exec_args(&i).unwrap();
        assert_eq!(args.reason.as_deref(), Some("nightly backup"));
    }

    // ── bounded connect (rider task, alongside #75/#81/#82) ─────────────

    fn tmp_socket_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aoide-secrets-client-connect-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A path too long for `sun_path` is a clean, immediate `Err` — never a
    /// panic, and never reached via a raw slice-index that could.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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

        let _listener = crate::test_net::UnixListener::bind(&sock).unwrap();
        let stream = connect_bounded(&sock, Duration::from_secs(5)).unwrap();
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
        let _listener = crate::test_net::UnixListener::bind(&sock).unwrap();

        let start = std::time::Instant::now();
        let stream = connect_bounded(&sock, Duration::from_secs(5)).unwrap();
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
        let err = connect_bounded(&dead, Duration::from_secs(5)).unwrap_err();
        assert!(start.elapsed() < Duration::from_millis(500), "a dead path must fail near-instantly, not wait out the bound");
        assert_ne!(err.kind(), io::ErrorKind::TimedOut, "ENOENT is not a timeout");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The WINDOWS spelling of "nothing is listening", pinned against a real
    /// connect rather than asserted in prose: an `AF_UNIX` connect to a path
    /// whose own DIRECTORY is absent reports `WSAENETDOWN` (10050), a code Rust
    /// has no `ErrorKind` for. If that mapping ever stops holding, an absent
    /// broker on native Windows becomes a hard failure instead of the
    /// direct-write fallback — which is exactly what
    /// [`nothing_is_listening`] exists to prevent.
    #[cfg(windows)]
    #[test]
    fn a_connect_under_a_missing_parent_directory_reads_as_nothing_listening() {
        let dir = std::env::temp_dir().join(format!("aoide-no-such-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("broker.sock");

        let err = connect_bounded(&path, Duration::from_secs(5)).expect_err("nothing is listening there");
        assert_eq!(
            err.raw_os_error(),
            Some(10050),
            "the code this host reports for a missing parent directory: {err:?}"
        );
        assert!(nothing_is_listening(&err), "and it must read as nothing-listening: {err}");
    }

    /// Hand-rolled listener + a single unaccepted connection, exactly the
    /// reviewer's own reproduction recipe (review-bounce fix, this
    /// commit): `crate::test_net::UnixListener::bind` hardcodes a
    /// backlog of 128, far too large to saturate cheaply in a test, so
    /// this builds the listener directly with `libc::listen(fd, 1)` —
    /// the same raw-socket construction `connect_bounded`/`unix_sockaddr`
    /// already use in production code, reused here for the test's own
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
            set_fd_nonblocking(fd, true);
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

    /// **The headline review-bounce proof:** `connect_bounded` must
    /// actually RETRY through a saturated backlog and succeed once a slot
    /// frees, not fail immediately on the first `EAGAIN` the way the
    /// bounced version of this function did. A background thread frees
    /// the one occupied slot (by accepting the filler connection) ~80ms
    /// in; `connect_bounded`'s own retry interval is 15ms, so it must
    /// notice well within its 3s budget — and the elapsed time must be
    /// LONG ENOUGH to prove it actually waited (not a lucky race past a
    /// backlog that was never really full).
    #[test]
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
        let stream = connect_bounded(&sock, Duration::from_secs(3)).unwrap();
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

    /// The deadline half of the same proof: when the backlog stays
    /// saturated for good (nothing ever accepts), `connect_bounded`'s
    /// `EAGAIN` retry loop must still respect the overall bound rather
    /// than retrying forever.
    #[test]
    // `cfg(unix)`: this reads the UNIX arm'"'"'s own body
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
        let err = connect_bounded(&sock, Duration::from_millis(200)).unwrap_err();
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

    #[test]
    fn default_var_name_uppercases_and_underscores_hyphens() {
        assert_eq!(default_var_name("db-prod"), "DB_PROD");
        assert_eq!(default_var_name("token"), "TOKEN");
        assert_eq!(default_var_name("a-b-c"), "A_B_C");
    }

    #[test]
    fn parses_a_full_invocation() {
        let i = inv(&[("as", "m"), ("secret", "db-prod")], &["psql", "-c", "select 1"]);
        let args = parse_exec_args(&i).unwrap();
        assert_eq!(args.consumer, "m");
        assert_eq!(args.secret, "db-prod");
        assert_eq!(args.var, "DB_PROD");
        assert_eq!(args.cmd, vec!["psql", "-c", "select 1"]);
    }

    #[test]
    fn explicit_var_name_wins_over_the_derived_default() {
        let i = inv(&[("as", "m"), ("secret", "db-prod:PGPASSWORD")], &["psql"]);
        let args = parse_exec_args(&i).unwrap();
        assert_eq!(args.secret, "db-prod");
        assert_eq!(args.var, "PGPASSWORD");
    }

    #[test]
    fn totp_flag_is_refused_and_its_code_is_not_carried() {
        let i = inv(&[("as", "m"), ("secret", "t"), ("totp", "123456")], &["cmd"]);
        let err = parse_exec_args(&i).unwrap_err();
        assert!(err.what.contains("--totp"), "{err:?}");
        assert!(!format!("{err:?}").contains("123456"), "{err:?}");
    }

    #[test]
    fn invalid_secret_name_is_rejected() {
        let i = inv(&[("as", "m"), ("secret", "Bad Name")], &["cmd"]);
        assert!(parse_exec_args(&i).unwrap_err().what.contains("not a valid secret name"));
    }

    // ── describe_connect_error (pure — injected io::Error, no real socket) ──

    #[test]
    fn permission_denied_teaches_both_the_sg_and_relogin_fixes() {
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let msg = describe_connect_error(socket, &err, "aoide secrets status");
        assert!(msg.contains(&socket.display().to_string()), "{msg}");
        assert!(msg.contains("aoide-secrets-access"), "{msg}");
        assert!(msg.contains("sg aoide-secrets-access -c 'aoide secrets status'"), "{msg}");
        assert!(msg.to_lowercase().contains("log out"), "{msg}");
    }

    #[test]
    fn a_put_by_anyone_but_the_socket_owner_is_refused_before_a_value_is_asked_for() {
        use crate::peercred::PeerUser::Uid;
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let refusal = not_the_socket_owner(&Some(Uid(1000)), Some(Uid(990)), socket).expect("a foreign owner is refused");
        assert!(refusal.contains("secrets put must run as the broker user"), "{refusal}");
        assert!(not_the_socket_owner(&Some(Uid(1000)), Some(Uid(1000)), socket).is_none());
        assert!(not_the_socket_owner(&Some(Uid(1000)), None, socket).is_none(), "no socket to stat: the connect says so");
        assert!(not_the_socket_owner(&None, Some(Uid(990)), socket).is_none(), "the broker gate stays the authority");
    }

    #[test]
    fn permission_denied_on_an_admin_op_teaches_the_sudo_line_not_sg() {
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let msg = describe_admin_connect_error(socket, &err, "aoide secrets put db-prod");
        assert!(msg.contains(NO_ACCESS), "{msg}");
        assert!(msg.contains("sudo -u aoide-secrets aoide secrets put db-prod"), "{msg}");
        assert!(!msg.contains("sg aoide-secrets-access"), "{msg}");
    }

    #[test]
    fn not_found_teaches_checking_the_broker_service() {
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let err = io::Error::new(io::ErrorKind::NotFound, "no such file or directory");
        let msg = describe_connect_error(socket, &err, "aoide secrets exec --as m --secret t -- true");
        assert!(msg.contains(&socket.display().to_string()), "{msg}");
        assert!(msg.contains("systemctl status aoide-secrets-serve"), "{msg}");
        assert!(msg.contains("AOIDE_SECRETS_SOCKET"), "{msg}");
    }

    #[test]
    fn connection_refused_gets_the_same_not_running_hint_as_not_found() {
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        let msg = describe_connect_error(socket, &err, "aoide secrets put t");
        assert!(msg.contains("systemctl status aoide-secrets-serve"), "{msg}");
    }

    #[test]
    fn an_unrelated_error_kind_rides_through_unenriched() {
        let socket = Path::new("/run/aoide-secrets/secrets.sock");
        let err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let msg = describe_connect_error(socket, &err, "aoide secrets put t");
        assert!(msg.contains(&socket.display().to_string()), "{msg}");
        assert!(msg.contains("timed out"), "{msg}");
        assert!(!msg.contains("aoide-secrets-access"), "{msg}");
        assert!(!msg.contains("systemctl"), "{msg}");
    }

    #[test]
    fn resolve_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-test.sock");
        let err = resolve(dead, "t", "m", None, None, None).unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    #[test]
    fn put_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-put-test.sock");
        let err = put(dead, "t", "irrelevant", false).unwrap_err();
        assert!(matches!(err, PutError::Other(_)), "{err}");
        assert!(err.to_string().contains("connecting"), "{err}");
    }

    // ── P-V4e: `secrets put`'s tty prompt ───────────────────────────────
    //
    // `read_hidden_line`'s own tty path (`aoide_protocol::pick::
    // hidden_input`) is not exercised here — `cargo test`'s own stdin is
    // never a tty. `strip_one_trailing_newline`'s own pure-trim test moved
    // to `aoide_protocol::dialog`'s test module with the function itself
    // (P-P5, F5); what stays here is `stdin_is_tty` reading false (so
    // `run_put` takes the untouched pipe path) under this process's own
    // non-tty stdin, same as every existing `run_put`-adjacent test already
    // implicitly relies on.

    // ── P-67: warn-before-overwrite ─────────────────────────────────────

    /// End-to-end proof that [`put`] surfaces the wire's `exists` flag as
    /// [`PutError::Exists`] (never inferred by matching `error` prose) and
    /// that `overwrite: true` reports `replaced: true` back — a REAL
    /// broker + socket round trip, same shape as `commands.rs`'s own
    /// `require_totp_on_add_births_a_gated_policy_denied_without_a_code`.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn put_reports_exists_then_replaced_true_through_a_real_broker() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!(
            "aoide-secrets-client-put-overwrite-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        crate::store::save_policies(&home, &[crate::policy::Policy::new("t", "file", "k")]).unwrap();

        // A short /tmp-direct socket path — sockaddr_un's ~108-byte
        // sun_path can overflow under a nested tempdir (same SUN_LEN
        // caution `tests/e2e.rs`/`commands.rs`'s own TOTP e2e test document).
        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/aoide-secrets-client-put-overwrite-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));

        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = crate::broker::serve(&home_for_thread, &sock_for_thread);
        });

        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());

        // First put: no stored value yet -> `replaced: false`.
        assert_eq!(put(&socket_path, "t", "first-value", false), Ok(false));

        // Second put, no overwrite: the DISTINCT exists refusal.
        assert_eq!(put(&socket_path, "t", "attempted-overwrite", false), Err(PutError::Exists));

        // Third put, overwrite: true -> `replaced: true`.
        assert_eq!(put(&socket_path, "t", "second-value", true), Ok(true));

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    // ── P-N2: pending/approve/dismiss ───────────────────────────────────

    #[test]
    fn pending_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-pending-test.sock");
        let err = pending(dead).unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    #[test]
    fn approve_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-approve-test.sock");
        let err = approve(dead, "1", "123456").unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    #[test]
    fn dismiss_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-dismiss-test.sock");
        let err = dismiss(dead, "1").unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    /// A real broker + socket round trip through the client wrappers
    /// themselves: `resolve` parks (no code given), `pending` sees the ask
    /// with no value anywhere in it, `approve` releases the value down the
    /// ORIGINAL `resolve` call — never into `approve`'s own `Ok(())` — and
    /// once approved the ask is gone from `pending` again. Same real-broker
    /// shape as `put_reports_exists_then_replaced_true_through_a_real_broker`
    /// (a single `serve` call for the whole test, dropped-not-joined, same
    /// pattern that test already establishes).
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn pending_approve_round_trips_through_a_real_broker_and_releases_to_the_original_caller() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!(
            "aoide-secrets-client-park-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        let mut p = crate::policy::Policy::new("t", "file", "k");
        p.require_totp = true;
        crate::store::save_policies(&home, &[p]).unwrap();
        let totp_secret = b"a-twenty-byte-totp-s".to_vec();
        crate::store::save_totp_secret(&home, &totp_secret).unwrap();

        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/aoide-secrets-client-park-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));

        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = crate::broker::serve(&home_for_thread, &sock_for_thread);
        });
        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());

        // `put` a value first (no TOTP gate on `put`, module doc), so the
        // parked `resolve` below has something real to release.
        assert_eq!(put(&socket_path, "t", "the-real-value", false), Ok(false));
        assert_eq!(pending(&socket_path).unwrap(), Vec::new());

        let sock_for_resolve = socket_path.clone();
        let resolve_thread = std::thread::spawn(move || resolve(&sock_for_resolve, "t", "m", None, None, None));

        let mut ask = None;
        for _ in 0..200 {
            let list = pending(&socket_path).unwrap();
            if let Some(a) = list.into_iter().next() {
                ask = Some(a);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ask = ask.expect("the resolve did not park in time");
        assert_eq!(ask.secret, "t");
        assert_eq!(ask.consumer, "m");
        // #73: a REAL socket connection's SO_PEERCRED is this same test
        // process's own euid (the resolving thread and this thread are one
        // process) — proves the peer uid survives the full accept ->
        // park -> pending round trip, not just the in-process unit tests.
        match crate::home::effective_user().expect("this process has an identity") {
            crate::peercred::PeerUser::Uid(uid) => assert_eq!(ask.peer_uid, Some(uid)),
            crate::peercred::PeerUser::Sid(sid) => assert_eq!(ask.peer_sid, Some(sid)),
        }

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let step = crate::totp::timestep(now);
        let code = crate::totp::format6(crate::totp::hotp(&totp_secret, step, crate::totp::DIGITS));
        approve(&socket_path, &ask.id, &code).unwrap();

        assert_eq!(resolve_thread.join().unwrap(), Ok("the-real-value".to_string()));
        assert_eq!(pending(&socket_path).unwrap(), Vec::new());

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    // ── P-N2c FIX 1: interim-line framing (`read_final_reply`) ──────────

    #[test]
    fn read_final_reply_skips_interim_lines_and_returns_the_first_real_one() {
        let wire = "{\"interim\":true,\"parked\":true,\"id\":\"ab12-1\",\"timeoutSecs\":300}\n\
                    {\"ok\":true,\"value\":\"the-value\"}\n";
        let mut reader = std::io::Cursor::new(wire.as_bytes());
        let reply = read_final_reply(&mut reader).unwrap();
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["value"], "the-value");
    }

    #[test]
    fn read_final_reply_with_no_interim_line_behaves_exactly_as_before() {
        // Old-broker compat: a reply with zero interim lines (every op
        // besides a parking `resolve`) must round-trip byte-identically.
        let wire = "{\"ok\":false,\"error\":\"secret not found\"}\n";
        let mut reader = std::io::Cursor::new(wire.as_bytes());
        let reply = read_final_reply(&mut reader).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "secret not found");
    }

    #[test]
    fn read_final_reply_skips_multiple_interim_lines() {
        let wire = "{\"interim\":true,\"parked\":true,\"id\":\"1\",\"timeoutSecs\":1}\n\
                    {\"interim\":true,\"parked\":true,\"id\":\"1\",\"timeoutSecs\":1}\n\
                    {\"ok\":false,\"error\":\"timed out\"}\n";
        let mut reader = std::io::Cursor::new(wire.as_bytes());
        let reply = read_final_reply(&mut reader).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "timed out");
    }

    #[test]
    fn read_final_reply_on_a_connection_that_closes_after_only_interim_lines_is_a_clear_error() {
        let wire = "{\"interim\":true,\"parked\":true,\"id\":\"1\",\"timeoutSecs\":1}\n";
        let mut reader = std::io::Cursor::new(wire.as_bytes());
        let err = read_final_reply(&mut reader).unwrap_err();
        assert!(err.contains("closed the connection"), "{err}");
    }

    #[test]
    fn resolve_against_a_dead_socket_never_reaches_the_interim_loop_at_all() {
        // Sanity: a dead-socket connect error still short-circuits before
        // `read_final_reply` is ever called — same shape as the existing
        // `resolve_against_a_dead_socket_is_a_connect_error` test, kept
        // here as a reminder that FIX 1's new loop sits strictly AFTER the
        // connect step, never wrapping it.
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-interim-test.sock");
        let err = resolve(dead, "t", "m", None, None, None).unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    #[test]
    fn stdin_is_tty_is_false_under_cargo_test() {
        // `cargo test` never runs with a tty on fd 0 — this is the same
        // guarantee `run_put`'s existing pipe-path callers already lean on
        // implicitly; asserted directly so a sandboxing change that somehow
        // attaches a tty would fail loudly here instead of silently
        // changing `run_put`'s behavior under every other test.
        assert!(!stdin_is_tty());
    }

    // ── resolve_bounded (task #84: bounded, wait:false, no cache) ───────────

    #[test]
    fn resolve_bounded_against_a_dead_socket_is_a_connect_error() {
        let dead = Path::new("/tmp/aoide-secrets-nonexistent-bounded-test.sock");
        let err = resolve_bounded(dead, "t", "a2a-door", Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("connecting"), "{err}");
    }

    /// A real broker + socket round trip proving [`resolve_bounded`] returns
    /// the value on a granted, TOTP-free resolve — the deployed automation-
    /// open happy path task #84's PINNED CONSTRAINTS describe.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn resolve_bounded_returns_the_value_on_a_granted_resolve() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!(
            "aoide-secrets-client-bounded-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        crate::store::save_policies(&home, &[crate::policy::Policy::new("t", "file", "k")]).unwrap();

        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/aoide-secrets-client-bounded-ok-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = crate::broker::serve(&home_for_thread, &sock_for_thread);
        });
        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());

        assert_eq!(put(&socket_path, "t", "bounded-value", false), Ok(false));
        assert_eq!(
            resolve_bounded(&socket_path, "t", "a2a-door", Duration::from_secs(2)),
            Ok("bounded-value".to_string())
        );

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// **The PARKING HAZARD test (task #84).** A `requireTotp` secret with
    /// NO code and NO automation-open exemption would, under plain
    /// [`resolve`], PARK — holding the connection open for the full
    /// `AOIDE_SECRETS_PARK_TIMEOUT` (300s default). [`resolve_bounded`]
    /// must never do that: `wait:false` on the wire makes the broker deny
    /// immediately instead of parking at all, so this returns well inside
    /// the bound (asserted against a generous few-second wall-clock budget,
    /// never the full park timeout) with a denial, not a hang.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn resolve_bounded_never_parks_on_a_requiretotp_secret_with_no_code() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!(
            "aoide-secrets-client-bounded-parking-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        let mut p = crate::policy::Policy::new("t", "file", "k");
        p.require_totp = true;
        crate::store::save_policies(&home, &[p]).unwrap();
        let totp_secret = b"a-twenty-byte-totp-s".to_vec();
        crate::store::save_totp_secret(&home, &totp_secret).unwrap();

        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/aoide-secrets-client-bounded-parking-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = crate::broker::serve(&home_for_thread, &sock_for_thread);
        });
        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());

        assert_eq!(put(&socket_path, "t", "never-released", false), Ok(false));

        let started = std::time::Instant::now();
        let err = resolve_bounded(&socket_path, "t", "a2a-door", Duration::from_secs(2)).unwrap_err();
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(10),
            "resolve_bounded must never approach the 300s park timeout — took {elapsed:?}"
        );
        assert!(err.contains("totp") || err.contains("TOTP"), "{err}");

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    // ── `secrets status` (the value-free inventory op) ───────────────────

    /// Bind a one-shot listener at a scratch socket path that answers
    /// exactly ONE connection with `reply` — the same hand-rolled-fake-broker
    /// idiom this module's connect tests already use, extended to a reply.
    /// `None` for a broker that accepts and then never answers at all (the
    /// read-timeout fixture). The thread is detached: the test process
    /// exiting tears it down.
    fn one_shot_broker(tag: &str, reply: Option<&str>) -> std::path::PathBuf {
        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/as-{tag}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos()
        ));
        let listener = crate::test_net::UnixListener::bind(&socket_path).unwrap();
        let reply = reply.map(str::to_string);
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                // Consume the request line first — the client is BLOCKED
                // writing/reading, so an accept-then-close would look like
                // a different failure than the one under test.
                let mut request = String::new();
                let _ = std::io::BufReader::new(&stream).read_line(&mut request);
                match reply {
                    Some(body) => {
                        let mut w = &stream;
                        let _ = w.write_all(format!("{body}\n").as_bytes());
                    }
                    // The silent case: hold the connection open, answer
                    // nothing. Long enough to outlast any bounded read.
                    None => std::thread::sleep(Duration::from_secs(60)),
                }
            }
        });
        socket_path
    }

    /// The happy path: the typed parse keeps exactly the eight documented
    /// value-free fields — and DROPS everything else a reply might carry.
    /// The fixture row deliberately also carries a backend `key` and a
    /// `value`, the two things this op must never forward: a
    /// `serde_json::Value` pass-through would leak both straight into a
    /// caller's `--json`, while this parse has nowhere to put them. `home`
    /// is the BROKER's, taken verbatim from the reply (never re-derived from
    /// this process's own env).
    #[test]
    fn status_keeps_only_the_documented_value_free_fields_and_drops_the_rest() {
        let sock = one_shot_broker(
            "status-ok",
            Some(
                r#"{"ok":true,"home":"/var/lib/aoide-secrets","secrets":[{"name":"db-prod","backend":"age","key":"some/backend/key","value":"hunter2","requireTotp":true,"consumers":["m"],"automation":{"enabled":true,"consumers":["cron"]},"sharedWith":["osaka"],"remote":false,"allowRemoteOrigin":true}]}"#,
            ),
        );
        let report = status(&sock).unwrap();
        assert_eq!(report.home, "/var/lib/aoide-secrets");
        assert_eq!(
            report.secrets,
            vec![StatusSecret {
                name: "db-prod".to_string(),
                backend: "age".to_string(),
                require_totp: true,
                consumers: vec!["m".to_string()],
                automation: StatusAutomation { enabled: true, consumers: vec!["cron".to_string()] },
                shared_with: vec!["osaka".to_string()],
                remote: false,
                allow_remote_origin: true,
            }]
        );
        std::fs::remove_file(&sock).ok();
    }

    /// An older broker that does not know this op answers `{"ok":false,
    /// "error":"unknown op `status`"}` — an explicit `Err`, never an empty
    /// inventory.
    #[test]
    fn status_reports_an_old_broker_that_does_not_know_the_op_as_an_err() {
        let sock = one_shot_broker("status-unknown-op", Some(r#"{"ok":false,"error":"unknown op `status`"}"#));
        let err = status(&sock).unwrap_err();
        assert!(err.contains("unknown op"), "{err}");
        std::fs::remove_file(&sock).ok();
    }

    /// A malformed row (a field missing, or of the wrong type) is a taught
    /// `Err` naming the field and the secret — never a defaulted value.
    #[test]
    fn status_reports_a_malformed_row_as_a_taught_err() {
        let sock = one_shot_broker(
            "status-malformed",
            Some(
                r#"{"ok":true,"home":"/h","secrets":[{"name":"db-prod","backend":"age","requireTotp":"yes","consumers":[],"automation":{"enabled":false,"consumers":[]},"sharedWith":[],"remote":false,"allowRemoteOrigin":false}]}"#,
            ),
        );
        let err = status(&sock).unwrap_err();
        assert!(err.contains("requireTotp") && err.contains("db-prod"), "{err}");
        std::fs::remove_file(&sock).ok();
    }

    /// **The bound, proven:** a broker that accepts the connection and never
    /// answers must not hang this call — the read timeout
    /// ([`STATUS_TIMEOUT`]) fires and the caller gets the taught "did not
    /// answer" error, well inside a generous wall-clock budget.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn status_gives_up_on_a_broker_that_never_answers() {
        let sock = one_shot_broker("status-silent", None);
        let started = std::time::Instant::now();
        let err = status(&sock).unwrap_err();
        let elapsed = started.elapsed();
        assert!(err.contains("did not answer"), "{err}");
        assert!(elapsed >= STATUS_TIMEOUT, "must actually wait out its own bound, took {elapsed:?}");
        assert!(elapsed < STATUS_TIMEOUT + Duration::from_secs(5), "must not overrun the bound, took {elapsed:?}");
        std::fs::remove_file(&sock).ok();
    }

    #[test]
    fn no_production_string_teaches_a_totp_code_in_argv() {
        let sources = [
            ("client.rs", include_str!("client.rs")),
            ("commands.rs", include_str!("commands.rs")),
            ("broker.rs", include_str!("broker.rs")),
            ("watch.rs", include_str!("watch.rs")),
            ("park.rs", include_str!("park.rs")),
            ("lib.rs", include_str!("lib.rs")),
        ];
        let mut hits = Vec::new();
        for (name, text) in sources {
            let production = text.split("\n#[cfg(test)]\nmod tests").next().unwrap_or(text);
            for (n, line) in production.lines().enumerate() {
                let code_line = !line.trim_start().starts_with("//");
                if code_line && ["--totp <code>", "--totp 123456", "--totp NNNNNN"].iter().any(|p| line.contains(p)) {
                    hits.push(format!("{name}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(hits.is_empty(), "production text still teaches a code in argv:\n{}", hits.join("\n"));
    }
}

#[cfg(all(test, unix))]
mod argv_free_tests {
    use super::*;
    use crate::fixtures::{bare_digits_contain, code_now, enrolled_home, ops, real_broker, scripted, Scripted};
    use aoide_protocol::Door;
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const ROLE: &str = "AOIDE_C1_ROLE";
    const OUT: &str = "AOIDE_C1_OUT";
    const SOCKET: &str = "AOIDE_C1_SOCKET";
    const PARK_ID: &str = "3f2a-3";
    const PARKED_ROW: &str = r#"{"id":"3f2a-3","secret":"db-prod","consumer":"m","requestedAt":1700000000,"peerUid":1000,"reason":"psql","origin":{"username":"khoa","pid":42,"comm":"psql","hostname":"box"}}"#;
    const TIMED_OUT: &str = r#"{"ok":false,"error":"the pending TOTP ask for `locked` timed out after 300s"}"#;

    fn role() -> Option<String> {
        std::env::var(ROLE).ok()
    }

    fn park_line(id: &str) -> String {
        format!(r#"{{"interim":true,"parked":true,"id":"{id}","timeoutSecs":300}}"#)
    }

    fn inv_of(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
            door: Door::Cli,
        }
    }

    fn exec_inv(flags: &[(&str, &str)], args: &[&str]) -> Invocation {
        inv_of(&["secrets", "exec"], args, flags)
    }

    fn approve_inv(id: &str, flags: &[(&str, &str)]) -> Invocation {
        inv_of(&["secrets", "approve"], &[id], flags)
    }

    /// Answers `pending` with `rows` (a JSON array), `approve` with `approve_reply`.
    fn park_broker(rows: String, approve_reply: &'static str) -> Scripted {
        scripted("c1-approve", move |req| match req["op"].as_str() {
            Some("pending") => format!(r#"{{"ok":true,"pending":{rows}}}"#),
            Some("approve") => approve_reply.to_string(),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        })
    }

    fn approves(s: &Scripted) -> Vec<Value> {
        s.requests.lock().unwrap().iter().filter(|r| r["op"] == "approve").cloned().collect()
    }

    fn wait_for_park(socket: &Path) -> String {
        for _ in 0..500 {
            if let Some(ask) = pending(socket).unwrap().first() {
                return ask.id.clone();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("nothing parked on {}", socket.display());
    }

    /// Sets env vars for one test; the old values come back on drop, so a
    /// panic cannot leak them into the next test.
    struct Env(Vec<(&'static str, Option<String>)>);

    impl Env {
        fn set(pairs: &[(&'static str, String)]) -> Self {
            let saved = pairs.iter().map(|(k, _)| (*k, std::env::var(k).ok())).collect();
            for (k, v) in pairs {
                std::env::set_var(k, v);
            }
            Env(saved)
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            for (k, v) in &self.0 {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    /// Output that the test can read after handing it to the code under test.
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A code reader that records what the output held when it was first asked for a byte.
    struct Probe {
        shown: Arc<Mutex<Vec<u8>>>,
        at_first_read: Option<String>,
        rest: &'static [u8],
    }

    impl Read for Probe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.at_first_read.is_none() {
                self.at_first_read = Some(String::from_utf8_lossy(&self.shown.lock().unwrap()).into_owned());
            }
            self.rest.read(buf)
        }
    }

    fn open_pty() -> (File, File) {
        let (mut master, mut slave) = (0, 0);
        let rc = unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), std::ptr::null()) };
        assert_eq!(rc, 0, "openpty failed");
        unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) }
    }

    /// One poll-bounded read of the master: false on EOF, once the child side
    /// has closed, or at the deadline.
    fn pty_step(master: &mut File, transcript: &mut Vec<u8>, deadline: Instant) -> bool {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        let mut fd = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut fd, 1, left.as_millis().min(500) as i32) } <= 0 {
            return true;
        }
        let mut buf = [0u8; 4096];
        match master.read(&mut buf) {
            Ok(n) if n > 0 => {
                transcript.extend_from_slice(&buf[..n]);
                true
            }
            _ => false,
        }
    }

    fn pty_read_until(master: &mut File, transcript: &mut Vec<u8>, needle: &[u8], limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while !transcript.windows(needle.len()).any(|w| w == needle) {
            if !pty_step(master, transcript, deadline) {
                return false;
            }
        }
        true
    }

    fn pty_drain(master: &mut File, transcript: &mut Vec<u8>, limit: Duration) {
        let deadline = Instant::now() + limit;
        while pty_step(master, transcript, deadline) {}
    }

    /// Runs one test of this binary in a fresh process with `role` set, so the
    /// test body owns the child's fd 0. With `tty`, the child's stdio is that
    /// terminal and the child leads its own session on it.
    fn spawn_role(test: &str, role: &str, envs: &[(&str, &std::ffi::OsStr)], tty: Option<File>) -> std::process::Child {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", test, "--nocapture", "--test-threads=1"]).env(ROLE, role);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        match tty {
            Some(slave) => {
                let fd = slave.as_raw_fd();
                let stdio = || unsafe { Stdio::from(File::from_raw_fd(libc::dup(fd))) };
                cmd.stdin(stdio()).stdout(stdio()).stderr(stdio());
                unsafe {
                    cmd.pre_exec(|| {
                        libc::setsid();
                        libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                        Ok(())
                    });
                }
            }
            None => {
                cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
            }
        }
        cmd.spawn().unwrap()
    }

    #[test]
    fn approve_refuses_a_totp_flag_before_any_broker_op() {
        let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
        let mut input: &[u8] = b"";
        let result = approve_with(&approve_inv(PARK_ID, &[("totp", "123456")]), &s.socket, &mut input, &mut Vec::new(), false);
        assert_eq!(ops(&s.requests), Vec::<String>::new(), "--totp reached the broker: {result:?}");
        let err = result.expect_err("--totp must be refused");
        assert!(err.what.contains("--totp"), "{err:?}");
        assert!(!format!("{err:?}").contains("123456"), "the refusal echoed the code: {err:?}");
    }

    /// F3: the approve screen names the program the requester says receives the
    /// value, as a claim, so a person sees what the code releases before typing it.
    #[test]
    fn approve_context_names_the_parked_argv0_as_a_claim() {
        let row = r#"{"id":"3f2a-3","secret":"db-prod","consumer":"m","requestedAt":1700000000,"peerUid":1000,"reason":"psql","argv0":"/usr/bin/psql","origin":{"username":"khoa","pid":42,"comm":"psql","hostname":"box"}}"#;
        let s = park_broker(format!("[{row}]"), r#"{"ok":true}"#);
        let mut out = Vec::new();
        approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut &b"123456\n"[..], &mut out, false).unwrap();
        let shown = String::from_utf8(out).unwrap();
        assert!(
            shown.contains("the requester claims (unverified): the program that receives the value is /usr/bin/psql"),
            "{shown}"
        );
    }

    #[test]
    fn exec_refuses_a_totp_flag_before_any_broker_op() {
        let s = scripted("c1-exec-refuse", |_| r#"{"ok":true,"value":"v"}"#.to_string());
        let exec = exec_inv(&[("as", "m"), ("secret", "locked"), ("totp", "123456")], &["true"]);
        let result = run_exec(&exec, &s.socket);
        assert_eq!(s.connections.load(Ordering::SeqCst), 0, "--totp reached the broker: {result:?}");
        let err = result.expect_err("exec must refuse --totp");
        assert!(err.what.contains("--totp"), "{err:?}");
        assert!(!format!("{err:?}").contains("123456"), "the refusal echoed the code: {err:?}");
    }

    #[test]
    fn approve_reads_one_piped_line_and_sends_one_approve_op() {
        let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
        let mut input: &[u8] = b"123456\nTRAILING";
        let id = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut input, &mut Vec::new(), false)
            .expect("a piped code approves");
        assert_eq!(id, PARK_ID);
        let sent = approves(&s);
        assert_eq!(sent.len(), 1, "{:?}", ops(&s.requests));
        assert_eq!(sent[0]["id"], PARK_ID);
        assert_eq!(sent[0]["totp"], "123456");
    }

    #[test]
    fn approve_refuses_an_empty_code_with_no_approve_op() {
        for empty in [&b""[..], &b"\n"[..]] {
            let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
            let mut input = empty;
            let result = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut input, &mut Vec::new(), false);
            assert!(approves(&s).is_empty(), "an empty code reached approve: {:?}", ops(&s.requests));
            assert!(result.is_err(), "{result:?}");
        }
    }

    #[test]
    fn approve_refuses_an_unknown_id_before_it_reads_a_code() {
        let other = r#"[{"id":"other-1","secret":"x","consumer":"m","requestedAt":1,"peerUid":1000,"origin":{}}]"#;
        let s = park_broker(other.to_string(), r#"{"ok":true}"#);
        let mut input: &[u8] = b"123456\n";
        let result = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut input, &mut Vec::new(), false);
        assert_eq!(ops(&s.requests), ["pending"], "{result:?}");
        assert!(result.is_err(), "{result:?}");
        assert_eq!(input, &b"123456\n"[..], "a code was read for an id that is not parked");
    }

    #[test]
    fn approve_shows_the_park_and_labels_the_claims_before_it_reads_the_code() {
        let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
        let shown = Arc::new(Mutex::new(Vec::new()));
        let mut probe = Probe { shown: shown.clone(), at_first_read: None, rest: b"123456\n" };
        let outcome = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut probe, &mut Shared(shown.clone()), false);
        assert!(outcome.is_ok(), "{outcome:?}");
        let before_read = probe.at_first_read.expect("the code was never read");
        for claim in ["db-prod", "consumer", "psql", "1000", "requester", "kernel"] {
            assert!(before_read.contains(claim), "`{claim}` is not shown before the code is read:\n{before_read}");
        }
    }

    #[test]
    fn approve_shows_the_park_origin_pid_before_it_reads_the_code() {
        let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
        let shown = Arc::new(Mutex::new(Vec::new()));
        let mut probe = Probe { shown: shown.clone(), at_first_read: None, rest: b"123456\n" };
        let outcome = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut probe, &mut Shared(shown.clone()), false);
        assert!(outcome.is_ok(), "{outcome:?}");
        let before_read = probe.at_first_read.expect("the code was never read");
        assert!(
            before_read.contains("from: khoa \u{b7} psql (pid 42) @ box"),
            "the park's origin line, with its park-time pid 42, is not shown before the code is read:\n{before_read}"
        );
    }

    #[test]
    fn approve_prompts_without_echo_on_a_tty() {
        if role().as_deref() == Some("pty-approve") {
            let s = park_broker(format!("[{PARKED_ROW}]"), r#"{"ok":true}"#);
            let result = approve_with(&approve_inv(PARK_ID, &[]), &s.socket, &mut RawStdin, &mut std::io::stderr(), crate::client::stdin_is_tty());
            let record = json!({ "ok": result.is_ok(), "approves": approves(&s) });
            std::fs::write(std::env::var(OUT).unwrap(), record.to_string()).unwrap();
            return;
        }
        let out = std::env::temp_dir().join(format!("c1-pty-{}.json", std::process::id()));
        let (mut master, slave) = open_pty();
        let test = "client::argv_free_tests::approve_prompts_without_echo_on_a_tty";
        let mut child = spawn_role(test, "pty-approve", &[(OUT, out.as_os_str())], Some(slave));
        let mut transcript = Vec::new();
        assert!(
            pty_read_until(&mut master, &mut transcript, b"TOTP code", Duration::from_secs(15)),
            "no prompt on the tty:\n{}",
            String::from_utf8_lossy(&transcript)
        );
        let typed_from = transcript.len();
        master.write_all(b"123456\r").unwrap();
        pty_drain(&mut master, &mut transcript, Duration::from_secs(15));
        let status = child.wait().unwrap();
        let after_prompt = String::from_utf8_lossy(&transcript[typed_from..]).into_owned();
        assert!(!after_prompt.contains("123456"), "the typed code echoed on the tty:\n{after_prompt}");
        assert!(status.success(), "the child failed:\n{}", String::from_utf8_lossy(&transcript));
        let record: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(record["approves"][0]["totp"], "123456", "{record}");
    }

    #[test]
    fn read_line_unbuffered_takes_one_line_and_leaves_the_rest() {
        let mut input: &[u8] = b"123456\nREST";
        assert_eq!(read_line_unbuffered(&mut input).unwrap(), "123456");
        assert_eq!(input, &b"REST"[..], "the reader took more than its line");
    }

    /// F21: a code line is capped at 256 bytes. A line up to the cap is read;
    /// one byte past it is refused, so a writer that never sends a newline
    /// cannot grow the buffer without bound.
    #[test]
    fn read_line_unbuffered_refuses_a_line_past_the_cap() {
        let at_cap = format!("{}\n", "1".repeat(256));
        assert_eq!(read_line_unbuffered(&mut at_cap.as_bytes()).unwrap().len(), 256);
        let past = "1".repeat(257);
        let mut past_cap: &[u8] = past.as_bytes();
        assert!(read_line_unbuffered(&mut past_cap).is_err(), "a 257-byte line must be refused, not buffered");
    }

    #[test]
    fn unparked_exec_leaves_the_child_stdin_whole() {
        if role().as_deref() == Some("unparked-exec") {
            let s = scripted("c1-unparked", |_| r#"{"ok":true,"value":"v"}"#.to_string());
            let exec = exec_inv(&[("as", "m"), ("secret", "locked")], &["sh", "-c", "cat > \"$AOIDE_C1_OUT\""]);
            assert_eq!(run_exec(&exec, &s.socket).unwrap(), 0);
            return;
        }
        let out = std::env::temp_dir().join(format!("c1-unparked-{}.txt", std::process::id()));
        let test = "client::argv_free_tests::unparked_exec_leaves_the_child_stdin_whole";
        let mut child = spawn_role(test, "unparked-exec", &[(OUT, out.as_os_str())], None);
        child.stdin.take().unwrap().write_all(b"one\ntwo\n").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn parked_exec_reads_one_line_approves_its_own_park_and_the_child_reads_the_rest() {
        if role().as_deref() == Some("parked-exec") {
            let socket = std::env::var(SOCKET).unwrap();
            let exec = exec_inv(&[("as", "m"), ("secret", "locked")], &["sh", "-c", "cat > \"$AOIDE_C1_OUT\""]);
            assert_eq!(run_exec(&exec, Path::new(&socket)).unwrap(), 0);
            return;
        }
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (home, secret) = enrolled_home("c1-parked-exec");
        let _env = Env::set(&[
            ("AOIDE_SECRETS_PARK_TIMEOUT", "2".to_string()),
            ("AOIDE_AUDIT_LOG", home.join("mirrored-aoide-log").display().to_string()),
        ]);
        let socket = real_broker(&home, "c1-parked-exec");
        let out = std::env::temp_dir().join(format!("c1-parked-{}.txt", std::process::id()));
        let test = "client::argv_free_tests::parked_exec_reads_one_line_approves_its_own_park_and_the_child_reads_the_rest";
        let mut child = spawn_role(test, "parked-exec", &[(SOCKET, socket.as_os_str()), (OUT, out.as_os_str())], None);
        let line = format!("{}\nREST", code_now(&secret));
        child.stdin.take().unwrap().write_all(line.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "the parked exec failed:\n{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "REST", "the child did not get the rest of its stdin");
    }

    /// F4: the code a park's exec prompt asks for releases the secret, so the
    /// prompt shows what it releases first, read from the park's own row (the
    /// `approve` context), before any code is read.
    #[test]
    fn parked_exec_shows_what_the_code_releases_before_it_reads_the_code() {
        let s = scripted("c1-exec-context", |req| match req["op"].as_str() {
            Some("resolve") => format!("{}\n{}", park_line(PARK_ID), r#"{"ok":true,"value":"stored-value"}"#),
            Some("pending") => format!(r#"{{"ok":true,"pending":[{PARKED_ROW}]}}"#),
            Some("approve") => r#"{"ok":true}"#.to_string(),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let mut probe = Probe { shown: shown.clone(), at_first_read: None, rest: b"123456\nREST" };
        let value = resolve_for_exec(&s.socket, &exec, &mut probe, false, &mut Shared(shown.clone()));
        assert_eq!(value.expect("the value returns on the first connection"), "stored-value");
        let before_read = probe.at_first_read.expect("the code was never read");
        for claim in ["db-prod", "consumer", "psql", "requester", "kernel"] {
            assert!(before_read.contains(claim), "`{claim}` is not shown before the code is read:\n{before_read}");
        }
    }

    /// F20: the park line gives the instant the ask expires, read from the
    /// broker's interim line, so a person typing a code sees the window.
    #[test]
    fn parked_exec_prints_the_instant_the_park_expires() {
        let interim = format!(r#"{{"interim":true,"parked":true,"id":"{PARK_ID}","timeoutSecs":300,"expiresAt":1700000300}}"#);
        let s = scripted("c1-expiry-instant", move |req| match req["op"].as_str() {
            Some("resolve") => format!("{interim}\n{}", r#"{"ok":true,"value":"stored-value"}"#),
            Some("pending") => format!(r#"{{"ok":true,"pending":[{PARKED_ROW}]}}"#),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let mut input = std::io::empty();
        let value = resolve_for_exec(&s.socket, &exec, &mut input, false, &mut Shared(shown.clone()));
        assert_eq!(value.expect("the value returns on the first connection"), "stored-value");
        let shown = String::from_utf8(shown.lock().unwrap().clone()).unwrap();
        assert!(shown.contains("expires at unix 1700000300"), "the park line must give the expiry instant:\n{shown}");
    }

    /// F4: a park whose row cannot be read asks for no code, because the code
    /// is what releases the value and nothing would show what it releases.
    #[test]
    fn parked_exec_asks_for_no_code_when_the_park_cannot_be_read() {
        let s = scripted("c1-no-row", |req| match req["op"].as_str() {
            Some("resolve") => format!("{}\n{TIMED_OUT}", park_line(PARK_ID)),
            Some("pending") => r#"{"ok":true,"pending":[]}"#.to_string(),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let mut input: &[u8] = b"123456\nREST";
        let mut out = Vec::new();
        let result = resolve_for_exec(&s.socket, &exec, &mut input, false, &mut out);
        assert_eq!(ops(&s.requests), ["resolve", "pending"], "no approve, and no code asked for, without the row");
        assert_eq!(input, &b"123456\nREST"[..], "a code line was read although the park was not shown");
        let said = String::from_utf8_lossy(&out).into_owned();
        assert!(said.contains("no code asked for"), "{said}");
        assert!(result.expect_err("the final reply is the timeout").contains("timed out"));
    }

    #[test]
    fn parked_exec_pipe_eof_prints_the_approve_line_and_keeps_waiting() {
        let s = scripted("c1-eof", |req| match req["op"].as_str() {
            Some("pending") => format!(r#"{{"ok":true,"pending":[{PARKED_ROW}]}}"#),
            _ => format!("{}\n{TIMED_OUT}", park_line(PARK_ID)),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let mut out = Vec::new();
        let result = resolve_for_exec(&s.socket, &exec, &mut &b""[..], false, &mut out);
        let said = String::from_utf8_lossy(&out).into_owned();
        assert_eq!(ops(&s.requests), ["resolve", "pending"], "end of stdin must not approve");
        assert!(said.contains("aoide secrets approve 3f2a-3"), "{said}");
        assert!(!said.contains("--totp"), "{said}");
        let err = result.expect_err("the final reply is the timeout");
        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn parked_exec_approves_its_own_park_on_a_second_connection_and_returns_the_value() {
        let s = scripted("c1-own-park", |req| match req["op"].as_str() {
            Some("resolve") => format!("{}\n{}", park_line(PARK_ID), r#"{"ok":true,"value":"stored-value"}"#),
            Some("pending") => format!(r#"{{"ok":true,"pending":[{PARKED_ROW}]}}"#),
            Some("approve") => r#"{"ok":true}"#.to_string(),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let mut input: &[u8] = b"123456\nREST";
        let mut out = Vec::new();
        let value = resolve_for_exec(&s.socket, &exec, &mut input, false, &mut out);
        assert_eq!(ops(&s.requests), ["resolve", "pending", "approve"]);
        let sent = approves(&s);
        assert_eq!(sent[0]["id"], PARK_ID);
        assert_eq!(sent[0]["totp"], "123456");
        assert_eq!(value.expect("the value returns on the first connection"), "stored-value");
        assert_eq!(input, &b"REST"[..], "the code line was not read one byte at a time");
    }

    #[test]
    fn parked_exec_reports_a_wrong_code_once_and_keeps_waiting() {
        let s = scripted("c1-wrong", |req| match req["op"].as_str() {
            Some("resolve") => format!("{}\n{TIMED_OUT}", park_line(PARK_ID)),
            Some("pending") => format!(r#"{{"ok":true,"pending":[{PARKED_ROW}]}}"#),
            Some("approve") => r#"{"ok":false,"error":"the TOTP code is wrong"}"#.to_string(),
            _ => r#"{"ok":false,"error":"unexpected op"}"#.to_string(),
        });
        let exec = parse_exec_args(&exec_inv(&[("as", "m"), ("secret", "locked")], &["true"])).unwrap();
        let mut input: &[u8] = b"000000\nREST";
        let mut out = Vec::new();
        let result = resolve_for_exec(&s.socket, &exec, &mut input, false, &mut out);
        assert_eq!(ops(&s.requests), ["resolve", "pending", "approve"], "a wrong code is one attempt, not a loop");
        let said = String::from_utf8_lossy(&out).into_owned();
        assert!(said.contains("the TOTP code is wrong"), "{said}");
        assert!(result.expect_err("the final reply is the timeout").contains("timed out"));
    }

    #[test]
    fn park_hint_names_the_approve_line_and_no_totp_code() {
        let hint = park_hint(PARK_ID, 300, None);
        assert!(hint.contains("aoide secrets approve 3f2a-3"), "{hint}");
        assert!(!hint.contains("--totp"), "{hint}");
    }

    #[test]
    fn the_code_reaches_no_outcome_no_log_no_feed_and_no_environment() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (home, secret) = enrolled_home("c1-no-leak");
        let mirror = home.join("mirrored-aoide-log");
        let events = home.join("events.jsonl");
        let _env = Env::set(&[
            ("AOIDE_AUDIT_LOG", mirror.display().to_string()),
            ("AOIDE_SECRETS_EVENTS", events.display().to_string()),
            ("AOIDE_SECRETS_PARK_TIMEOUT", "5".to_string()),
        ]);
        let socket = real_broker(&home, "c1-no-leak");
        let parked = {
            let socket = socket.clone();
            std::thread::spawn(move || resolve(&socket, "locked", "m", None, None, None))
        };
        let id = wait_for_park(&socket);
        let code = code_now(&secret);
        let line = format!("{code}\n");
        let mut input = line.as_bytes();
        let mut out = Vec::new();
        let released = approve_with(&approve_inv(&id, &[]), &socket, &mut input, &mut out, false);
        released.expect("a right code approves");
        assert_eq!(parked.join().unwrap().expect("the parked caller gets the value"), "stored-value");
        let mut seen = String::from_utf8_lossy(&out).into_owned();
        for file in [home.join("audit.log"), mirror, events] {
            seen += &std::fs::read_to_string(file).unwrap_or_default();
        }
        assert!(!bare_digits_contain(&seen, &code), "the code reached an outcome, a log or the feed:\n{seen}");
        assert!(!std::env::vars().any(|(_, v)| bare_digits_contain(&v, &code)), "the code reached the environment");
    }
}
