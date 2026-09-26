# aoide core · POSIX portability

> **Status: the required baseline is NOT met yet.** This page records, per
> capability, where core (`aoide`/`aoided` and their dependency closure)
> stands against POSIX.1-2008. It is deliberately *not* a certification, and
> not a claim that the whole closure runs on either host: Linux runs every
> crate's tests, and native Windows runs five of them — `aoide-protocol`,
> `aoide-storage`, `aoide-secrets`, `aoide-upkeep` and `aoide-client` — with
> the rest measured as still failing to compile there. Every number below
> names the run it rests on (see "Evidence and limits").

## Targets

**Linux and native Windows are the primary targets; macOS is later.**
Aoide core must run on Windows without WSL, MSYS or Cygwin. Shared logic
retains the same core contracts; Windows implementations must preserve
their identity, policy, process and storage guarantees using native APIs.
POSIX compatibility guides the Unix implementation and later macOS work;
it does not establish native Windows support. Five crates of the closure do
have native runtime evidence (see "Evidence and limits"); the rest is measured,
and the Unix guards below are not Windows implementations.

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
| process liveness | `storage/src/fs.rs::pid_is_alive`, re-exported as `conduct::reap::proc_exists` and imported by `client/src/{tunnel,pair_watch}.rs` | required; **one portable probe** — `kill(pid, 0)`, with `0` and any value above `pid_t::MAX` rejected as process-group names, `ESRCH` alone read as absent, and every other errno (including `EPERM`) conservatively live. Liveness is not identity. Native Windows: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetExitCodeProcess` out of `protocol/src/win_proc.rs`, the same verdicts one-for-one (`ERROR_INVALID_PARAMETER`/`ERROR_NOT_FOUND` the absent verdict `ESRCH` is, `ERROR_ACCESS_DENIED` live exactly as `EPERM` is, anything unanswerable conservatively live), with `0` and an out-of-range pid refused before the call. Native tests on ThinkChiyo. |
| end a process, and wait for it | `storage/src/fs.rs::terminate`/`wait_for_exit` (the two acts, one seam), called by `client/src/tunnel.rs::terminate_pid` | required, **host-split and NOT the same guarantee on both** — the row the caller must read: Unix sends `SIGTERM`, a REQUEST a trapped, hung or broken child can survive, and `waitpid` (real `wait(2)`, `WNOHANG` or blocking) is what answers whether it died; native Windows has no signal at all, so `TerminateProcess` (`protocol/src/win_proc.rs`) is the primitive — uncatchable, so a survivor is impossible there — and the wait is `WaitForSingleObject`, which is also available for a pid an EARLIER invocation spawned (no `ECHILD`). The three wait verdicts map one-for-one (ended · still running · no wait can succeed), and the acts FAIL silently on every arm: a caller's real question is the wait's. What makes the harder arm acceptable is the caller's own guard, not the primitive: `tunnel::terminate_pid` is reachable only through `kill_if_still_our_ssh`, which has already read the pid's argv, and a dropped loopback forward leaves no remote state for a clean shutdown to unwind. Native tests on ThinkChiyo (`win_proc::tests::terminate_ends_a_child_and_the_wait_reports_exactly_that`, and the client's tunnel tests over a real `ssh.exe`). |
| peer credentials (`SO_PEERCRED`) | `conduct/src/graph/identity.rs::peer_cred`, `secrets/src/peercred.rs::peer_cred`; consumed by `server/src/daemon.rs::cross_uid_gate` | **required for the daemon dispatch door; one identity, two host spellings, never a number invented from a SID.** Unix: `SO_PEERCRED`, read directly through `libc` from Linux/Android only — POSIX defines no peer-credential API and BSD `getpeereid` lacks the PID the ancestry walk needs. Native Windows: `WSAIoctl(SIO_AF_UNIX_GETPEERPID)` (`protocol/src/win_unix.rs`) → pid → `OpenProcess`+`OpenProcessToken`+`GetTokenInformation(TokenUser)` → the peer's user SID (`protocol/src/win_proc.rs`), compared with this process's own token user. `PeerCred` keeps `uid`/`pid` where they were and the SID rides BESIDE them, additive and skipped when absent (`peerSid`) — the wire keeps the number, the new fact is a separate field. Anything unanswerable is REFUSED, never admitted by default: `aoided` refuses every dispatch connection, the shell bridge refuses connections, and secrets `dismiss`/`admin` refuse. Weaker than the Unix read, and stated as such: the Windows fact is pid-based, and a pid is only a NAME for a process — `win_proc::sid_of_pid` re-reads the creation time before and after the token read and refuses on a change, which closes a reuse landing across either read but not one wholly inside the window. |
| shell interpreter for a stored command line | `protocol/src/host_shell.rs` (the one seam), called by `secrets/src/backend.rs::run_backend_command` (backend templates) and `upkeep/src/checklane.rs::run_verify` (`upkeep.verifyCommand`) | required; **one seam, one answer per host, in that host's own language**. Unix: `sh -c <line>`, the line as ONE argv element. Native Windows: `cmd /C <line>` with the line handed over VERBATIM (`CommandExt::raw_arg`) — `cmd` is not a `CommandLineToArgvW` program, it re-applies its own quote rules to `/C`'s remainder, so `arg`'s MSVC quoting mangles any line carrying inner quotes (measured natively: the identical `findstr "^" >…` exits 1 through `arg` and 0 through `raw_arg`; `"%COMSPEC%" /C exit 0` likewise). A template is written FOR a host's interpreter, so the shipped POSIX presets (`cat`, `age`, `pass`, `gopass`, `bw`, `sops`) are REFUSED BY NAME on native Windows at the spawn, with the template shown — never run under `cmd`, never silently stubbed. The separators are the template author's too: `cmd`'s builtins refuse a MIXED path (`type C:\a\b/file` is "The syntax of the command is incorrect.", measured). |
| OS randomness | `protocol/src/host_random.rs` (the one seam), called by `secrets/src/enroll.rs::generate_secret` | required; Unix reads `/dev/urandom` (never blocking once the kernel CSPRNG is seeded), native Windows calls CNG's `BCryptGenRandom` with `BCRYPT_USE_SYSTEM_PREFERRED_RNG`. No pool, no seed, no fallback: a host that cannot hand out OS randomness fails, rather than quietly returning something weaker. |
| verified process termination | `conduct/src/graph/actions.rs` (pidfd) | optional host-specific, **Linux**; taught refusal off Linux |
| PTY / controlling tty | `conduct/src/graph/conduct.rs` (`spawn_on_pty`: `libc::openpty` + `setsid` + `TIOCSCTTY` + `dup2`; detached spawn through `std::os::unix::process::CommandExt::pre_exec` in `conduct/src/graph/spawn.rs`) | required for the interactive conduct channel — the PTY *is* the channel a managed task run's live view reads and `send` types into. **Remaining**: no non-Linux arm exists. Native Windows needs ConPTY (`CreatePseudoConsole`) for interactive parity, or a pipes-only transport for headless parity (no controlling tty, so `send`-injection and windowed launch are taught refusals naming the missing capability). A `cfg` branch alone proves nothing here. |
| argv / `comm` / `cwd` / ancestry identity | `conduct/src/graph/conduct.rs` (`cwd`, `cmdline`, `comm`, `/proc/<pid>/task/<pid>/children`), `client/src/tunnel.rs::looks_like_our_ssh`, `storage/src/attest.rs::pid_starttime`/`parent_pid`/`pid_ancestry`, `secrets/src/peercred.rs::read_comm` | required; **host-split**. The sealed-credential pid-reuse defence reads starttime and ppid: `/proc/<pid>/stat` on Unix, `GetProcessTimes` + one `Toolhelp32` snapshot on native Windows (`storage/src/attest.rs` → `protocol/src/win_proc.rs`), where the start time is the creation `FILETIME` — the same fact in another unit, re-derived fresh and never trusted from the record, so the defence is real on both hosts. **The sealed-identity lane now verifies on native Windows too**: its channel is an `AF_UNIX` socket on both hosts (`std`'s on Unix, `protocol/src/win_unix.rs`'s native binding on Windows), so `storage/src/attest.rs::connect_bounded`/`daemon_seal_pubkey_hex` are ONE body with no second arm — the socket TYPE is the seam, and the refusal that used to stand there (`#[cfg(not(unix))] -> None`) is deleted rather than kept beside it. Evidence: the native fake-daemon round trip and the full `attested_caller` resolution, both un-gated from Unix and green on ThinkChiyo |
| lock probe | `protocol/src/dialog.rs::probe_locker_running` | required; **host-split**: a `/proc` scan on Unix, one `Toolhelp32` snapshot on native Windows (`protocol/src/win_proc.rs`) matching the executable name exactly on Unix (`comm` is a byte string) and case-insensitively on native Windows (a file name there is), with Windows' own `.exe` suffix folded; an unreadable `/proc` — or a snapshot that cannot be taken — answers "not running". `probe_loginctl_locked` stays systemd-logind only, so the Windows gate reads the locker process alone |
| boot epoch | `conduct/src/reap.rs::boot_epoch` (`/proc/stat` `btime`, reused by `server/src/daemon.rs`) | required; **remaining** — off Linux `boot_epoch` is `None`, so the pre-boot reap signal and the boot-epoch-guarded auto-resume never fire |
| runtime dir | `storage/src/runtime_dir.rs::socket_dir` — the ONE authority, called by `storage/src/tunnel.rs`, `storage/src/attest.rs::daemon_socket_path`, `conduct/src/graph/conduct.rs::conduct_socket_path`/`channel_socket_path`, `conduct/src/shellbridge.rs`, `client/src/pair_watch.rs`, `server/src/daemon.rs`, and lyra's preview | required; **one seam, two answers, and a PURE resolution — a path, never a side effect** (`tunnel::list_records`' own tests depend on "the `aoide/` subdirectory does not exist until a writer makes it"). Unix: `$XDG_RUNTIME_DIR` when set to a non-empty value, else `/run/user/<this process's euid>`, then `aoide/` — the shipped literal `/run/user/1000` was the one hard-coded uid in the convention (right for uid 1000, wrong for everyone else) and is now derived; the directory is the session's own, so nothing here creates or chmods anything and one old spelling's empty-string bug (a relative `aoide/…`) is pinned by a test. `$XDG_RUNTIME_DIR` is the override on BOTH hosts — every isolating fixture sets exactly that variable, and an answer that ignored it on one host would read and write the machine's real per-user directory instead of the fixture's. Native Windows default: `%LOCALAPPDATA%\aoide` (this host has no XDG runtime dir; that is its per-user, non-roaming local store). The host provides neither that directory nor an owner-only policy on a new one — an elevated token's default owner is `BUILTIN\Administrators` — so a native socket binder creates it through `aoide_protocol::owner_only::ensure_private_dir`; the path itself is still unchecked against a `sun_path` budget here, while the native transport checks the budget on bind and connect. |
| unix socket addressing | `secrets/src/client.rs::unix_sockaddr` (Unix); `protocol/src/win_unix.rs` (native Windows) | required; capacity and field offset come from the target's `sockaddr_un`. Embedded NULs are refused; empty paths and unused fields follow Rust's `SocketAddr::from_pathname` convention, including leaving BSD `sun_len` zero. Linux boundary and real-socket tests pass; other platform layouts remain unverified at runtime. **Native Windows is a real `AF_UNIX` (Winsock) listener and stream, built once in `protocol/src/win_unix.rs`**: `socket(AF_UNIX)`/`bind`/`listen`/`accept`/`connect`, `WSAStartup` once, and the same `Read`/`Write`/`accept`/`connect`/`shutdown`/timeout surface the Unix callers use from `std::os::unix::net` — one code shape, so a socket path is a socket path on either host; the `sun_path` budget is checked on BIND and CONNECT and refused by name, and the peer pid comes from `WSAIoctl(SIO_AF_UNIX_GETPEERPID)`. **The socket FILE's own policy is the question this row has to answer for the bind site** (`secrets/src/broker.rs::bind_socket` pins the parent directory before the name exists and has no `chmod`-after-bind to mirror the Unix arm's): PROVEN — the parent directory is owner-only BY CONSTRUCTION there and reads back that way, and a bound socket reads back either as carrying the same owner-only policy or as the ONE refusal that object draws — it is a REPARSE POINT (tag `0x80000023`, `IO_REPARSE_TAG_AF_UNIX`, measured), which `owner_only`'s policy reader refuses by name — never as an object detectable as NOT owner-only (`win_unix::tests::a_bound_socket_adds_no_policy_wider_than_the_owner_only_directory`). NOT PROVEN — a cross-account connect REFUSAL: this host has no second account to connect from, and `runas /trustlevel:0x20000` is the SAME user (measured), so what gates a connect here is the directory's traversal right with the file's own policy adding nothing; that a different user is refused is a consequence of the directory policy, not a measurement. |
| shell resolution | `conduct/src/graph/resurrect.rs::passwd_login_shell` | required; **remaining** — a `getent passwd` shell-out (glibc/nss; its absence falls back to `/bin/sh`) |
| `ps` invocation | `conduct/src/graph/codex_app.rs::process_table` | required, **named dependency** — `ps -axo pid=,ppid=,command=` works on Linux/macOS/FreeBSD; POSIX.1 only mandates `ps -e -f -o <format>`, so a strict/`BusyBox` `ps` may reject it |
| stage/state lock | `storage/src/fs.rs`, `storage/src/outbox.rs`, `conduct/src/graph/codex_app.rs::lock_is_held` | required, **non-POSIX primitive by design** — `flock(2)` is BSD/XSI, not POSIX.1 (POSIX offers `fcntl(F_SETLK)`, per-process and dropped when any fd to the file closes). The guarantees here — the lock surviving a `fork`/`setsid` into a detached child, and `lock_is_held` as a liveness probe that never creates the file — rest on open-file-description semantics. Native Windows answers with `LockFileEx` on the same lock files (`LOCKFILE_FAIL_IMMEDIATELY` for the non-blocking probe). What it guarantees INSTEAD of surviving a `fork`: the lock belongs to the HANDLE, so a second handle in the same process blocks exactly as a second process's does — which is what `with_stage_lock`'s per-thread re-entrancy flag exists for — and `lock_is_held`'s "never creates the file" probe is unchanged. That port covers `storage`'s own lock files and their call sites only: `conduct/src/graph/codex_app.rs::lock_is_held`, named in this row's site list, is still `cfg(unix)` and has no Windows arm today (see "Not run" below) The one naming difference: Windows byte-range locks are mandatory where `flock` is advisory; nothing here ever reads or writes a lock file's bytes, so no caller can observe it. Evidence: native `fs_windows` lock tests on ThinkChiyo |
| private file / directory policy | `storage/src/fs.rs::atomic_write_private`/`write_temp_file`/`secure_private_dir`, `protocol/src/owner_only.rs` | required baseline, **policy by host, never silently weakened**. Unix: the temp is created AT `0o600` via `OpenOptions::mode`, the directory `chmod`ed `0o700`. Native Windows: a protected, single-ACE owner-only DACL for the current token user is attached AT CREATION (no create-then-tighten window), and any creation mode but `0o600` is refused by name. It is read back before the first payload byte by both of its consumers — the feed on the handle it just created, and `atomic_write_private`'s temp by path (`file_privacy`, which is also what refuses a reparse point at that path) — and a directory that is a symbolic link or a JUNCTION is refused by name rather than having the link's own policy written or read as the target's. Owner pinning in the tighten path (`set_dir_access`) is evidenced on a NON-admin token: `runas /trustlevel:0x20000` reports `BUILTIN\Administrators` as "Group used for deny only", and the tighten test passes there. A DIRECTORY is not a file here: its policy is the whole of `0o700`, which means `FILE_EXECUTE` (`FILE_TRAVERSE` — open what is inside by path) and `FILE_DELETE_CHILD` (the other half of `w`: on Unix removing an entry asks for write on the PARENT) on top of the file's bits, or the owner has made a directory they cannot list through. `secure_private_dir` creates-or-tightens and refuses a directory whose policy the filesystem would not honor, and a directory policy change is Windows' own re-propagation to children with merely inherited ACEs — a fact its test names. One implementation, exposed from `aoide-protocol` because `aoide-storage` needs the same policy — never a second copy. Exercise: `fs.rs`'s `assert_private_file`/`assert_private_dir` read the host's own policy; native tests on ThinkChiyo. |
| OS hostname | `storage/src/display.rs::os_hostname` | required; Unix asks `gethostname(2)`, native Windows `GetComputerNameExW(ComputerNameDnsHostname)` — the DNS hostname, not the NetBIOS name, with a length checked against the buffer it was given so a too-long name is refused rather than reported truncated. `None` on any failure on either host, never a fabricated name. Verified natively on ThinkChiyo by an ASSERTION rather than by execution: the row's test demands this host's own name (case-folded against `COMPUTERNAME`, suffix-tolerant) and rejects the shared `"aoide"` fallback, so a constant, a `None`, or a never-run arm cannot pass it. |
| append-only feed file (create policy + tail identity) | `protocol/src/feed.rs`, private `protocol/src/feed_windows.rs` | required baseline for the algorithm (append · cap · truncate-in-place · tail); **policy by host, never silently weakened**, `pkgs/aoide/crates/protocol/README.md`'s `feed` entry names the seams — and `protocol/src/owner_only.rs` is that policy factored out for its second consumer, `aoide-storage`'s private-write and private-directory half. Unix: `chmod` to the caller's exact `create_mode`, `(dev, ino)` identity — unchanged. Native Windows: owner-only policy attached at creation and read back before the first payload byte (a filesystem that ignores ACLs refuses), an existing file validated on its own handle before truncate/append, reparse points refused, identity by native 128-bit file id; **`create_mode` is ADVISORY there and the policy is always a protected owner-only DACL**: there is no group reader on native Windows (no lyra/desktop surface) and no gid to name, so the broker's group-shared `0o640` feed is delivered STRICTER than it asks — that same user, nobody else — rather than refused, and a same-user `watch::Follower` sees the line the broker writes (asserted natively). Compile-checked for `x86_64-pc-windows-gnu` and exercised natively on ThinkChiyo with MSVC; see the bounded runtime evidence below. |
| desktop / systemd capabilities in core | `hyprctl` window ops, `loginctl` lock gate, power actions, `notify-send`, `zenity`/`lyra` dialogs, `/run`+`/var/lib` deployment paths | optional host-specific — window ops gate on `HYPRLAND_INSTANCE_SIGNATURE` and degrade to `None`; power actions surface a spawn failure rather than a named refusal; paths are env-overridable placeholders, not POSIX shapes |
| the `ssh` client this box runs | `client/src/tunnel.rs::ssh_program` (the ONE place the binary is chosen), `local_login`/`resolve_login` (the login half); the argv read it is guarded by is `looks_like_our_ssh` → `process_argv` | required, **named difference per host** — Unix runs the bare name `ssh` through `PATH` (the operator's own client); native Windows runs the OpenSSH client that ships with the OS, `%SystemRoot%\System32\OpenSSH\ssh.exe`, by FULL PATH (there `ssh` is not a name `PATH` is guaranteed to carry, and OpenSSH installs outside any standard directory) — refused by name when `%SystemRoot%` is unset, never a guessed `C:\Windows`. The login chain differs for the same reason: `$USER` → `$LOGNAME` on Unix, and `%USERNAME%` last on Windows, where the other two are not variables the OS sets. The identity read differs too — `/proc/<pid>/cmdline` (NUL-split) versus `win_proc::command_argv` (`ProcessCommandLineInformation` + this host's own `CommandLineToArgvW`) — and argv[0] is `ssh` on one host and `ssh.exe` (folded, either separator) on the other. Evidence: a native `ssh_program` assertion against this host's own file, and the tunnel tests that run a REAL `ssh.exe` against a loopback banner listener. **Two differences that do NOT follow from choosing the system binary, stated rather than implied**: (1) the path is the NATIVE one, so a 32-bit (WOW64) build would be redirected to `SysWOW64`, where OpenSSH is not installed — only `x86_64-pc-windows-msvc` is targeted today, so nothing here builds that way yet, and a future i686 target would have to answer it; (2) Win32-OpenSSH is still OpenSSH — it reads `%USERPROFILE%\.ssh\config`, its `known_hosts` and the ssh-agent pipe exactly as any other build does, so `BatchMode=yes`, the `-L` spec and the `authorized_keys` teaching all behave as the Unix arm documents; only the BINARY's location differs |
| a node name this box gives itself | `storage/src/display.rs::local_node_name`, used by `storage/src/mail.rs` (`file_letter`/`file_receipt`/`mint_outbound_letter`/`mint_ack`/legacy migration), `storage/src/seal.rs` (the "is this container for me" and last-hop comparisons), `client/src/{letter_send,mail_wire}.rs` and `client/src/commands.rs`'s signed-header and pair-request names | required, **host-split by an OS fact rather than an API**: a node name in an address is grammar-lowercase (`^[a-z0-9][a-z0-9-]*$`) while an OS host name is case-PRESERVED and native Windows' DNS name is conventionally upper-case (`ThinkChiyo`, measured), so `local_host_name()` folded is the one name this box mints, declares and is looked up under. The raw name stays what a human is shown and what an ssh target spells. The limit is stated, not papered over: a host name carrying anything outside `[a-z0-9-]` still cannot BE a node name — refused by grammar, never mangled. NOT folded, deliberately: the mesh-charter and discovery self-name comparisons (`client/src/mesh.rs`, and the `is_self_target` callers in `client/src/commands.rs`) and `conductor`'s mailbox targets (`conductor/src/app.rs`, `conductor/src/lib.rs`'s `{local_host_name()}/{petname}`), each of which reads `local_host_name()` on BOTH sides — a peer-fed advertisement or an operator's charter names a box the same way this side does, so folding one side alone would break the comparison it exists for; the conductor sites are outside this slice's crates, and pre-existing on a host whose name differs in case. Evidence: `display::tests::local_node_name_is_the_local_host_name_folded` plus the mail/seal suites on ThinkChiyo, where the raw name is upper-case |

