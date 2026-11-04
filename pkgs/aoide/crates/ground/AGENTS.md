# AGENTS.md: ground

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## Invariants

- **It names neither product and depends on no workspace crate.** No code line,
  fixture, temp path, test-only environment variable or manifest line here
  names either product, and no `Cargo.toml` entry here points at a crate of
  either side: the `boundary` check counts a line matching
  `/aoide|strillix|\bstrx\b/i` in any scanned file of this directory and every
  edge from this crate to a crate on either side, `[dev-dependencies]`
  included. Comments and `*.md` files are not counted, so keep them neutral by
  review: say "the daemon" or "the secrets crate", not a product's name or an
  `aoide_*` path. The README and this file are the exception: they name the
  crates that link ground (`aoide-protocol`, `aoide-storage`) and the one a
  test must not use (`aoide-test-support`). A fixture takes `ground` for the
  product word (`ground-peercred-{pid}`, `GROUND_COMMON_DIR_WT`). A test helper
  is written in this crate and never taken from `aoide-test-support`.
- **A moved item is one item at both paths.** `aoide-protocol` and
  `aoide-storage` re-export a module (`pub use ground::<module>`) or an item
  (`pub use ground::host_name::os_hostname`), under the `cfg` the module has
  here. A consumer never gets a copy (`pkgs/aoide/crates/AGENTS.md`: no
  cross-crate copying). A signature change here changes both old paths, so
  check the whole workspace (`cargo check --workspace --all-targets`).
- **The write and lock family has no unguarded form.** Each `fs` item that
  makes a directory for a write, a copy or a lock file takes the directory
  step, `make_dir: fn(&Path) -> io::Result<()>`, and calls it exactly where it
  makes that directory, on that directory. In a write that is after the
  symlink is resolved, so a write through a link prepares the directory it
  lands in. None has a twin without the argument. A linking crate passes its
  own guard (`aoide-storage` passes `ensure_write_dir`); a caller that wants
  no guard says so at its own call site, with a plain create. `atomic_symlink`
  is the one creator with no step: it makes its link's parent with a plain
  `create_dir_all`. `secure_private_dir`'s Windows arm takes the step on the
  parent and makes its leaf owner-only through
  `owner_only::ensure_private_dir`. A new member that makes a directory takes
  the step the same way. `std::fs::create_dir_all` is generic and does not
  coerce to the step's type: pass a closure (`|d| std::fs::create_dir_all(d)`)
  or a named function. Why: a twin with storage's name and signature would let
  a mechanical import switch drop the guard without a compile error.
- **`fs::pid_is_alive` is the shared liveness probe** — a POSIX
  `kill(pid, 0)` where `ESRCH` alone is absent and every other errno (including
  `EPERM`) reads live, so an unanswerable probe never reaps or unlinks. `0` and
  `pid > pid_t::MAX` are refused BEFORE the syscall: both name a process group.
  Live is NOT identity, so a caller about to SIGNAL a pid still checks its own
  argv/port. Callers re-export or import this probe; never fork a `/proc` copy.
- **The two process ACTS live beside the probe and promise DIFFERENT things per
  host** — `fs::terminate` is `SIGTERM` (a request a trapped or hung child can
  survive) on Unix and `TerminateProcess` (uncatchable) on native Windows, and
  `fs::wait_for_exit` is `waitpid` there and `WaitForSingleObject` here. An
  agent editing either must keep the doc's table honest and keep callers
  written for the WEAKER arm; the one caller, the client crate's tunnel, reaches
  `terminate` only through its argv guard, which is what makes the hard arm
  safe. Never add a second kill/wait path: this is the seam.
- **The atomic-write temps carry a per-write counter.** `temp_path` takes the
  next value of a process-wide `AtomicU64` (`TEMP_SEQ`) and forms
  `<stem>.<n>.tmp.<pid>` for `atomic_write*`, `<n>.migrate-tmp.<pid>` for
  `migrate_dir`'s copy, and `<n>.seed.<pid>` for `seed_if_absent`. Two
  writes in one process, threads included, never share one of these, so a
  rename never finds another writer's temp already moved. The counter sits
  before `.tmp.`, so the sweep, which reads the pid after the last `.tmp.`,
  still reads the pid. One temp does not carry it: `atomic_symlink`'s
  `.{name}.tmp-{pid}` at `fs.rs:234`, which the counter does not cover.
  The counter adds `1 + digits(n)` bytes to each temp's name, so a target
  whose name is within that margin of `NAME_MAX` (255 bytes on Linux) now
  fails at create, where the `<stem>.tmp.<pid>` form still fit.
