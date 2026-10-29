# AGENTS.md: aoide-desktop-exec

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The forbid rule

- This crate's lints table allows `unsafe_code`, `disallowed_methods` and
  `disallowed_types`. Every other member takes the workspace table, which
  forbids the three, so this is the one crate where `unsafe`, a process start
  or a signal can appear.
- Every file this crate compiles lies inside that boundary. The gate holds the
  crate to one source file, `src/lib.rs`: cargo makes a target of any `.rs`
  file under `src/bin`, `tests`, `examples` or `benches`, and this table allows
  the three lints in each (`the_exec_crate_has_one_source_file`). A `#[path]`
  module or an `include!` would compile another file under this table, so the
  gate also reads the dep-info rustc writes for this crate and refuses any file
  in it but `src/lib.rs`, the root's `clippy.toml` and the two manifests, and
  it refuses any dep-info line that is not a target, a `<file>:` or an
  `# env-dep:` line (`the_exec_crate_compiles_only_its_lib_rs`). The read sees
  only the dep-info files beside the test binary: a build script or a proc
  macro of a dependency, a linked library, and code that only a build the gate
  does not make (a build without `fakes`) are outside it. A second file in this
  crate changes both tests, in the same commit.

## The API

- The door helpers are `pub` for the shell's door code: `euid`, `open_pidfd`,
  `terminate_pidfd`, `pidfd_exited`, `stat_of`, `spawn_door`, and the `Arrived`
  and `Stat` they return.
- `terminate` stays `pub(crate)`, and `run` stays private. `run` is reached
  only through `Exec`, and `terminate` only through `run` and
  `test_doors::terminate`.

## The spawn and the calls

- `spawn_door` starts only the `Pinned` program it is given, at its canonical
  path. Its pre-exec closure makes only `setsid`, `fork`, `write`, `kill` and
  `_exit`, which POSIX lists as async-signal-safe, and allocates nothing. `fork`
  runs glibc's atfork handlers, which take malloc's locks before the fork and
  release them on both sides. The closure runs in the intermediate, which has
  one thread, so those locks are free. `spawn_detached`'s doc says the same. If
  the write of the pid fails, the closure sends SIGTERM to that pid. The door is
  the intermediate's unreaped child then, so the pid is its own. That is the one
  signal not sent through a pidfd.
- `spawn_door` returns the door as it arrived. `spawn_detached` opens the pidfd
  and reads the start time as the intermediate's pid comes off the pipe, before
  it reaps the intermediate. The window that is left runs from the write to that
  read: a door that exits and is reaped by init inside it can lose its pid to
  another process. A launch that then refuses SIGTERMs that process through the
  pidfd. No test reaches that window.
- Between fork and exec, in order: std's `pre_exec` makes std fork, and the
  child runs its atfork handlers; the closure then calls `setsid` and
  `libc::fork`, still in that one-threaded child; the grandchild returns from
  the closure and execs the door. libc 0.2.189, the version the desktop lock
  binds, has no `_Fork`, so the second fork is `libc::fork`, which is safe here
  because the child has one thread when the closure runs.
