# modules/dendrites/lyra

The shell surface and the bridge that feeds it.

```
modules/dendrites/lyra/
├── default.nix        the lane: builds the QML tree, deploys it, runs the shell
└── shellbridge.nix    the bidirectional bridge (socket + stage files)
```

## Named seams (what it exposes)

- **`default.nix` — the paint lane.** Renders the complete Aoide shell surface
  with Quickshell (bar, notifications, launcher, osd, lockscreen, wallpaper,
  agentWidgets, sessionGraph), builds the config from `pkgs/lyra-shell`'s QML
  plus each committed song's `widgets/`, and deploys it to
  `$AOIDE_ROOT/run/qml` (`home.activation.aoideDeployQml` — an rsync, not a
  symlink tree, so live QML edits survive until the next switch). It declares
  the surfaces it owns in `aoide.surfaces` (stylix stands down for those),
  anchors `aoided` with `aoide.sessionTarget = "graphical-session.target"`,
  runs `aoide-quickshell` and its healthcheck timer, and installs `pkgs.aoide.
  rice` — the lyra binary — as the `aoide.lyra.enable` fact. Guarded on that
  fact; the lane sets it `mkDefault true`.
- **`shellbridge.nix` — the bridge.** The bidirectional seam between the daemon
  / agents and the live desktop: OUT as atomic JSON under `state/stage/`, IN as
  unix-socket commands, and the only consumer of Hyprland's IPC (QML never
  speaks an agent protocol). Self-gated on `aoide.enable && aoide.lyra.enable`:
  lyra owns the bridge, and its `ExecStart` execs lyra out of `pkgs.aoide.rice`,
  a separate droppable output.

Beside this directory: `pkgs/lyra-shell` ships the QML, icons and preview
fixtures; `pkgs/lyra-songbook` ships the built-in songs and their manifests.

## What it consumes

Only the dress (`aoide.livery`), the structure (`aoide.arrangement`), the
surface registry it declares into (`aoide.surfaces`), the identity scalars
(`aoide.user`, `aoide.root`, `aoide.song`) and its own fact — root `AGENTS.md`
house rule 5. Component-tier fallback (null → palette) is applied locally and
never pushed back into the option system. It never reads a `song/` RUNTIME path
at build time (`checks.no-song-read` enforces that structurally); committed
songbook score is not a runtime path.

`checks.livery-fanout` guards the activation seed: the stage twin is the active
song's committed livery with the venue's `aoide.livery.override` applied through
`lib/livery.nix`'s `stagePatch`, and the same jq run publishes the declared twin
(`song/declared/livery.json`) — this lane is its only writer.

## How it composes

Naming no song performs no song: `aoide.song` defaults to null, every committed
`rice.nix` self-gates on `config.aoide.song == "<name>"`, and this lane's shell
service is gated the same way — so a host that names nothing gets no QML tree
and no shell service, rather than an empty surface. Shellbridge rides the lyra
fact, not the song. QML is a render surface: state, policy, IPC and system
access live behind bridges reachable with only a shell (CONTRACTS.md §0's paint
test), so deleting every `.qml` leaves every capability reachable from a
terminal.
