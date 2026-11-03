# ground

The host seams that name neither product: the local socket, peer credentials,
OS randomness and the stored-command shell, and, on Windows, the process table,
the `AF_UNIX` socket and the owner-only policy. The crate is a leaf. It names
neither product, and it depends on no workspace crate: the `boundary` check
holds both lines. `aoide-protocol` links it and re-exports every module at its
old path (`pub use ground::<module>`), so `aoide_protocol::peercred::peer_cred`
names the same item it always did.

Code on either side of the product line can use a host seam here without
linking the other side. The re-exports keep every old path valid until the
importers name `ground` themselves.

## Seams

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

`libc` on Unix (`fcntl`, `socket`, `getsockopt`, `geteuid`) and `windows-sys` on
Windows. No workspace crate: a `dep` row from a neutral crate to either side is
a failure of the `boundary` check.

## How it composes

```
 aoide-protocol
 pub use ground::<module>
      |
      |        +----------------------------------+
      +------> | ground                           |
               |                                  |
               |  host_shell  host_random         |   libc        [unix]
               |  local_socket  peercred          |   windows-sys [windows]
               |  [windows] owner_only            |
               |            win_proc  win_unix    |
               +----------------------------------+
```

- A module keeps its name when it moved whole, so `aoide_protocol::peercred`
  and `ground::peercred` are one module.
- On Windows, `local_socket` and `peercred` reach `win_unix` and `win_proc`.
  `win_proc` reads the token user through `owner_only`, and `win_unix::pair_in`
  makes its rendezvous directory with `owner_only::ensure_private_dir`.