- The door's environment is `Exec::door_env`: `child_env(Child::Eidolon, "web",
  ...)`, with `XDG_RUNTIME_DIR` replaced by the checked runtime dir.
- Every child runs from a `Pinned` program. `check_pin` is the only maker of
  one, and `run` and `spawn_door` take one. `run` is private, and only `Exec`
  calls it. Do not add a second way in.
- `check_pin` checks the target and every directory above it, up to `/`: each
  is owned by root or this user, and none is writable by group or other, unless
  it is a directory with its sticky bit set. The target also has no setuid or
  setgid bit (`PinError::Owner`, `PinError::Writable`, `PinError::SetId`). The
  tests' scratch dirs lie under `TMPDIR`, so `TMPDIR`'s ancestors must pass the
  same rule. No test can make a directory another uid owns, so the owner branch
  is not tested (see Not tested).
- No shell, ever. The argv is a vector of words, never a string handed to a
  shell. A child runs by its absolute path with `env_clear()`.
- The argv is `[aoide, <path words>, --json, <flags>, (-- <positionals>)]`. A
  valued flag is `--name=value`. A bool flag is written bare, because aoide's
  parser never lets a declared bool take the next word. A bare `--` ends flag
  parsing, so `--` is written only when there are positionals.
- A call that does not match the schema is refused before anything runs: an
  unknown path, an undeclared flag, a bool given a value, a valued flag given
  none, a missing required argument, or a NUL byte in any word.
- An `eidolon` child loses `AOIDE_SESSION_ID`, every `AOIDE_MELETE_*` variable
  and every vault key variable. An `aoide` child loses every `AOIDE_MELETE_*`
  variable unless its first word is `melete`, and every vault key variable.
  Names are compared as bytes, so a name that is not UTF-8 is scrubbed too.
- The vault key names come from `aoide config --json`. While they are unknown,
  every call but `schema` and `config` is refused before a child starts, and an
  `eidolon` launch is refused too. The names are unknown when the config call
  fails or does not parse, or when a `tokenEnv` is not
  `[A-Za-z_][A-Za-z0-9_]*`, or names `PATH`, `HOME`, `XDG_RUNTIME_DIR` or
  `AOIDE_ROOT`. A bad `tokenEnv` is shown through `visible`. The pattern has no
  length bound and is case-sensitive, so a 10 KiB name and `path` pass, and
  `HOME=x` does not.
- Each stream is kept up to 16 MiB. Past that, the child still runs to its end,
  the extra bytes are dropped, and the call is refused.
- Each call runs under one deadline, 30 s, from the spawn to the end of the
  reading of its output. `Exec::deadline` can only shorten it: the shorter of
  the two is taken. A child still running at the deadline is sent SIGTERM and
  has 5 s to exit, and the call is refused as a timeout. A child that has
  exited, while a process it started still holds its output pipe, is refused at
  the same deadline, and no signal is sent to that process. That process keeps
  the call's two reader threads and their pipes until it exits; freeing them at
  the deadline is not implemented. Nothing sends SIGKILL. The word is barred
  from every source; in door and shell `Child::kill` is a clippy disallowed
  method, and this crate, whose table allows it, calls it nowhere, its tests
  included.
- `open_pidfd` opens a pidfd for a pid, and `terminate_pidfd` sends SIGTERM
  through one, and only SIGTERM. Both refuse pid 0, pid 1, any pid past
  `i32::MAX`, and this process's own pid. A pidfd names one process, so the
  signal cannot reach a process that later takes the pid. Outside this crate's
  tests, `arrive` and the shell's door code in `doors.rs` are the only callers
  of `open_pidfd`: a door is signalled through its pidfd, except for the pid
  signal in the spawn closure.
- `pidfd_exited` polls a pidfd with a zero timeout: it reports whether the
  process has ended, and never waits. Only the watch thread uses it.
- `terminate(pid)` sends SIGTERM to an unreaped child this process started, and
  refuses the same pids. Its one production caller is `run`, on the child it has
  not reaped, so the pid is that child's. The door tests reach it through
  `test_doors::terminate`.
- A non-zero exit is not an error. `call` returns the child's status and its
  stderr, verbatim, for the caller to show.

## The fakes

- `make_fifo` makes the FIFO that the shell's `tests/token.rs` puts at the
  token path. The door crate forbids `unsafe`, so the FIFO is made here.
- The door tests start, copy and signal programs through `test_doors`, behind
  `fakes`. It is the one way the tests reach `spawn_door`, `copy_program` and
  `terminate`.
- `block_sigterm` and `wait_sigterm` are the SIGTERM pair the fake eidolon
  uses. The fake blocks SIGTERM before any thread starts and waits for it on
  each pass, so the bin holds no libc call and no `unsafe`.
- The two hostile children in this crate's tests are shell scripts that the
  test writes, pins and runs through `run`: a grandchild that holds stdout, and
  a child that ignores SIGTERM. Each is bounded and exits by itself within 8 s.
- A test that runs a program it wrote writes it through a child: `write_program`
  runs `/bin/sh` to write the file. A concurrent test's fork would hold this
  process's writer open until its own exec, and the program's exec would then
  fail with ETXTBSY. The pid a program writes is polled for up to 5 s, since the
  call that runs the program can be refused first.

## Residuals

- A same-uid process can re-point a path it owns between `check_pin` and the
  exec. The pin is checked before the exec, not at it (ruling 29).
- A child that ignores SIGTERM for 5 s is left running and unreaped. The call
  is refused all the same.

## Not tested

- The intermediate's failed pid write, which SIGTERMs the door and exits
  non-zero: no seam makes that write fail.
- `check_pin`'s owner branch, which refuses a target or directory owned by a
  uid that is neither root nor this user: no test can make a directory another
  uid owns. The mode rule is tested.

## Extension points

- A new kind of child goes in `src/lib.rs`, runs only a `Pinned` program, and
  has its own environment rule in `child_env`. If it needs vault keys, it is
  refused while their names are unknown.
- A stable std API that starts a process or sends a signal goes into
  `clippy.toml` when the toolchain gains it. `ChildExt::send_signal` and
  `PidFd` are unstable on rustc 1.98.1, so until then a signal goes through
  `libc` here.
