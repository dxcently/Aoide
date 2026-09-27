# aoide core · POSIX portability

> **Status: the required baseline is NOT met yet, but the layer is nearly
> closed.** This page records, per capability, where core (`aoide`/`aoided` and
> their dependency closure) stands against POSIX.1-2008. It is deliberately
> *not* a certification, and not a claim that the whole closure runs on either
> host: Linux runs every crate's tests, and native Windows now compiles ALL of
> the core closure — `aoide-protocol`, `aoide-storage`, `aoide-secrets`,
> `aoide-upkeep`, `aoide-client`, `aoide-conduct`, `aoide-server` and
> `aoide-cli`, whose `aoide` AND `aoided` binaries build there too, and runs
> the conducted session itself on ConPTY. That host's tests are green per crate
> — `aoide-server`'s lib 266 passed / 0 failed / 5 ignored with reasons,
> `aoide-conduct` 931 / 0 / 0, `aoide-cli`'s targets 0 failed — and the
> DAEMON is now measured there, not merely compiled: resident and alive past
> 60 s from a scratch root, its control socket serving real CLI invocations,
> the conducted session driven end to end, and the A2A, mail and MCP-stdio
> doors answering (the run is named under "Evidence and limits"). What remains
> open is per row, and named there: `boot_epoch` is Linux-only, so the pre-boot
> reap signal and the boot-epoch-guarded auto-resume never fire off it;
> residency there is an install path rather than a missing mechanism —
> `docs/INSTALL.md` § 7.2's per-user logon task (exercised on that host: both
> processes started without a logout, the door answering its AgentCard on
> `127.0.0.1:8720`, and both still running from a later session) — while the
> tree itself still ships no Windows unit, and a logon task's process ends with
> its logon session; one
> `graph_residency_p_d6` binary is contention-sensitive under full parallelism
> (recorded under "Next layer"); and ONE `aoide-storage` test fails there
> deterministically and PRE-EXISTING —
> `fs::tests::tightening_a_directory_strips_a_child_that_only_inherited_its_access`,
> which also fails at `724fe0f` itself on that box (see "Evidence and limits").
> Every number below names the run it rests on.

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
| PTY / controlling tty | `conduct/src/graph/pty.rs` — the whole capability, one seam with an arm per host: `spawn_on_pty` (`openpty` + `setsid` + `TIOCSCTTY` + `dup2` on Unix; `CreatePseudoConsole` + the `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` attribute on `CreateProcessW` on native Windows), `Pty` (read · write · resize · has-output · hung-up · close), `PtyChild`, `Console` (raw mode + the resize source), `Inbox` (the injection door, gated on the owner-only directory it binds in), `wait_ready` (the one readiness wait), `write_stdout` (the interactive pump's unbuffered sink); the detached, NON-PTY spawn is `std::os::unix::process::CommandExt::pre_exec` / `storage::fs::detach` in `conduct/src/graph/spawn.rs`, and `conduct/src/graph/conduct.rs` multiplexes over the seam and owns the policy | required for the interactive conduct channel — the pty *is* the channel a managed task run's live view reads and `send` types into. **Native on both hosts, and the whole channel is exercised there**: `a_pseudo_console_child_is_read_resized_typed_into_and_observed_to_exit`, `typing_into_the_pseudo_console_reaches_the_childs_own_stdin`, `a_resize_reaches_the_childs_own_console` (the child's own `mode con` reports the resized geometry), `the_childs_own_stdout_is_a_console_not_this_processs_pipe`, `conduct_runs_a_child_on_a_pseudo_console_and_resolves_its_session` (the whole command, headless: registered, mirrored into its log, resolved `done` with the child's real code) and **`a_connection_to_a_live_inbox_types_into_the_conducted_child` — a real `aoide conduct` child, a connection to its own inbox from another process, and the injected bytes asserted to arrive at the child's stdin**, which is the one run that proves the readiness wait can report the inbox (`aoide send`'s whole premise). Plus the mechanism probe `an_af_unix_connection_signals_the_inboxs_event_and_delivers_its_bytes` and the pure `ready_of_wait` table. **What differs by host, by name** (`pty.rs`'s own table): Windows has no signals, so `Ended::Signal` does not exist there and a Ctrl-C termination arrives as the SIGNED `exitCode` the record carries (`0xC000013A` is `-1073741510`); the deadline kill reaches the direct child only (`TerminateProcess` — Windows has no process groups, where Unix's `killpg` takes the child's whole group); `foreground_pgid()` answers `0` (no foreground process group exists), so a conducted SHELL's live cwd/command tick reads idle there; `hung_up()` answers `None` — the seam's shape, not a guessed `false` — because the pseudo console, not the child, holds the output pipe's write end and "a descendant is still attached" is not a question any call in this tree can ask, so `ptyHeldAfterExit` is never stamped there; and a reader must keep draining a pseudo console for a settle window after its client exits, because conhost renders on its own pipeline — a pty needs none. **That last one has an exception worth stating**: after the DIRECT child has exited the loop's own deadline is no longer consulted (the deadline governs a running child), and the settle clock resets on every byte, so a descendant that keeps writing holds the drain open — the same shape the Unix loop always had, and the reason "continuous output is never a reprieve" is true of the DEADLINE, not of the post-exit drain. **Remaining on native Windows**: the windowed launch and the focus-jump's compositor discovery, which degrade through their own "no adapter" path (they are Hyprland's), never a silent pretend. Five measured defects were found and fixed by these runs rather than by inspection: the attribute takes the console handle AS its value (the address leaves the child at `STATUS_DLL_INIT_FAILED`); the child's std streams are the CONSOLE's (`STARTF_USESTDHANDLES` with the three handles NULL and `bInheritHandles = FALSE` — the shape shipping ConPTY implementations use and the one the runs here prove, measured against two others: no `STARTF_USESTDHANDLES` at all loses the child's output to the pipe, and naming the console's pipe ends leaves the child with a redirected stdout (and, in the re-review's own measurement, no output at all)); the sizing call of `InitializeProcThreadAttributeList` FAILS by design, so testing its return refuses every spawn with `ERROR_INSUFFICIENT_BUFFER`; our own pipe ends close before `ClosePseudoConsole` (Microsoft's own deadlock warning); and the output pipe can never sit in the wait array — a synchronous anonymous pipe's read handle is always signalled, so it wins the wait forever and the inbox is never reported (`aoide send` typed into nothing until the pipe was polled instead). |
| argv / `comm` / `cwd` / ancestry identity | `conduct/src/graph/conduct.rs` (`cwd`, `cmdline`, `comm`, `/proc/<pid>/task/<pid>/children`), `client/src/tunnel.rs::looks_like_our_ssh`, `storage/src/attest.rs::pid_starttime`/`parent_pid`/`pid_ancestry`, `secrets/src/peercred.rs::read_comm` | required; **host-split**. The sealed-credential pid-reuse defence reads starttime and ppid: `/proc/<pid>/stat` on Unix, `GetProcessTimes` + one `Toolhelp32` snapshot on native Windows (`storage/src/attest.rs` → `protocol/src/win_proc.rs`), where the start time is the creation `FILETIME` — the same fact in another unit, re-derived fresh and never trusted from the record, so the defence is real on both hosts. **The sealed-identity lane now verifies on native Windows too**: its channel is an `AF_UNIX` socket on both hosts (`std`'s on Unix, `protocol/src/win_unix.rs`'s native binding on Windows), so `storage/src/attest.rs::connect_bounded`/`daemon_seal_pubkey_hex` are ONE body with no second arm — the socket TYPE is the seam, and the refusal that used to stand there (`#[cfg(not(unix))] -> None`) is deleted rather than kept beside it. Evidence: the native fake-daemon round trip and the full `attested_caller` resolution, both un-gated from Unix and green on ThinkChiyo |
| lock probe | `protocol/src/dialog.rs::probe_locker_running` | required; **host-split**: a `/proc` scan on Unix, one `Toolhelp32` snapshot on native Windows (`protocol/src/win_proc.rs`) matching the executable name exactly on Unix (`comm` is a byte string) and case-insensitively on native Windows (a file name there is), with Windows' own `.exe` suffix folded; an unreadable `/proc` — or a snapshot that cannot be taken — answers "not running". `probe_loginctl_locked` stays systemd-logind only, so the Windows gate reads the locker process alone |
| boot epoch | `conduct/src/reap.rs::boot_epoch` (`/proc/stat` `btime`, reused by `server/src/daemon.rs`) | required; **remaining** — off Linux `boot_epoch` is `None`, so the pre-boot reap signal and the boot-epoch-guarded auto-resume never fire |
| runtime dir | `storage/src/runtime_dir.rs::socket_dir` — the ONE authority, called by `storage/src/tunnel.rs`, `storage/src/attest.rs::daemon_socket_path`, `conduct/src/graph/conduct.rs::conduct_socket_path`/`channel_socket_path`, `conduct/src/shellbridge.rs`, `client/src/pair_watch.rs`, `server/src/daemon.rs`, and lyra's preview | required; **one seam, two answers, and a PURE resolution — a path, never a side effect** (`tunnel::list_records`' own tests depend on "the `aoide/` subdirectory does not exist until a writer makes it"). Unix: `$XDG_RUNTIME_DIR` when set to a non-empty value, else `/run/user/<this process's euid>`, then `aoide/` — the shipped literal `/run/user/1000` was the one hard-coded uid in the convention (right for uid 1000, wrong for everyone else) and is now derived; the directory is the session's own, so nothing here creates or chmods anything and one old spelling's empty-string bug (a relative `aoide/…`) is pinned by a test. `$XDG_RUNTIME_DIR` is the override on BOTH hosts — every isolating fixture sets exactly that variable, and an answer that ignored it on one host would read and write the machine's real per-user directory instead of the fixture's. Native Windows default: `%LOCALAPPDATA%\aoide` (this host has no XDG runtime dir; that is its per-user, non-roaming local store). The host provides neither that directory nor an owner-only policy on a new one — an elevated token's default owner is `BUILTIN\Administrators` — so a native socket binder creates it through `aoide_protocol::owner_only::ensure_private_dir`; the path itself is still unchecked against a `sun_path` budget here, while the native transport checks the budget on bind and connect. |
| the per-user home the root resolves against | `protocol/src/audit.rs::aoide_home`, consumed by `storage/src/fs.rs::default_root`/`flake_root`/`migrate_root_once`, `protocol/src/audit.rs::default_audit_log` and `cli/src/commands/onboard.rs` | required baseline; **host-split by the host's own variable, never by a POSIX shape**: Unix is `$AOIDE_USER` → `/home/<user>`, else `$HOME`, else the literal `/home/khoa` this chain shipped with; native Windows is `%USERPROFILE%`, else `%HOMEDRIVE%%HOMEPATH%`, else the temp dir — the same last-resort shape the runtime-dir row above takes. Windows sets neither input of the Unix chain (`$HOME` is not a variable it has, `$AOIDE_USER` is a Linux deployment variable the nix modules export), and honoring either there answers a path OFF THE CURRENT DRIVE: measured on ThinkChiyo, `aoide daemon` with `$HOME` cleared resolved `\home\khoa\.aoide\log` and CREATED `C:\home\khoa\.aoide`. A fabricated path is not the refusal convention's answer any more than a fabricated uid is; the host's own per-user directory is. Evidence: `audit::tests::the_native_home_is_the_hosts_own_per_user_directory_never_a_unix_shape` (run by name, beside that host's full `aoide-protocol` run). |
| unix socket addressing | `secrets/src/client.rs::unix_sockaddr` (Unix); `protocol/src/win_unix.rs` (native Windows) | required; capacity and field offset come from the target's `sockaddr_un`. Embedded NULs are refused; empty paths and unused fields follow Rust's `SocketAddr::from_pathname` convention, including leaving BSD `sun_len` zero. Linux boundary and real-socket tests pass; other platform layouts remain unverified at runtime. **Native Windows is a real `AF_UNIX` (Winsock) listener and stream, built once in `protocol/src/win_unix.rs`**: `socket(AF_UNIX)`/`bind`/`listen`/`accept`/`connect`, `WSAStartup` once, and the same `Read`/`Write`/`accept`/`connect`/`shutdown`/timeout surface the Unix callers use from `std::os::unix::net` — one code shape, so a socket path is a socket path on either host; the `sun_path` budget is checked on BIND and CONNECT and refused by name, and the peer pid comes from `WSAIoctl(SIO_AF_UNIX_GETPEERPID)`. **The socket FILE's own policy is the question this row has to answer for the bind site** (`secrets/src/broker.rs::bind_socket` pins the parent directory before the name exists and has no `chmod`-after-bind to mirror the Unix arm's): PROVEN — the parent directory is owner-only BY CONSTRUCTION there and reads back that way, and a bound socket reads back either as carrying the same owner-only policy or as the ONE refusal that object draws — it is a REPARSE POINT (tag `0x80000023`, `IO_REPARSE_TAG_AF_UNIX`, measured), which `owner_only`'s policy reader refuses by name — never as an object detectable as NOT owner-only (`win_unix::tests::a_bound_socket_adds_no_policy_wider_than_the_owner_only_directory`). NOT PROVEN — a cross-account connect REFUSAL: this host has no second account to connect from, and `runas /trustlevel:0x20000` is the SAME user (measured), so what gates a connect here is the directory's traversal right with the file's own policy adding nothing; that a different user is refused is a consequence of the directory policy, not a measurement. |
| shell resolution | `conduct/src/graph/resurrect.rs::passwd_login_shell` | required; **remaining** — a `getent passwd` shell-out (glibc/nss; its absence falls back to `/bin/sh`) |
| `ps` invocation | `conduct/src/graph/codex_app.rs::process_table` | required, **named dependency** — `ps -axo pid=,ppid=,command=` works on Linux/macOS/FreeBSD; POSIX.1 only mandates `ps -e -f -o <format>`, so a strict/`BusyBox` `ps` may reject it |
| stage/state lock | `storage/src/fs.rs`, `storage/src/outbox.rs`, `conduct/src/graph/codex_app.rs::lock_is_held` | required, **non-POSIX primitive by design** — `flock(2)` is BSD/XSI, not POSIX.1 (POSIX offers `fcntl(F_SETLK)`, per-process and dropped when any fd to the file closes). The guarantees here — the lock surviving a `fork`/`setsid` into a detached child, and `lock_is_held` as a liveness probe that never creates the file — rest on open-file-description semantics. Native Windows answers with `LockFileEx` on the same lock files (`LOCKFILE_FAIL_IMMEDIATELY` for the non-blocking probe). What it guarantees INSTEAD of surviving a `fork`: the lock belongs to the HANDLE, so a second handle in the same process blocks exactly as a second process's does — which is what `with_stage_lock`'s per-thread re-entrancy flag exists for — and `lock_is_held`'s "never creates the file" probe is unchanged. That port covers `storage`'s own lock files and their call sites only: `conduct/src/graph/codex_app.rs::lock_is_held`, named in this row's site list, is still `cfg(unix)` and has no Windows arm today (see "Not run" below) The one naming difference: Windows byte-range locks are mandatory where `flock` is advisory; nothing here ever reads or writes a lock file's bytes, so no caller can observe it. Evidence: native `fs_windows` lock tests on ThinkChiyo |
| private file / directory policy | `storage/src/fs.rs::atomic_write_private`/`write_temp_file`/`secure_private_dir`, `protocol/src/owner_only.rs` | required baseline, **policy by host, never silently weakened**. Unix: the temp is created AT `0o600` via `OpenOptions::mode`, the directory `chmod`ed `0o700`. Native Windows: a protected, single-ACE owner-only DACL for the current token user is attached AT CREATION (no create-then-tighten window), and any creation mode but `0o600` is refused by name. It is read back before the first payload byte by both of its consumers — the feed on the handle it just created, and `atomic_write_private`'s temp by path (`file_privacy`, which is also what refuses a reparse point at that path) — and a directory that is a symbolic link or a JUNCTION is refused by name rather than having the link's own policy written or read as the target's. Owner pinning in the tighten path (`set_dir_access`) is evidenced on a NON-admin token: `runas /trustlevel:0x20000` reports `BUILTIN\Administrators` as "Group used for deny only", and the tighten test passes there. A DIRECTORY is not a file here: its policy is the whole of `0o700`, which means `FILE_EXECUTE` (`FILE_TRAVERSE` — open what is inside by path) and `FILE_DELETE_CHILD` (the other half of `w`: on Unix removing an entry asks for write on the PARENT) on top of the file's bits, or the owner has made a directory they cannot list through. `secure_private_dir` creates-or-tightens and refuses a directory whose policy the filesystem would not honor, and a directory policy change is Windows' own re-propagation to children with merely inherited ACEs — a fact its test names. One implementation, exposed from `aoide-protocol` because `aoide-storage` needs the same policy — never a second copy. Exercise: `fs.rs`'s `assert_private_file`/`assert_private_dir` read the host's own policy; native tests on ThinkChiyo. |
| OS hostname | `storage/src/display.rs::os_hostname` | required; Unix asks `gethostname(2)`, native Windows `GetComputerNameExW(ComputerNameDnsHostname)` — the DNS hostname, not the NetBIOS name, with a length checked against the buffer it was given so a too-long name is refused rather than reported truncated. `None` on any failure on either host, never a fabricated name. Verified natively on ThinkChiyo by an ASSERTION rather than by execution: the row's test demands this host's own name (case-folded against `COMPUTERNAME`, suffix-tolerant) and rejects the shared `"aoide"` fallback, so a constant, a `None`, or a never-run arm cannot pass it. |
| append-only feed file (create policy + tail identity) | `protocol/src/feed.rs`, private `protocol/src/feed_windows.rs` | required baseline for the algorithm (append · cap · truncate-in-place · tail); **policy by host, never silently weakened**, `pkgs/aoide/crates/protocol/README.md`'s `feed` entry names the seams — and `protocol/src/owner_only.rs` is that policy factored out for its second consumer, `aoide-storage`'s private-write and private-directory half. Unix: `chmod` to the caller's exact `create_mode`, `(dev, ino)` identity — unchanged. Native Windows: owner-only policy attached at creation and read back before the first payload byte (a filesystem that ignores ACLs refuses), an existing file validated on its own handle before truncate/append, reparse points refused, identity by native 128-bit file id; **`create_mode` is ADVISORY there and the policy is always a protected owner-only DACL**: there is no group reader on native Windows (no lyra/desktop surface) and no gid to name, so the broker's group-shared `0o640` feed is delivered STRICTER than it asks — that same user, nobody else — rather than refused, and a same-user `watch::Follower` sees the line the broker writes (asserted natively). Compile-checked for `x86_64-pc-windows-gnu` and exercised natively on ThinkChiyo with MSVC; see the bounded runtime evidence below. |
| desktop / systemd capabilities in core | `hyprctl` window ops, `loginctl` lock gate, power actions, `notify-send`, `zenity`/`lyra` dialogs, `/run`+`/var/lib` deployment paths | optional host-specific — window ops gate on `HYPRLAND_INSTANCE_SIGNATURE` and degrade to `None`; power actions surface a spawn failure rather than a named refusal; paths are env-overridable placeholders, not POSIX shapes |
| the `ssh` client this box runs | `client/src/tunnel.rs::ssh_program` (the ONE place the binary is chosen), `local_login`/`resolve_login` (the login half); the argv read it is guarded by is `looks_like_our_ssh` → `process_argv` | required, **named difference per host** — Unix runs the bare name `ssh` through `PATH` (the operator's own client); native Windows runs the OpenSSH client that ships with the OS, `%SystemRoot%\System32\OpenSSH\ssh.exe`, by FULL PATH (there `ssh` is not a name `PATH` is guaranteed to carry, and OpenSSH installs outside any standard directory) — refused by name when `%SystemRoot%` is unset, never a guessed `C:\Windows`. The login chain differs for the same reason: `$USER` → `$LOGNAME` on Unix, and `%USERNAME%` last on Windows, where the other two are not variables the OS sets. The identity read differs too — `/proc/<pid>/cmdline` (NUL-split) versus `win_proc::command_argv` (`ProcessCommandLineInformation` + this host's own `CommandLineToArgvW`) — and argv[0] is `ssh` on one host and `ssh.exe` (folded, either separator) on the other. Evidence: a native `ssh_program` assertion against this host's own file, and the tunnel tests that run a REAL `ssh.exe` against a loopback banner listener. **Two differences that do NOT follow from choosing the system binary, stated rather than implied**: (1) the path is the NATIVE one, so a 32-bit (WOW64) build would be redirected to `SysWOW64`, where OpenSSH is not installed — only `x86_64-pc-windows-msvc` is targeted today, so nothing here builds that way yet, and a future i686 target would have to answer it; (2) Win32-OpenSSH is still OpenSSH — it reads `%USERPROFILE%\.ssh\config`, its `known_hosts` and the ssh-agent pipe exactly as any other build does, so `BatchMode=yes`, the `-L` spec and the `authorized_keys` teaching all behave as the Unix arm documents; only the BINARY's location differs |
| a node name this box gives itself | `storage/src/display.rs::local_node_name`, used by `storage/src/mail.rs` (`file_letter`/`file_receipt`/`mint_outbound_letter`/`mint_ack`/legacy migration), `storage/src/seal.rs` (the "is this container for me" and last-hop comparisons), `client/src/{letter_send,mail_wire}.rs` and `client/src/commands.rs`'s signed-header and pair-request names | required, **host-split by an OS fact rather than an API**: a node name in an address is grammar-lowercase (`^[a-z0-9][a-z0-9-]*$`) while an OS host name is case-PRESERVED and native Windows' DNS name is conventionally upper-case (`ThinkChiyo`, measured), so `local_host_name()` folded is the one name this box mints, declares and is looked up under. The raw name stays what a human is shown and what an ssh target spells. The limit is stated, not papered over: a host name carrying anything outside `[a-z0-9-]` still cannot BE a node name — refused by grammar, never mangled. NOT folded, deliberately: the mesh-charter and discovery self-name comparisons (`client/src/mesh.rs`, and the `is_self_target` callers in `client/src/commands.rs`) and `conductor`'s mailbox targets (`conductor/src/app.rs`, `conductor/src/lib.rs`'s `{local_host_name()}/{petname}`), each of which reads `local_host_name()` on BOTH sides — a peer-fed advertisement or an operator's charter names a box the same way this side does, so folding one side alone would break the comparison it exists for; the conductor sites are outside this slice's crates, and pre-existing on a host whose name differs in case. Evidence: `display::tests::local_node_name_is_the_local_host_name_folded` plus the mail/seal suites on ThinkChiyo, where the raw name is upper-case. **The scheme's own limit, measured where it will bite first**: a node name is an OS fact, so TWO nodes on ONE box compute the SAME name — native Windows' host name and the WSL instance's are the same computer name (`ThinkChiyo` raw, `thinkchiyo` folded, measured on both sides) — and every site that decides "is this for me" or "is this myself" by that name (the mail/seal container checks, the discovery self-target guard, the mesh drift comparison) cannot tell the two nodes apart. The escape already exists and is the whole chain's own input: `AOIDE_A2A_NODE_NAME` (read by `local_host_name`, therefore by the folded form too) set per node; it renames the RAW name with it, so the ssh target a `via` fallback spells is the override as well. **Exercised, not just named**: one box running both — native Windows `AOIDE_A2A_NODE_NAME=thinkchiyo-win` on port 8720, WSL left at its own `thinkchiyo` on 8710 — pairs both ways and exchanges mail, and the peer's name is what the wire addresses: a container is checked against the name the RECEIVING node knows itself by (`refused: addressing-mismatch: container is addressed to \`127-0-0-1\`, not this node`), so the URL-dialled side must be named what it calls itself at the ceremony (`--name`), never the sanitized host the URL would default to. |