## Evidence and limits

- **Run**: Linux tests on `x86_64-unknown-linux-gnu` for every crate in the
  closure — 177 (protocol) + 485 (storage) + 449 (+7 e2e) (secrets) + 31
  (upkeep) + 338 (client) + 272 (server) + 75 (cli) + 957 (conduct) + 191
  (conductor) + 213 (lyra), every one 0 failed, and `cargo test
  --workspace --no-run` clean. The same tree on `x86_64-unknown-linux-gnu` is
  the ONLY place the crates that do not build natively yet are exercised at
  all, which is why their counts are stated here rather than dismissed; the
  native side of this table is the "Run"-shaped bullet above and the "Next
  layer" table. Two facts were asked of a NON-admin token as well, because the
  earlier runs used an elevated one: under `runas /trustlevel:0x20000` —
  `whoami /groups` reporting `BUILTIN\Administrators` as "Group used for deny
  only" — the directory-tighten test passes, so owner pinning through
  `SetSecurityInfo` needs no privilege on a standard-user token either. What
  that token is NOT is a second USER: `whoami` inside it reports the same
  `thinkchiyo\dxcen`, and it reads both a user-pinned owner-only file and a
  plain `BUILTIN\Administrators`-owned one — measured, so it is evidence about
  privileges and none at all about a different account's access. Reported, not
  measured:
  a filesystem that IGNORES ACLs (the readback refuses by name rather than
  writing) and whether `LockFileEx` is supported on exFAT/FAT32 — if it is
  not, the best-effort `with_stage_lock` runs unlocked there while `flock` on
  the same volume still works, and nothing here has run on such a volume. The
  natively-run arms behind the rows above are the owner-only file and
  directory policy read back from the object's own handle, `LockFileEx` held
  against a second handle and against a blocking waiter, `MoveFileExW`'s
  no-clobber, the private-write refusal of any mode but `0o600`, the
  `cmd.exe`-child liveness reading, the native pid-ancestry/creation-time
  defence, the lock probe over a real snapshot, the `AF_UNIX` listener and
  stream (a live pair, accept, `SIO_AF_UNIX_GETPEERPID`, the `sun_path`
  refusal by name), the feed's append/cap/truncate with its owner-only native
  policy, and — this slice's end-to-end paths — the secrets broker over a real
  socket with a real `cmd` template (put → store file → resolve → refusal →
  status → audit log), enrollment over the native `BCryptGenRandom`, the
  secrets backend template's own exit-status discipline, and the check lane's
  exit-code verdict. This is four crates' evidence, not a working native core
  deployment — see the next bullet for what is still behind it.
