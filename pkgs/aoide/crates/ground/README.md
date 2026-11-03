# ground

The host seams that name neither product: the local socket, peer credentials,
OS randomness and the stored-command shell, the git probes, time formatting,
the host name, the runtime root and the SHA-256 and hex helpers, and, on
Windows, the process table, the `AF_UNIX` socket and the owner-only policy.
The crate is a leaf. No code line in it names either product, and it depends
on no workspace crate: the `boundary` check holds both lines. `aoide-protocol`
and `aoide-storage` link it and re-export every module at its old path
(`pub use ground::<module>`), so `aoide_protocol::peercred::peer_cred` and
`aoide_storage::git::common_dir` name the same items they always did.

Code on either side of the product line can use a host seam here without
linking the other side. The re-exports keep every old path valid until the
importers name `ground` themselves.

## Seams

- `git.rs`: the git seam. `installed` (`git --version`), `toplevel` (the root
  of the working tree a path sits in) and `status_under` (a path's uncommitted
  lines, `status --porcelain`, read-only) run `git`. `common_dir` is the one
  pure read: the git common directory of the repo a path sits in, found by
  walking up to the nearest `.git` and following a file's `gitdir:` through its
  `commondir`. Every linked worktree of one repo answers the same path, and no
  `git` process runs, so it answers with `git` absent from `PATH`.
- `hash.rs`: `sha256`, and `hex_encode` and `hex_decode` for lowercase hex
  with no separator.
- `host_name.rs`: `os_hostname`, this host's name as the OS reports it:
  `gethostname(2)` on Unix, the DNS host name from `GetComputerNameExW` on
  Windows (never the truncated NetBIOS name). `None` on any failure, never a
  panic.
- `host_random.rs`: `fill`, OS randomness with one contract per host:
  `/dev/urandom` on Unix, CNG's `BCryptGenRandom` on native Windows. No pool,
  no seed, no fallback: a host that cannot hand out OS randomness fails rather
  than returning something weaker. Its one consumer is the secrets crate's
  enrollment.
- `host_shell.rs`: `command`, the ONE place a stored command LINE is handed to
  this host's own interpreter: `sh -c` on Unix, `cmd /C` on native Windows with
  the line handed over VERBATIM (`CommandExt::raw_arg`: `cmd` is not a
  `CommandLineToArgvW` program and re-applies its own quote rules to `/C`'s
  remainder, so MSVC-shaped quoting mangles a line carrying inner quotes). Two
  consumers, one implementation: the secrets crate's backend templates and the
  upkeep crate's `verify_command`.
- `local_socket.rs`: `connect_bounded`, a unix connect that cannot hang: a full
  listen backlog, which makes a bare `UnixStream::connect` wait forever, times
  out instead. The socket is close-on-exec, as `UnixStream::connect` makes it.
  On Windows it is `win_unix`'s `connect_timeout`.
- `peercred.rs`: the kernel-truth identity of a connected peer: `peer_cred`
  (`SO_PEERCRED` on Linux and Android, the peer's token user on native
  Windows, `None` elsewhere), the `PeerCred` it returns, the `PeerUser` a gate
  compares (a uid on Unix, a SID on Windows, never one derived from the other),
  and `own_user`, this process's side of the comparison.
- `runtime_dir.rs`: `runtime_root`, the per-user runtime root:
  `$XDG_RUNTIME_DIR` when it is set and not empty, else this host's own answer
  (`/run/user/<euid>` on Unix, `%LOCALAPPDATA%` on native Windows, the temp dir
  when that variable is absent). Each owner appends its own segment.
- `time.rs`: ISO-8601 UTC time as `YYYY-MM-DDTHH:MM:SSZ`, hand-rolled so the
  offline lock never grows a time crate: `now_iso_utc`, `iso_utc_from_epoch`,
  `shift_iso_utc` and `parse_iso_utc`, the exact inverse of the formatters.
- `owner_only.rs` (Windows only): the owner-only policy a native-Windows core
  attaches to a private file or directory: an explicit, `SE_DACL_PROTECTED`,
  single-ACE DACL for the current token user, attached AT creation and read
  back before the first payload byte BY BOTH CONSUMERS (the feed writer on the
  handle it just created, the storage crate's private temp by path), with a
  reparse point refused as itself and a `chmod` stand-in (`set_dir_access`,
  owner pinned) plus one idempotent `ensure_private_dir` that refuses a
  symbolic link or junction by name. Two consumers, one implementation: the
  feed writer and the storage layer's private-write and private-directory half.
- `win_proc.rs` (Windows only): the process table `/proc` would answer, plus
  the two acts `kill` and `waitpid` perform and the one fact the table does not
  carry: one `Toolhelp32` snapshot (pid, parent, executable name), a
  creation-time start time for the pid-reuse defence, the `kill(pid, 0)`
  liveness reading with the same conservative verdicts, `terminate`
  (`TerminateProcess`, the only primitive that host has), `wait_for_exit`
  (`WaitForSingleObject`, whose three verdicts map onto `waitpid`'s), and
  `command_argv` (`NtQueryInformationProcess(ProcessCommandLineInformation)`
  and this host's own `CommandLineToArgvW`, for callers that must know WHAT a
  pid is running before signalling it). Its callers are the storage layer's
  attestation walk and process acts, the client's recycled-pid guard and the
  protocol crate's dialog locker probe.
- `win_unix.rs` (Windows only): the local socket, natively: `AF_UNIX`
  (Winsock, Windows 10 1803+) behind the surface `std::os::unix::net` gives
  Unix callers (`UnixStream`, `UnixListener`, `Incoming`, `SocketAddr`), with
  a bounded connect, read and write deadlines enforced by this module,
  `try_clone`, `pair` and `peer_pid`. It owns the socket, not the policy: who
  may connect is the socket directory's `owner_only` policy.

## What it consumes

`libc` on Unix (`fcntl`, `socket`, `getsockopt`, `geteuid`, `gethostname`, and
the `O_NOFOLLOW` and `O_NONBLOCK` flags of the git file open), `windows-sys` on
Windows, and `sha2` for `hash`. No workspace crate: a `dep` row from a neutral
crate to either side is a failure of the `boundary` check.

## How it composes

```
 aoide-protocol                                          aoide-storage
 pub use ground::<module>                  pub use ground::<module> / <item>
      |                                                        |
      |        +----------------------------------+            |
      +------> | ground                           | <----------+
               |                                  |
               |  host_shell  host_random         |   libc      [unix]
               |  local_socket  peercred          |   windows-sys [windows]
               |  git  time  host_name            |   sha2
               |  runtime_dir  hash               |
               |  [windows] owner_only            |
               |            win_proc  win_unix    |
               +----------------------------------+
```

- A module keeps its name when it moved whole, so `aoide_protocol::peercred`
  and `ground::peercred` are one module. `host_name`, `runtime_dir` and `hash`
  hold the stateless halves of three storage modules; storage keeps the rest of
  each and re-exports the moved item beside it (`display::os_hostname`,
  `runtime_dir::runtime_root`, `seal::{hex_encode, sha256}`).
- On Windows, `local_socket` and `peercred` reach `win_unix` and `win_proc`.
  `win_proc` reads the token user through `owner_only`, and `win_unix::pair_in`
  makes its rendezvous directory with `owner_only::ensure_private_dir`.
- `git` is a root module of this crate, so its tests keep the path
  `git::tests::...`.
