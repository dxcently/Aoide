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
  `aoide_*` path. A fixture takes `ground` for the product word
  (`ground-peercred-{pid}`, `ground-cap-{pid}-{nanos}`). A test helper is
  written in this crate and never taken from `aoide-test-support`.
- **A moved item is one item at both paths.** `aoide-protocol` re-exports a
  module (`pub use ground::<module>`) under the `cfg` the module has here. A
  consumer never gets a copy (`pkgs/aoide/crates/AGENTS.md`: no cross-crate
  copying). A signature change here changes the old path too, so check the
  whole workspace (`cargo check --workspace --all-targets`).
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

## Extension points

- **A new seam** enters only if it reads no product state and needs no
  workspace crate: a module that reads a product's path, variable or policy
  stays in that product's crate. Add `pub mod <name>;` to `src/lib.rs` under
  its `cfg`, a bullet to README.md, and a `pub use ground::<name>;` line to the
  crate that re-exports it.
- **A new neutral crate** is named in `tests/boundary/scan.pl`'s `side_of`; see
  `pkgs/aoide/crates/AGENTS.md`.

## Docs that need an update in the same commit

- A module added, removed or renamed updates README.md, this file when an
  invariant moves with it, the re-exporting crate's `src/lib.rs` and its
  README.md and AGENTS.md, and the `ground` row of
  `docs/architecture/PACKAGE-LAYOUT.md`.
- A change to what the `boundary` check holds of a neutral crate updates
  `tests/boundary/README.md` and the header of `tests/boundary/scan.pl`.