- **`fs::atomic_write`'s temp cleanup is directory-wide, and every failing
  half unlinks its own temp.** `sweep_stale_temps` reclaims any sibling
  `<stem>.<n>.tmp.<pid>` whose pid is dead, not only temps sharing the target's
  stem — a stem-scoped sweep can only ever reclaim a temp whose own file is
  written again, which never happens for a write-once name (an outbox entry
  is keyed by a msgid minted once; 1519 zero-byte orphans accumulated in one
  node's `state/outbox/` that way). The `read_dir` was already paid on every
  write, so widening the predicate costs nothing. Match `.tmp.` as a whole
  separator: `migrate_dir`'s `<n>.migrate-tmp.<pid>` and `seed_if_absent`'s
  `<n>.seed.<pid>` are deliberately distinct shapes and must stay out of the
  sweep. A new temp-file shape in this crate picks a separator that is NOT
  `.tmp.` unless it wants this sweep to own it.
- **Every atomic write to a watched path renames inside its watch.**
  `fs::watch_writes` registers a `WriteWatch` for a list of paths.
  `atomic_write_bytes_impl` runs its rename inside each live watch for the path
  the caller passed, before symlink resolution, the first registered outermost.
  A watch calls `rename` exactly once and returns its result. It never writes a
  watched path from inside `rename`, and it never writes through storage while
  holding its own lock, since the rename runs inside that call. The registry
  lock is released before any watch runs, and a poisoned one is recovered. The
  temp, its removal on failure and the stale-temp sweep stay outside the seam,
  so a write whose temp cannot be created never reaches a watch. A stage write
  that does not go through `atomic_write` is invisible to the seam, so a watcher
  reads it as a hand edit. `fs::seed_if_absent`'s absent-path seed is one such
  write, and it runs before the daemon registers its watch.
- **`fs.rs` carries five PUBLIC seams, each with one answer per host**, and a
  caller asks one of them rather than spelling a platform: `detach` (the
  detached-spawn posture — `setsid` in a `pre_exec`, or
  `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`), `link_dir` (a directory link —
  `symlink(2)` or `symlink_dir`, with a refused creation surfacing its own
  error), `path_is_under` (a project-prefix question, component-wise, with the
  `\\?\` verbatim prefix stripped; case-SENSITIVE on both hosts, stated as a
  limit), `looks_absolute_any_host` (a remote node's root judged by the grammar
  of the node it names) and `create_new_private` (write-once, the policy
  attached at creation, and an unlink before any refusal so a caller never
  inherits a "stale" file this process just made). The lock trio is `pub` for
  the same reason: the conduct crate probes with it instead of a second
  `flock`. Each of these has a native test on both hosts, or states its limit.
- **`fs::atomic_write_private` is the ONE way a sensitive file gets written**
  (the storage crate's identity key is its first caller). The
  TEMP is created ALREADY at `0600` (`OpenOptions::mode`, never
  `File::create`-then-`chmod`), so the `rename(2)` onto the live path never
  passes a wider-mode window, mirroring the secrets crate's
  `save_policies`/`save_totp_secret` (secure the temp BEFORE the rename,
  never the final path after it). A leftover temp of the same name is
  removed first and the create is exclusive (`O_EXCL`): `O_TRUNC` would keep
  the leftover's wider mode, and `O_EXCL` refuses a link planted at that
  name. Never write a sensitive file as a `File::create` + post-rename
  `set_permissions` pair. The temp's mode is pinned directly, not just the
  end state, by
  `fs::tests::the_private_temp_is_created_already_0600_before_any_rename`,
  and a leftover at the temp's name is pinned by
  `fs::tests::write_temp_file_never_keeps_a_leftovers_wider_mode`.
- **`fs::secure_private_dir` locks the DIRECTORY a sensitive file lives
  in, not only the file** (the storage crate's identity `mint()` calls it on
  its identity directory before writing anything into it). A `0600` file in a
  `0755` directory still leaves that directory's entries world-listable.
  The directory is created through the directory step. On Unix the chmod acts
  on a descriptor opened `O_NOFOLLOW`, never on the name: a directory that is
  a symlink is refused and its target keeps its mode, and a `dir` with a `..`
  in it is refused before anything is created, since the directory it climbs
  to is not one this call was asked to own. A future sensitive file in its OWN
  new subdirectory calls this on that subdirectory the same way, before the
  first `atomic_write_private` into it.
- **The family's Windows spellings live in `fs_windows.rs`**, a submodule of
  `fs` (the shape the protocol crate's `feed_windows.rs` set): every Unix-only
  primitive gets its Windows spelling there, and `fs.rs` keeps the one
  function each caller names with two `cfg` bodies. `write_temp_file`'s
  Windows arm reads the private temp back through `owner_only::file_privacy`
  BEFORE its first payload byte, because a `SECURITY_ATTRIBUTES` descriptor
  is a request and a volume that ignores ACLs would otherwise accept a
  private key at whatever policy it likes.
- **`git` stays a root module of this crate.**
  `common_dir_answers_with_git_absent_from_path` re-runs its own test binary in
  a child, filtered `--exact` on its own path under `git::tests`. Under a
  nested module that filter matches nothing, libtest runs zero tests and exits
  0, and the parent passes without having tested anything. The child's own
  result line reads `1 passed`.
- **`git::common_dir` is the one `git.rs` function that runs no process
  (SESSION-FIND B2).** It canonicalizes its input, walks up to the nearest
  `.git`, and follows a file's `gitdir:` (relative to the file's own
  directory) and that gitdir's `commondir`. It judges the `.git` entry's kind
  with `symlink_metadata`: a symlink, FIFO or socket answers as no repo. A git
  file is opened with `O_NOFOLLOW | O_NONBLOCK` (the open refuses a symlink
  and cannot block on a FIFO), its kind is judged on the open descriptor, and
  it is read to at most 4 KiB. Its no-git-on-`PATH` test re-runs itself in a
  child with an empty `PATH`, because the parent's `PATH` is shared by every
  test thread. Don't give it a `Command`.
- **A unix connect that must not hang goes through
  `local_socket::connect_bounded`, never a bare `UnixStream::connect`.** A full
  listen backlog makes the bare call wait forever. On Linux the socket
  `connect_bounded` opens is close-on-exec from `socket(2)` on, so a child
  spawned meanwhile never inherits it; elsewhere the flag is set just after
  `socket(2)`, and a child spawned in that instant can. Its poll branch
  (`EINPROGRESS`) has no test: a Linux unix connect never returns
  `EINPROGRESS`, so only other unix targets reach that branch.
- **There is one peer-credential implementation, here.** `peercred::peer_cred`,
  `PeerCred`, `PeerUser` and `own_user` are not copied into a consumer. A
  consumer that needs a display helper beside them (a username, a process
  name) keeps that helper in its own crate and imports the identity types.
- **A host seam that two crates need is exposed from here, never copied.**
  `owner_only` (the private file/directory policy), `win_proc` (the process
  table) and `win_unix` (the `AF_UNIX` binding) exist because two or more
  consumers need the same native implementation and this crate is the leaf
  both already depend on: neither consumer makes a second `windows-sys` call
  sequence of its own. The same rule covers the host-shaped seams that are NOT
  Windows-only: `host_shell` (which interpreter runs a stored command line) and
  `host_random` (this host's OS CSPRNG) each have two consumers and each keeps
  ONE spelling per host, never a `cfg` in the consumer. The rule cuts both
  ways: a new primitive with ONE consumer stays private to that consumer.
- **`cfg(windows)` code is read, not run.** No gate builds or tests it here,
  and the tests of the Windows modules are `cfg(windows)` too. An edit to a
  Windows arm says so in its commit and is read against the Unix contract it
  answers (a Windows arm keeps that contract or refuses by name); a change that
  is not a pure move gets a native check before anything relies on it.
- **`windows-sys`'s feature list is the union of what the modules name, and
  it shrinks only with a Windows build in hand.** A feature can gate the
  signature of an API (`WSAIoctl` takes an `OVERLAPPED`) with no `use` path
  naming it.

## Not tested here

- `hash` has no test module. The `seal` tests of the storage crate exercise
  `sha256`, `hex_encode` and `hex_decode` through the re-export, in the same
  gate.
- `host_name` and `runtime_dir` have no test of their own. Their tests sit
  beside the fallback and the segment they are compared with, in the storage
  crate's `display` and `runtime_dir` modules, and reach the items through the
  re-exports. `runtime_dir::euid` and `runtime_dir::host_default` are `pub`
  for those tests and for no other caller.
- Whether a linking crate's guard reaches each `fs` member: the tests here
  pass a plain create as the step. `aoide-storage`'s fs tests hold its
  wrappers to its guard.
- `stage` has no test module. Storage's stage test reads through the
  re-export.

## Extension points

- **A new seam** enters only if it reads no product state and needs no
  workspace crate: a module that reads a product's path, variable or policy
  stays in that product's crate. Add `pub mod <name>;` to `src/lib.rs` under
  its `cfg`, a bullet to README.md, and a `pub use ground::<name>;` line to the
  crate that re-exports it.
- **A new member of the `fs` family that creates a directory** takes
  `make_dir`. The crate that links it adds a wrapper that passes its guard,
  and a test that fails when the wrapper passes a plain create instead.
- **A new neutral crate** is named in `tests/boundary/scan.pl`'s `side_of`; see
  `pkgs/aoide/crates/AGENTS.md`.

## Docs that need an update in the same commit

- A module added, removed or renamed updates README.md, this file when an
  invariant moves with it, the re-exporting crate's `src/lib.rs` and its
  README.md and AGENTS.md, and the `ground` row of
  `docs/architecture/PACKAGE-LAYOUT.md`.
- A change to what the `boundary` check holds of a neutral crate updates
  `tests/boundary/README.md` and the header of `tests/boundary/scan.pl`.
- An `fs` or `stage` item added, removed or re-signed updates
  `aoide-storage`'s `fs.rs` wrappers and shim list, and its README and
  AGENTS.
