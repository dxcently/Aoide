# aoide core · POSIX portability

> **Status: the required baseline is NOT met yet.** This page records, per
> capability, where core (`aoide`/`aoided` and their dependency closure)
> stands against POSIX.1-2008. It is deliberately *not* a certification and
> not a claim that core runs on a POSIX host today: the Linux source and the
> Linux test suite are what has actually been run (see "Evidence and limits").

## Targets

**Linux and native Windows are the primary targets; macOS is later.**
Aoide core must run on Windows without WSL, MSYS or Cygwin. Shared logic
retains the same core contracts; Windows implementations must preserve
their identity, policy, process and storage guarantees using native APIs.
POSIX compatibility guides the Unix implementation and later macOS work;
it does not establish native Windows support. Only Linux has runtime
evidence here. The Unix guards below are not Windows implementations.

Native Windows lacks `std::os::unix`, which core currently uses for
`std::os::unix::net::{UnixStream, UnixListener}` (the sockets),
`std::os::unix::fs::PermissionsExt`/`fs::symlink` (the modes and links),
`std::os::unix::io::AsRawFd` (the raw fds the lock and pidfd paths need),
`std::os::unix::ffi::OsStrExt`, `std::os::unix::process`. Each is a compile
blocker on that target before any question of syscall semantics is reached;
the `libc` shapes the rows below name (`sockaddr_un`, `flock`,
`SO_PEERCRED`, `pidfd_*`) sit behind that. Replacing these platform bindings
is required work, not an optional WSL deployment path.

## What is portable, what is a capability

The repo's own rule (root `AGENTS.md`): a portable capability is written
against a POSIX process table and unix file locks; a `cfg(target_os)` branch
is a **tie-break or a taught refusal, never a second discovery path**. A
capability that exists only on one host is allowed to say so by name and
refuse elsewhere — the precedent is `conduct/src/graph/actions.rs`'s
`terminate_verified` ("verified process termination requires Linux pidfds").

Two classes follow from that, and every row below is one of them:

- **required baseline** — must work on any POSIX.1-2008 host;
- **optional host-specific** — a refusal (or a named degradation) is the
  correct behavior off its host, and must be legible, never silent.

## Matrix

| capability | sites | class / status |
| --- | --- | --- |
| process liveness | `storage/src/fs.rs::pid_is_alive`, re-exported as `conduct::reap::proc_exists` and imported by `client/src/{tunnel,pair_watch}.rs` | required; **one portable probe** — `kill(pid, 0)`, with `0` and any value above `pid_t::MAX` rejected as process-group names, `ESRCH` alone read as absent, and every other errno (including `EPERM`) conservatively live. Liveness is not identity |
| peer credentials (`SO_PEERCRED`) | `conduct/src/graph/identity.rs::peer_cred`, `secrets/src/peercred.rs::peer_cred`; consumed by `server/src/daemon.rs::cross_uid_gate` | **required for the daemon dispatch door; Linux/Android mechanism only**. Other hosts return unidentified `None`: `aoided` refuses every dispatch connection, the shell bridge refuses connections, and secrets `dismiss`/`admin` refuse. The source guards remove two compile blockers but leave the core runtime baseline unmet. POSIX defines no peer-credential API; BSD `getpeereid` lacks the PID required by the ancestry checks. |
| verified process termination | `conduct/src/graph/actions.rs` (pidfd) | optional host-specific, **Linux**; taught refusal off Linux |
| PTY / controlling tty | `conduct/src/graph/conduct.rs` (`spawn_on_pty`: `libc::openpty` + `setsid` + `TIOCSCTTY` + `dup2`; detached spawn through `std::os::unix::process::CommandExt::pre_exec` in `conduct/src/graph/spawn.rs`) | required for the interactive conduct channel — the PTY *is* the channel a managed task run's live view reads and `send` types into. **Remaining**: no non-Linux arm exists. Native Windows needs ConPTY (`CreatePseudoConsole`) for interactive parity, or a pipes-only transport for headless parity (no controlling tty, so `send`-injection and windowed launch are taught refusals naming the missing capability). A `cfg` branch alone proves nothing here. |
| argv / `comm` / `cwd` / ancestry identity | `conduct/src/graph/conduct.rs` (`cwd`, `cmdline`, `comm`, `/proc/<pid>/task/<pid>/children`), `client/src/tunnel.rs::looks_like_our_ssh`, `storage/src/attest.rs::pid_starttime`/`parent_pid`/`pid_ancestry`, `secrets/src/peercred.rs::read_comm` | required; **remaining `/proc` reads** — the sealed-credential pid-reuse defence reads `/proc/<pid>/stat` starttime, which has no POSIX equivalent, so off Linux that defence resolves nothing and the sealed-identity lane does not verify |
| lock probe | `protocol/src/dialog.rs::probe_locker_running` | required; **remaining `/proc` scan** for the locker's `comm`, and an unreadable `/proc` answers "not running" (`probe_loginctl_locked` is systemd-logind only) |
| boot epoch | `conduct/src/reap.rs::boot_epoch` (`/proc/stat` `btime`, reused by `server/src/daemon.rs`) | required; **remaining** — off Linux `boot_epoch` is `None`, so the pre-boot reap signal and the boot-epoch-guarded auto-resume never fire |
| runtime dir | `conduct/src/graph/conduct.rs::conduct_socket_path`/`channel_socket_path`, `conduct/src/shellbridge.rs`, `storage/src/tunnel.rs::runtime_dir` | required; **remaining** — a hardcoded `/run/user/1000` fallback (logind-shaped, and wrong for any uid ≠ 1000), and no `sun_path` budget check on the bind side |
| unix socket addressing | `secrets/src/client.rs::unix_sockaddr` | required; capacity and field offset come from the target's `sockaddr_un`. Embedded NULs are refused; empty paths and unused fields follow Rust's `SocketAddr::from_pathname` convention, including leaving BSD `sun_len` zero. Linux boundary and real-socket tests pass; other platform layouts remain unverified at runtime. |
| shell resolution | `conduct/src/graph/resurrect.rs::passwd_login_shell` | required; **remaining** — a `getent passwd` shell-out (glibc/nss; its absence falls back to `/bin/sh`) |
| `ps` invocation | `conduct/src/graph/codex_app.rs::process_table` | required, **named dependency** — `ps -axo pid=,ppid=,command=` works on Linux/macOS/FreeBSD; POSIX.1 only mandates `ps -e -f -o <format>`, so a strict/`BusyBox` `ps` may reject it |
| stage/state lock | `storage/src/fs.rs`, `storage/src/outbox.rs`, `conduct/src/graph/codex_app.rs::lock_is_held` | required, **non-POSIX primitive by design** — `flock(2)` is BSD/XSI, not POSIX.1 (POSIX offers `fcntl(F_SETLK)`, per-process and dropped when any fd to the file closes). The guarantees here — the lock surviving a `fork`/`setsid` into a detached child, and `lock_is_held` as a liveness probe that never creates the file — rest on open-file-description semantics |
| append-only feed file (create policy + tail identity) | `protocol/src/feed.rs`, private `protocol/src/feed_windows.rs` | required baseline for the algorithm (append · cap · truncate-in-place · tail); **policy by host, never silently weakened**, `pkgs/aoide/crates/protocol/README.md`'s `feed` entry names the seams. Unix: `chmod` to the caller's exact `create_mode`, `(dev, ino)` identity — unchanged. Native Windows: owner-only policy attached at creation and read back before the first payload byte (a filesystem that ignores ACLs refuses), an existing file validated on its own handle before truncate/append, reparse points refused, identity by native 128-bit file id; **`0o600` is the only supported creation mode** — `0o640` (the group-shared broker feed) is refused by name, so the group feed is unavailable there, not narrowed. Compile-checked for `x86_64-pc-windows-gnu` and exercised natively on ThinkChiyo with MSVC; see the bounded runtime evidence below. |
| desktop / systemd capabilities in core | `hyprctl` window ops, `loginctl` lock gate, power actions, `notify-send`, `zenity`/`lyra` dialogs, `/run`+`/var/lib` deployment paths | optional host-specific — window ops gate on `HYPRLAND_INSTANCE_SIGNATURE` and degrade to `None`; power actions surface a spawn failure rather than a named refusal; the deployment paths are env-overridable placeholders, not POSIX shapes |