- **Run**: `aoide-protocol`, `aoide-storage`, `aoide-secrets`,
  `aoide-upkeep` **and `aoide-client`** build AND test on ThinkChiyo with
  native `x86_64-pc-windows-msvc` (Rust 1.98.1): `cargo check -p aoide-client
  --all-targets` is 0 errors, and `cargo test` per crate is 199 (protocol) +
  488 (storage) + 366 (secrets lib) + 2 (secrets e2e) + 32 (upkeep) +
  313 (client) passed / 0 failed. The five crates are 177 + 485 + 449 (+7 e2e)
  + 31 + 338 on `x86_64-unknown-linux-gnu`, also 0 failed. **The client's own
  count is LOWER on Windows by design, not by omission**: 340 tests are defined
  in that crate, 27 of them are gated there with their reasons in-file, and the
  2 this slice added are Windows-only — so 340 − 27 = 313 runs on ThinkChiyo.
  The gate is the fixture: a POSIX `#!/bin/sh` script made executable with a
  mode (`CreateProcess` understands neither a shebang nor an extension-less
  name) — the gated groups are the `curl`-shim transport (14), the
  `zenity`/`lyra` dialog shims (10), the non-blocking-accept daemon fixture (1)
  and the two SIGTERM-survivor tunnel tests (2).
  Every one of those gates names its reason in-file, and what the native host
  does cover is narrower than "every group has a native test":
  `mcp_client::tests::native_windows_the_bearer_value_never_reaches_curls_argv`
  drives the shared transport (`commands::post_json` → `run_curl_capped`) with
  the REAL system `curl.exe` against a loopback listener and reads the live
  child's own command line through `win_proc::command_argv` — the same contract
  as the gated `commands::tests::mcp_credentials_and_session_headers_use_stdin_never_argv_or_body_file`,
  so that one has a native twin. The daemon round trip, the seal-pubkey fetch
  and the tunnel kill/reap path likewise run natively, against the native
  `AF_UNIX` binding and a real `ssh.exe`. The REST of the gated `commands`
  contracts have NO native assertion yet — the ledger row on an ack, the
  no-row-without-a-parent case, the over-cap and under-cap responses, and the
  three "never invokes curl" spies — and the loopback-listener-plus-real-curl
  fixture this slice built is the named way to cover them later. What else has
  no native arm, reported rather than hidden: the self-name comparisons and the
  `conductor` mailbox targets the node-name row above lists as deliberately NOT
  folded — each reads `local_host_name()` on BOTH sides of the match, so
  folding one side would break the very comparison it exists for.