## The seams this slice added, and what each one answers

One row per seam, because these are the primitives every other row's mappings
now go through — a mapping that does not name one of these is a mapping that
re-derives a host fact at its own call site, which is what the slice existed to
remove.

| seam | home | one answer per host, and why it is one seam |
| --- | --- | --- |
| `fs::path_is_under(path, root)` | `aoide-storage` | Is `cwd` inside a project root? A COMPONENT comparison (`Path::starts_with`), so this host's own separator rules decide, with a dunce-style strip of `canonicalize`'s `\\?\` verbatim prefix first. The defect it closes was a hard-coded `/`: `cwd.starts_with("{root}/")` matched nothing on native Windows, so the roster silently lost every `project:… → session:…` `anchors` edge there while every Linux run was green. Asked by `conduct`'s `model::cwd_under`. **Two limits stated, not hidden**: the comparison is case-SENSITIVE even on native Windows (nothing canonicalises a registered root, so a case-differing cwd/root pair does not anchor there), and a root of `/` now matches every absolute path where the old string prefix would not have — the documented "absolute path prefix" meaning, and reachable only if an operator registers `/` as a root. |
| `fs::looks_absolute_any_host(path)` | `aoide-storage` | Is this path absolute in SOME host's grammar? A remote node's host root is validated by the grammar of the node it names, never by `Path::is_absolute()` (which on Windows says no to `/srv/x` and on Unix says no to `C:\x`). Accepts a leading `/`, a drive-qualified `X:\`/`X:/`/bare `X:`, and `\\`-rooted UNC; `X:foo` is drive-RELATIVE and refused. Asked by `conduct`'s `manage::validate_host_root`. |
| `fs::link_dir(source, link)` | `aoide-storage` | Point a link at a directory. Unix's `symlink(2)` is kind-agnostic; Windows has two calls and asks the creator, so this host gets `symlink_dir`. A refused creation (no `SeCreateSymbolicLinkPrivilege`) surfaces its own error. Asked by `conduct`'s `hooks install` and `cli`'s `onboard`. |
| `fs::detach(&mut Command)` | `aoide-storage` | The detached-spawn posture: `setsid(2)` in a `pre_exec` hook, or `DETACHED_PROCESS \| CREATE_NEW_PROCESS_GROUP` as a property of the spawn call. Asked by `conduct`'s `spawn_detached` and `a2a`'s handler spawn — the same pair in both, never two copies. |
| `fs::create_new_private(path)` | `aoide-storage` | A write-once private file: `mode(0o600)` at creation on Unix; `owner_only::create_new` (policy attached by the creating call, read back before the first byte) on Windows, with `AlreadyExists` preserved. Asked by `conduct`'s instruction sidecar. |
| `fs::{lock_exclusive, try_lock_exclusive, unlock}` | `aoide-storage` | The file lock, `flock` or `LockFileEx`. Widened from `pub(crate)` so `conduct`'s own probe (`codex_app::lock_is_held`) asks it rather than carrying a second `flock` call — that crate's `#[cfg(not(unix))] → None` refusal is deleted, not kept beside the seam. |
| `win_unix::{UnixStream, UnixListener}` | `aoide-protocol` | The socket TYPE is the seam: every caller writes `use`-pairs keyed on `cfg(unix)`/`cfg(windows)` and no call site spells a platform. This slice added the missing surface the call sites needed: `set_nonblocking` on the listener (not only the stream), `try_clone` on the listener, `write_timeout` (the read-back half of `set_write_timeout`), and `impl AsRef<Path>` on `connect`/`bind` so an argument moves between hosts untouched. **What a LISTENER's mode propagates, measured the hard way**: Winsock's `accept` hands back a socket that INHERITS the listening socket's own properties — `FIONBIO`, and any asynchronous-event association — where `std`'s Unix `accept` does not; a listener armed non-blocking for a deadline accept loop (the delivery fixtures and conduct's injection inbox; the four production doors leave their listeners BLOCKING) therefore produced non-blocking CONNECTIONS on that host alone, and `set_read_timeout` (`SO_RCVTIMEO`), which a non-blocking socket ignores, answered `WSAEWOULDBLOCK` at once. The accept arm now clears the inherited flag best-effort, and the GUARANTEE is the deadline-bound read/write itself rather than that call: bounded whatever the socket's mode (`win_unix::tests::an_accepted_socket_is_blocking_even_when_the_listener_is_not`, `win_unix::tests::an_accepted_stream_with_no_budget_blocks_until_the_peer_writes`). |
| `win_proc::{processes, parent_chain, command_argv, process_user_sid, current_user_sid}` | `aoide-protocol` | The process-table facts — parent, argv, exe name, token user — that Unix reads out of `/proc`. `conduct`'s `proc_argv`, `proc_comm` and `proc_has_children` answer through it; `proc_cwd` is the one fact with NO arm (a working directory needs the target's PEB) and says so by name, falling back to the record's stamped `cwd`. |
| `test_support::{short_tmp, built_aoide_bin}` | `aoide-test-support` | Two fixture facts that are HOST facts. A socket path must fit `sun_path` (107 bytes on native Windows, against a ~36-byte temp prefix), so the tag is hashed rather than spelled; and a built binary's name carries this host's `EXE_SUFFIX`. Both were wrong in three separate copies before this slice — the bare-name one made the cli door fixture never launch a hook at all, which silently left the daemon's peer-pid verification UNEXERCISED on that host. |