## Evidence and limits

- **Run**: Linux tests on `x86_64-unknown-linux-gnu`, plus the isolated
  protocol feed tests on ThinkChiyo using native `x86_64-pc-windows-msvc`.
  All 17 Windows-runnable feed tests pass, including ACL readback/refusal,
  append/truncation and file replacement. This is module evidence, not a
  working native core deployment.
- **Not run**: macOS/FreeBSD runtime checks and the remaining Windows core
  capabilities. No POSIX conformance claim follows from a `cfg` branch — `cfg(unix)` is
  not evidence of POSIX (the tree already contains `cfg(unix)` branches whose
  *semantics* are Linux: `SO_PEERCRED`, `flock`). The protocol library and
  isolated native Windows test modules compile for `x86_64-pc-windows-gnu`;
  this does not establish a working Windows core or POSIX conformance.
- **Per-OS claims above** (which targets define `libc::ucred`, the `sun_path`
  width, `sa_family_t`'s width, `getpeereid`'s signature) were read from the
  workspace's libc 0.2.189 source, not from documentation.
- **Where a standard-library convention exists, it is followed rather than a
  second ABI opinion invented.** The unix-socket row's `sun_len` answer is
  `std`'s: the toolchain's own std source tree
  (`rust-lib-src/std/src/os/unix/net/addr.rs`, byte-identical to
  `rust-lang/rust` 1.97.1, sha256 `07465ce6…`) sets `sun_family` and nothing
  else, and the string `sun_len` occurs zero times in the whole of std's
  `src/` — so nothing here fills that byte, and nothing needs a per-target
  list to decide whether to.
- **Test-runner gap**: ThinkChiyo provides a native Windows runner; no
  macOS/FreeBSD runner is configured. Tests asserting
  `/proc` facts, `/run/user/1000` paths, and Linux-only peer-credential reads
  are either Linux-gated (they vanish elsewhere) or red elsewhere, so the
  non-Linux arms need a macOS/FreeBSD runner, per-crate
  (`cargo test -p <crate>`, never `--workspace`).
- **Windows feed runtime evidence**: the real feed modules pass their 17
  native tests on ThinkChiyo with Rust 1.98.1/MSVC. A separate scratch probe
  releases two ready child processes together; each appends 1,000 JSON records
  through the real `FeedWriter`. All 2,000 records are complete, valid and
  unique, with no missing or blank lines. This exercises the documented
  EOF `WriteFile` operation on this host; it does not prove behavior across
  every filesystem, payload size or failure mode. The daily operations ledger
  records the source hashes and evidence locations.
- **Refusal convention**: where a capability is host-specific, the shape is
  the named refusal — `cfg(target_os)` on the existing body plus a non-host arm
  that keeps today's fail-closed semantics — never a fabricated uid/pid, never
  a weaker check standing in for a stronger one, and never a parallel
  discovery mechanism beside the portable path.