## Next layer

Measured on ThinkChiyo (Rust 1.98.1/MSVC), `cargo check -p aoide-conduct
-p aoide-server -p aoide-cli --all-targets --keep-going`, after this slice's
client fix:

| crate | config | errors | where |
| --- | --- | --- | --- |
| `aoide-client` | `--all-targets` | **0** | — (this slice) |
| `aoide-conduct` | lib | 108 | `graph/conduct.rs`, `reap.rs`, `shellbridge.rs`, `graph/{codex_app,window,spawn,send,eidolon,hooks,resurrect}`, `graph/{doorbell,identity,pending,pingback,trace,view,workspace,codex_capture}` |
| `aoide-conduct` | lib test | 227 | the same files plus their fixtures, `graph/conduct.rs` alone 133 |
| `aoide-server` | `--all-targets` | **not independently measured** | blocked: it depends on `aoide-conduct`, so cargo cannot build it while that crate fails |
| `aoide-cli` | `--all-targets` | **not independently measured** | same block |

**ConPTY / PTY, named separately from the rest**: the PTY sites are
`conduct/src/graph/conduct.rs`'s `spawn_on_pty` (`libc::openpty` + `setsid` +
`TIOCSCTTY` + `dup2`) and `conduct/src/graph/spawn.rs`'s detached-spawn
`pre_exec` — 2 of the 227 error lines mention that vocabulary directly, and the
interactive-parity arm (ConPTY, `CreatePseudoConsole`) remains absent: no
`CreatePseudoConsole` call exists anywhere in the tree, so what is missing
there is a capability, not a `cfg` branch. `aoide-conduct` is the whole of the
next layer: it is the only crate in the remaining closure that fails to
compile, and it fails on the Unix-only vocabulary this file's rows above have
already mapped for the crates below it.

