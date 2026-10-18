# tests/ — non-cargo testing

Test content that isn't cargo's: whole-system and whole-artifact checks that
need Nix — a build, or an evaluation of the whole configuration — to even
exist. `lib/` holds build/eval machinery
(`checks.nix`, `aoideos.nix`, `pkgs.nix`, `rices.nix`);
this directory holds
what those checks actually test.

- `vm-boot.nix` — headless NixOS boot test, wired as `checks.<system>.vm-boot`.
- `vm-lock-greeter.nix` — graphical NixOS test of the login screen and the
  screen lock, wired as `checks.<system>.vm-lock-greeter`. Three machines
  built by the real constructor over the fixture rices in
  `lock-greeter/rices/`: `aubade` ships a `greeter.qml`, a `lockscreen.qml`
  and a `powermenu.qml` that reports its bridge (greetd with the Quickshell
  greeter over two accounts, `strx lock` through the shell's LockHost),
  `plainsong` ships none (ly, and `strx lock` runs hyprlock), and
  `nocturne`'s greeter face throws at start (the greeter's floor logs in).
  It types passwords at the greeter (a refusal, a retry, switch user) and
  the lock, checks the compositor's own lock state (`hyprctl locked`) and
  logind's `LockedHint`, kills the shell while locked and watches the
  restarted shell take the lock back on its own, swaps in
  `lock-greeter/broken-lockscreen.qml` (does not compile) and
  `lock-greeter/throwing-lockscreen.qml` (never reports ready) to prove the
  floor, and removes the `aoide-lock` PAM service to prove the lock falls to
  hyprlock instead of locking the user out. The greeter's own log is read
  from its Quickshell instance's log file.
  Screenshots land in the driver's output directory. Requires KVM.
- `portability.nix` — asserts `packages.aoide-static` is genuinely free of
  nix, wired as `checks.<system>.portability` (`lib/checks.nix` Check 8).
- `inventory.sh` — `tests/inventory.sh <flakeref> <host>`, the per-host
  evaluation inventory for the phase-5 gates: systemPackages names (an entry
  with no `name` keeps its outPath basename), the systemd unit namespaces
  (services, timers, sockets and paths, system and system-user, plus
  home-manager's own user services and the tmpfiles rules in declared order),
  home-manager activation entries with the `home.file`/`xdg.configFile` targets
  they seed, normal users and their groups, the toplevel drvPath, the
  `aoideOptions` names, and the `ricesManifest` sha256, as plain diffable
  text. Evaluation evidence only — it builds and activates nothing. Every
  phase-5 slice runs it at its parent commit and at HEAD and puts both columns
  in the commit body; a slice that predicts a drvPath delta names it, because
  G5 is opaque and the named measures are the only ones that say what moved.
- `selection/` — `tests/selection/run.sh` executes what is Aoide's own, case by
  case: `cases.nix` holds one attribute per case, the runner evaluates each on
  its own, and a negative case has to fail with a message the runner greps for,
  so a vague error is a failing test rather than a passing one. Composition
  itself is habit's and is tested in habit; three cases only show Aoide's real
  registry still composes through it. The rest are the rices's
  (`lib/rices.nix`) over the fixture rices beside it, one of which
  `throw`s on import, which is how "an unselected song stays unread" is proved
  instead of asserted, and the packages walker's overlay (`lib/pkgs.nix`): a
  name in `intentionalOverrides` yields to the overlay that replaced it, and any
  other walker name another overlay provides is refused by name. `lib` and
  habit come from this flake's own lock, so the cases run against the lib every
  host evaluates with and the habit every host is built by. Portable: `nix
  eval` plus bash, no VM, no host path.
- `templates/` — `tests/templates/run.sh` assembles a whole tree out of
  `templates/`, parses every template, resolves two hosts against the real
  constructor, and checks that the files nobody selected stayed unread. It
  keeps `templates/` from drifting away from the constructor.
- `quickshell-seam/` — `tests/quickshell-seam/run.sh` evaluates three fixture
  hosts through the real constructor (habit's composition) with the real
  catalogue and nucleus, and checks the quickshell/strillix seam: a host with its
  own `quickshell.config` runs the `aoide-quickshell` service on that directory
  and owes strillix nothing (no rice binary, no `songs/`, no shellbridge, no
  healthcheck); a host with no config gets the package and no service; and a
  song with no `strillix` lane fails its own evaluation with the taught message.
  `fixtures.nix` holds the readings, `hosts/` the three host modules. The ref
  IS the tree under test — fixtures, host modules, catalogue and nucleus are
  all read out of it, so a commit ref tests that commit and `path:` (the
  default: THIS checkout, uncommitted work included) tests what is on disk.
  Its positive control is `yomi-strix` from the same ref: a real host that HAS
  the shellbridge unit and the rice binary the strillix-less fixtures must not
  have, so an absence check that could never fail is not one.
- `distrobox.md` — manual container-based portability suite; not a gate.
- `consumer/` — `tests/consumer/run.sh`, the export-surface fixture: a
  stranger's flake (`flake.nix`, its own host modules, user, aggregation, songs
  and rices) that reaches AoideOS ONLY through the root flake's public
  outputs. The runner greps the fixture for any path into Aoide's tree (and
  proves that grep can fire), then applies the fixture's `outputs` to the ref
  under test's own inputs — so a commit ref really freezes the tree the consumer
  is built against — and reads the resulting hosts. `consumer` performs from the
  fixture's own rices: the nucleus contract and its own `nucleus` group, the shell, the theme, one performed song plus
  one built in that BORROWS the performer's widgets, the deployed config, the
  seeded rices, and the templates on the units. `aoide-songs` performs
  Aoide's own `sonata` through the `ricesRoot` export, and its strillix lane is
  handed that same directory. The contract it exercises is PACKAGE-LAYOUT's
  "The root flake — AoideOS's export surface".
  The value-slot proof: the consumer's own `bar`, `dock` and `lock` slots each
  name a provider of its own beside Aoide's `parts`. The `stubbed` host mixes
  its `bar` (a stub that brings no runtime) with Aoide's `dock`, and must give
  exactly two components, one shell unit and one bridge. `stub-dock.nix` and
  `stub-lock.nix` throw on read, and no host selects them, so every host
  evaluating is the proof they are never read (a mutation that selects one
  fails the consumer eval). The `front` checks read `parts` (ten, each a file),
  `rices`, and prove that a rice merged into its own hook is refused as a
  collision.

`selection/`, `templates/` and `quickshell-seam/` shell out to `nix eval` (and
`getFlake` a nixpkgs rev), so they run where an evaluation can: a developer's
shell, and
each phase-5 slice's gate. They are deliberately NOT `checks.*` — a check is a
build, and a build sandbox has no nix daemon and no network (`nix eval` inside
one fails creating `/nix/var/nix/profiles`), so a nested evaluation is not
something a check can do.

A top-level `tests/` is not a Rust convention — this is not where `cargo
test` looks. It is the NixOS one (`nixos/tests/`), extended here to any
non-cargo test.

## Why Rust tests do NOT live here

`#[cfg(test)]` unit tests stay in-source: they need access to private
functions, so moving them out breaks what they test. `crates/<name>/tests/`
is cargo's own convention for integration tests — it's where `cargo test -p
<crate>` looks, and that per-crate scoping is load-bearing here: `cargo test
--workspace` deadlocks (the conduct crate binds real sockets), so every Rust
test in this repo is run crate-scoped, always. Consolidating Rust tests into
this directory would break the only test invocation that works.