### What the daemon's peer-pid verification answers on native Windows

The security property is live there, not skipped: the accept loop reads the
connecting pid through `WSAIoctl(SIO_AF_UNIX_GETPEERPID)`
(`win_unix::UnixStream::peer_pid`), stamps it as the door's own peer pid, and
resolves the peer's user from THAT pid's token
(`win_proc::process_user_sid`, which re-reads the process's creation time around
the token read and refuses a pid reused between the two). A hook's
`AOIDE_SESSION_ID` claim is then verified against the real ancestry of that pid
(`win_proc::parent_chain`), so a contradicted claim is dropped exactly as it is
on Linux. Evidence: `aoide-cli`'s `daemon_dispatch_door` — 8 passed / 0 failed
on ThinkChiyo, the two tests that guard this being
`a_daemon_served_session_hook_links_the_hook_processs_own_parent` and
`a_daemon_served_hook_drops_a_contradicted_parent_claim`.

### Where a reader could ever wait forever

Every production `accept()` in the closure is either a server's own loop
(`shellbridge`, `a2a`, `daemon`, `mcp` — a door waiting for its next client by
design) or sits on a NON-blocking listener with EAGAIN handling
(`conduct.rs`'s injection door); the production connect side a fixture's reader
waits on is bounded (`doorbell::connect_for_ring`'s 2 s write timeout). Every
`accept()` that waits for a peer that may never come is in a `#[cfg(test)]`
fixture — where an unparseable fixture (a Windows path pasted into hand-built
JSON, which this slice fixed in `pingback`, `trace` and `send`) or a skipped
delivery turns into a hang rather than a failure, which is why those fixtures
now build their JSON with `serde_json`.

## Evidence and limits

- **Run**: Linux tests on `x86_64-unknown-linux-gnu` for every crate in the
  closure — 177 (protocol) + 486 (storage) + 449 (+7 e2e) (secrets) + 31
  (upkeep) + 339 (client) + 272 (server) + 75 (cli) + 975 (conduct) + 191
  (conductor) + lyra's targets, every one 0 failed except the two named under
  "Evidence and limits", and `cargo test
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
- **Run**: the WHOLE core closure — `aoide-protocol`, `aoide-storage`,
  `aoide-secrets`, `aoide-upkeep`, `aoide-client`, `aoide-conduct`,
  `aoide-server` and `aoide-cli` — builds on ThinkChiyo with native
  `x86_64-pc-windows-msvc` (Rust 1.98.1): `cargo check -p aoide-conduct
  -p aoide-server -p aoide-cli --all-targets` is **0 errors and 0 warnings**,
  and `cargo build --bin aoide` produces the `aoide` binary there, and the
  conducted session RUNS there (the ConPTY arm's four native tests, plus the
  mechanism probe its wait stands on). Runtime, per
  crate on that host: **931** (conduct, 0 failed, 0 ignored) + **266** (server lib, 0
  failed, 5 ignored with reasons) + 42/8/6 across cli's targets (0 failed), on
  top of the earlier crates' 199 (protocol) + 488/489 (storage — one
  PRE-EXISTING failure, below) + 366 (secrets) + 32 (upkeep) + 314 (client)
  passed. The same tree on `x86_64-unknown-linux-gnu` is 177 + 486 + 449 + 31 +
  339 + 975 + 272 + 191 (conductor) + lyra's targets, also 0 failed — with
  lyra's own two `preview_tools` failures named under "Evidence and limits".
  **Where a native count is
  lower, the gates are in-file with their reasons** — this file's seam table and
  the PTY row name each one, and every count that moved during this slice
  moved only by the tests it added (storage +1 function, no test; conduct +5
  native-only on Windows and +1 on Linux, none of them a count of an existing
  test).
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
- **Run**: the daemon itself, on that host, launched from a scratch
  `AOIDE_ROOT` (`C:\Users\dxcen\aoide-w7-probe`) with `XDG_RUNTIME_DIR`,
  `AOIDE_STAGE_DIR`, `AOIDE_STATE_DIR` and `AOIDE_AUDIT_LOG` all under it and
  non-default door ports (`AOIDE_A2A_PORT=18710`,
  `AOIDE_MAIL_ADAPTER_PORT=18712`), at `ea257fc`.
  `aoided.exe` started, bound `<scratch>/rt/aoide/aoided.sock`, wrote its feed
  and its audit log, and was alive past 60 s with an empty stderr. That socket
  served real CLI invocations, not just the unit tests': `aoide context --id
  …` answered "aoided must be running to fetch agent context" with no daemon
  and the daemon door's OWN audit record (`"door":"daemon","command":"context"`)
  with one, and `aoide session reap` likewise carried a `daemon`-door record.
  `aoide conduct --headless -- cmd.exe` registered on the roster
  (`presence online`), `aoide send --id … --yes --submit -- "echo W7_INJECTED"`
  delivered into the child's own console transcript (the echo ran, its output
  appears in the per-session log the seam mirrors), an externally killed child
  was noticed by the wrapper (`cmd exited -1` — `TerminateProcess`'s own code,
  read through the seam) and a record whose pid had died was reaped
  (`reaped dead session w7c (killed; running → done → dropped)`). The doors
  run as their own processes on both hosts — the daemon spawns NONE of them
  (its only child in that run was its own `conhost`) — and each answered from
  that host: `aoide a2a serve` bound `127.0.0.1:18710` (`GET
  /.well-known/agent-card.json` 200; a JSON-RPC `tasks/get` for an unknown id
  `-32001 task not found`), `aoide mail serve` bound `127.0.0.1:18712`
  (`aoide/mailPoll` without its node `-32602`), and `aoide mcp serve --stdio`
  answered `initialize` (`aoide v0.0.26`) and `tools/list` (110 tools) over its
  newline-delimited door.
- **Not run, and it is a residency fact rather than a code one**: nothing on
  that host keeps a daemon up across sessions. A daemon started there by
  `Start-Process` inside an ssh session was alive at 60 s and GONE at the next
  session — empty stderr, no stop record, socket file left behind — so an
  ssh-launched daemon is not a resident one; the host's own logon/service
  mechanism is unmeasured and nothing in the tree implements one.

## Next layer

Measured on ThinkChiyo (Rust 1.98.1/MSVC) after this slice, with the whole core
closure compiling there:

| crate | config | errors |
| --- | --- | --- |
| `aoide-protocol`, `aoide-storage`, `aoide-secrets`, `aoide-upkeep`, `aoide-client` | `--all-targets` | **0** (the five W2/W3 crates) |
| `aoide-conduct` | `--all-targets` | **0** (was 108 lib + 228 lib test) |
| `aoide-server` | `--all-targets` | **0** (was 11 lib + 26 lib test) |
| `aoide-cli` | `--all-targets` | **0** (first measurement of this crate) |
| `aoide` binary | `cargo build --bin aoide` | **builds** (`target\debug\aoide.exe`) |

Runtime, native, measured the same day:

| crate | run | passed / failed / ignored |
| --- | --- | --- |
| `aoide-conduct` | `cargo test` | **931 / 0 / 0** |
| `aoide-server` | `cargo test --lib` | **266 / 0 / 5** |
| `aoide-cli` | `cargo test` | 42 (lib) · 8 (`daemon_dispatch_door`) · 6 (`graph_residency_p_d6`, single-threaded) — **0 failed** |
| `aoide-storage` | `cargo test` | 488 / **1** / 0 — the one failure is pre-existing (below) |
| `aoide-client` | `cargo test` | **314 / 0 / 0** |

The five `ignored` on `aoide-server` are two proven host facts, not omissions:
three `run_boot_auto_resume_*` tests (this file's own boot-epoch row: `boot_epoch`
is `None` off Linux) and one a2a spawn test whose probe needs `/bin/sh` +
`printf`'s byte-exact output. One further test is ignored on the **cli** side for
the same class of reason: the door fixture there stores through the built-in
`file` backend, whose template is the POSIX preset `cat …` — refused BY NAME on
native Windows, with native cover in `aoide-secrets`' own `cmd /C` template
tests.

**The PTY capability is native on both hosts, and it was the last named gap.**
The seam is `conduct/src/graph/pty.rs`: `spawn_on_pty` (`libc::openpty` +
`setsid` + `TIOCSCTTY` + `dup2` on Unix; `CreatePseudoConsole` with the
`PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` attribute on `CreateProcessW` on native
Windows), plus `Pty`, `PtyChild`, `Console`, `Inbox` and `wait_ready` — one
contract, an arm per host, with the host differences named in that module's own
table rather than discovered at each call site. `aoide conduct` runs there
(headless: registered, its output mirrored into the per-session log, resolved
`done` with the child's real status), and four native runs on ThinkChiyo say so
by name — the output read, a line typed into the child's own stdin, a resize
the child's own `mode con` reports, and the whole command end to end — beside
the mechanism probe that an `AF_UNIX` connection signals through
`WSAEventSelect`/`WaitForMultipleObjects`. `conduct/src/graph/spawn.rs`'s
detached, NON-PTY spawn stays host-split through `aoide_storage::fs::detach`
(which `a2a`'s handler spawn shares) and is native with its own four tests. What
remains host-specific on Windows is named in the row above and in `pty.rs`:
no signals, no process group, no foreground pgid, no "a descendant still holds
the console", and a post-exit settle window a pty does not need — the WINDOWED
launch and the compositor's focus discovery are their own degradations, since
they are Hyprland's, not this capability's.

**One measured limit worth stating**: `aoide-cli`'s
`graph_residency_p_d6` binary passes single-threaded (6/0/1) and passes each test
alone, but a fully PARALLEL run of that binary left one test red — six resident
daemon children on one box, against the daemon hop's own 2 s reply bound. The
tests already serialize on `aoide_test_support::env_lock`, so this is contention,
not a code defect; it is recorded rather than papered over.

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
- **One native failure is PRE-EXISTING, and this tree does not cause it:**
  `aoide-storage`'s
  `fs::tests::tightening_a_directory_strips_a_child_that_only_inherited_its_access`
  fails deterministically on ThinkChiyo (three runs, `0 passed; 1 failed`), and
  it fails identically with the mirror checked out at `724fe0f` itself: the
  child file the host wrote keeps its inherited ACEs instead of being stripped
  by the parent's tightened, protected policy. The "storage 488 / 0" this page
  carried from the previous slice therefore does not reproduce on that box; the
  test is named here rather than the number quietly kept.
- **One `yomi` failure is PRE-EXISTING too:** `aoide-lyra`'s
  `commands::preview_tools::tests::session_menu_qml_top_level_children_match_the_real_checkout`
  and
  `commands::preview_tools::tests::file_matched_node_children_resolve_positionally_against_the_real_session_menu_children`
  compare hardcoded line numbers against the LIVE song (`flake_root()` is
  `~/.aoide`'s `Aoide` checkout): they expect the top-level children at lines
  67/134/165/179/186/187 and the file on disk has them at
  138/205/236/250/257/258 — with that file byte-identical to this worktree's own
  copy and unchanged since before this slice began. No `lyra` file is in this
  slice's diff; those expectations belong to the lane that owns the song.
- **Refusal convention**: where a capability is host-specific, the shape is
  the named refusal — `cfg(target_os)` on the existing body plus a non-host arm
  that keeps today's fail-closed semantics — never a fabricated uid/pid, never
  a weaker check standing in for a stronger one, and never a parallel
  discovery mechanism beside the portable path.