- **Not run**: macOS/FreeBSD runtime checks, and the rest of the core's
  closure on native Windows (the "Next layer" table above is the measured
  extent of what is left). One production path on the native side has no
  evidence at all, and is named here rather than left to look covered:
  **Ctrl-C on a Windows console reaching `client/src/pair_watch.rs`'s
  `install_sigint_handler` (`libc::signal(SIGINT)`)**. That call compiles and
  its `cfg` branch is shared, but whether the CRT's console handler delivers
  the interrupt through this process's handler is UNMEASURED — no test drives
  one, and no run has been done in a Windows console. The code is unchanged by
  that gap; the claim is simply not made. No POSIX
  conformance claim follows
  from a `cfg` branch — `cfg(unix)` is not evidence of POSIX (the tree still
  contains `cfg(unix)` branches whose *semantics* are Linux: `SO_PEERCRED`,
  the `/proc` reads `conduct` owns, `loginctl`). A `cfg(windows)` arm is
  evidence only where a native host ran its tests, which is why every claim
  above names the run it rests on.
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
  macOS/FreeBSD runner is configured. A test that asserts a `/proc` fact, a
  `/run/user/1000` path, or a Linux-only peer-credential read is either
  `cfg`-gated with the reason in place (its host-split sibling covers the
  other side) or still red elsewhere; the non-Linux arms need a
  macOS/FreeBSD runner, per-crate (`cargo test -p <crate>`, never
  `--workspace`). Where a Linux fact is the FIXTURE rather than the subject —
  a symlink needing a privilege, a `umask`, an AF_UNIX socket — the gate is
  the honest answer and the Windows run is told so in the test's own doc,
  never left silently skipped.
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
