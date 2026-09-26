# modules/dendrites/lyra

The shell surface and the bridge that feeds it.

```
modules/dendrites/lyra/
├── default.nix        the lane: builds the QML tree, deploys it, watches it
└── shellbridge.nix    the bidirectional bridge (socket + stage files)
```

## Named seams (what it exposes)

- **`default.nix` — the paint lane.** Renders the complete Aoide shell surface
  with Quickshell (bar, notifications, launcher, osd, lockscreen, wallpaper,
  agentWidgets, sessionGraph), builds the config from `pkgs/lyra-shell`'s QML
  plus the widget bodies of the songs this host BUILT IN
  (`aoide.songbook.builtIn`, from its own record), and deploys it to
  `$AOIDE_ROOT/run/qml` (`home.activation.aoideDeployQml` — an rsync, not a
  symlink tree, so live QML edits survive until the next switch). It declares
  the surfaces it owns in `aoide.surfaces` (stylix stands down for those),
  seeds the live stage from the active song, reasserts the paint on every
  activation, runs the healthcheck timer, and installs `pkgs.aoide.rice` — the
  lyra binary — as the `aoide.lyra.enable` fact. It also owns what "built in"
  means at the byte level: the deployed `manifest.json`/`registry.json` cover
  the built-in songs only, `pkgs.lyra-songbook` is overridden to ship just those
  folders plus `builtin.json` (`{ declared, songs, packages }`), and
  `home.activation.aoideSeedSongbook` copies each one into the machine's own
  songbook **only when it is absent** — a rebuild never rewrites what the machine
  has. That set is CLOSED UNDER BORROWS (`lib/songbook.nix`'s `builtIn`): a
  selected song's records name their slot bodies by `owner`, so the lenders ship
  and seed beside it, which is what makes every manifest record resolve to a
  directory that exists on the host. Guarded on that fact; the lane sets it
  `mkDefault true`.
- **`shellbridge.nix` — the bridge.** The bidirectional seam between the daemon
  / agents and the live desktop: OUT as atomic JSON under `state/stage/`, IN as
  unix-socket commands, and the only consumer of Hyprland's IPC (QML never
  speaks an agent protocol). Self-gated on `aoide.enable && aoide.lyra.enable`:
  lyra owns the bridge, and its `ExecStart` execs lyra out of `pkgs.aoide.rice`,
  a separate droppable output.

## The seam with `quickshell`

The shell RUNTIME is not this lane's: `modules/dendrites/quickshell.nix` holds
the quickshell package and the one `aoide-quickshell` user service. What this
lane holds is the surface — what gets painted, from which song, deployed where.

The two meet on one fact and never name each other:

```
lyra      sets  aoide.quickshell.config = "$AOIDE_ROOT/run/qml"   (this lane's deploy target)
quickshell reads it, installs the package, runs  quickshell -p <config>/shell.qml,
          and anchors `aoided` at graphical-session.target
```

So a shell with no lyra is a supported host, in either of two shapes: **own
config** — the host sets `aoide.quickshell.config` to a config directory of its
own (a store path or a path in someone's home) and gets the package, the
service, and the session anchor; or **bare** — nothing is set, and the host gets
the package and nothing else. Neither shape builds a QML tree, installs the
rice binary, runs shellbridge or the healthcheck: those are this lane's, and
this lane is what a song needs.

That is also why the song is gated here and not in the shell lane: `aoide.song`
with no `lyra` is a host that says "perform this rice" with nothing to perform
it, and the platform asserts on exactly that (`modules/nucleus/assertions.nix`).
The service itself no longer cares — it starts on a config directory, song or
no song.


Beside this directory: `pkgs/lyra-shell` ships the QML, icons and preview
fixtures; `pkgs/lyra-songbook` ships the built-in songs and their manifests.

## What it consumes

Only the dress (`aoide.livery`), the structure (`aoide.arrangement`), the
surface registry it declares into (`aoide.surfaces`), the identity scalars
(`aoide.user`, `aoide.root`, `aoide.song`), the derived fact
`aoide.songbook.builtIn` — the songs this host builds in, which is what the
deployed tree, the installed packages, the shipped templates and the seed are
built from — and its own fact: root `AGENTS.md` house rule 5. Component-tier
fallback (null → palette) is applied locally and
never pushed back into the option system. It never reads a `song/` RUNTIME path
at build time (the structural half is `checks.song-runtime-untracked`); committed
songbook score is not a runtime path.

`checks.livery-fanout` guards the activation seed: the stage twin is the active
song's committed livery with the venue's `aoide.livery.override` applied through
`lib/livery.nix`'s `stagePatch`, and the same jq run publishes the declared twin
(`song/declared/livery.json`) — this lane is its only writer.

## How it composes

Naming no song performs no song: `aoide.song` defaults to null, every committed
`rice.nix` self-gates on `config.aoide.song == "<name>"`, and this lane's
deploy/seed/restart half is gated the same way — so a host that names nothing
gets no QML tree and nothing deployed, rather than an empty surface. The shell
service itself is the `quickshell` lane's and rides the config directory
instead: it starts on whatever directory was named, so a host with its own
config runs a shell and names no song. Shellbridge rides the lyra fact, not the
song. QML is a render surface: state, policy, IPC and
system access live behind bridges reachable with only a shell (CONTRACTS.md §0's
paint test), so deleting every `.qml` leaves every capability reachable from a
terminal.
