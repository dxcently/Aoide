# modules/dendrites/lyra

Two roles in one directory. The catalogue entry `lyra` is the RICE RENDERER:
it writes the song's Hyprland rice to the state link the compositor sources.
The vibeos runtime is the shell surface, the bridge that feeds it, and the
`vibeos` component providers built on them; it is reached only through those
providers, never through the `lyra` entry.

```
modules/dendrites/lyra/
├── default.nix        the catalogue entry: the rice renderer (below)
├── runtime.nix        the vibeos runtime, system half: builds the QML tree,
│                      deploys it, watches it; publishes its home half as
│                      `aoide.lyra.home`
├── runtime-home.nix   the runtime's home half: the shell lane's home half
│                      plus `aoide.lyra.home`, read through `osConfig`
├── shellbridge.nix    the bidirectional bridge (socket + stage files)
└── bar.nix  launcher.nix  powermenu.nix  notifications.nix
    dock.nix calendar.nix  switcher.nix   lock.nix
                       the `vibeos` provider of each component slot
```

The runtime pair is plain modules with no `habit` key, so any lane that brings
the runtime imports the same two paths and the host gets one of each
(AGENTS.md). `default.nix` imports neither.

## The rice renderer (`default.nix`)

Selecting `lyra` gives a host the rice seams and the state link, and nothing
that paints. `wayland.windowManager.hyprland.extraConfig` gets one line at
`mkAfter`: `source = ~/.local/state/lyra/rice.conf`, plus the host's override
file when it sets one. The renderer owns six options under `rice.hyprland`:
`settings` and `conf` (the song's, `hostConf` and `host` the host's), and the
two read-only rendered files `file` and `hostFile`.

Render order, fixed (AGENTS.md):

```
rice-hyprland.conf  =  song hyprland.conf   (rice.hyprland.conf, binds and layout)
                    +  toHyprconf settings  (rice.hyprland.settings, Nix, LAST)

~/.local/state/lyra/rice.conf   ->  <root>/song/declared/rice.conf  (the declared twin)
                                    ->  the rendered file above       (the build's rice)
```

A stage repoints the state link without a rebuild: at the song's own runtime
`hyprland.conf`, or at the empty staging-owned file when the song has none
(`rice stage`, `rice mode declarative`, `lyra reload`; CONTRACTS.md §4). The
renderer's activation relinks the state link only when it is absent or already
names the twin, so a staged link survives a rebuild. The reload
(`hyprctl reload config-only`) runs when the twin's target changed and the link
names it.

Under `package = null` (the compositor lane's setting) the renderer runs no
`hyprctl` itself on hosts where Home Manager has no Hyprland package; the
compositor's own file watcher is then the only reload, and it is unverified on a
retargeted symlink (the declarative arm of `lyra reload` asks for the reload
explicitly).

## The vibeos component providers

A desktop is made of component slots, each a provider set in the catalogue
(`bar = { providers.vibeos = ./dendrites/lyra/bar.nix; };`). The `vibeos`
provider of a slot is that Quickshell component on the runtime's bridges: it
imports the runtime pair by path, records itself in `aoide.components`, and
declares the surface it owns. Two vibeos slots on one host share one runtime:
one shell, one bridge, one deployed tree.

| Slot | Provider file | `aoide.surfaces` owner it declares |
|---|---|---|
| `bar` | `bar.nix` | `bar` |
| `launcher` | `launcher.nix` | `launcher` |
| `powermenu` | `powermenu.nix` | — |
| `notifications` | `notifications.nix` | `notifications` |
| `dock` | `dock.nix` | — |
| `calendar` | `calendar.nix` | — |
| `switcher` | `switcher.nix` | — |
| `lock` | `lock.nix` | `lockscreen`, plus the lockers and their PAM services (below) |
| `wallpaper` | `../wallpaper/vibeos.nix` | `wallpaper` |
| `greeter` | `../greeter.nix` | `greeter` |

The greeter is separable: it runs in greetd, before any session, on a store
tree of its own, so `greeter.nix` does NOT import the runtime and fills its
slot on a host with no shell. A song with no greeter face gets `ly`.

The surfaces no slot answers for — `osd`, `agentWidgets`, `sessionGraph` —
stay the runtime's own. `runtime.nix` publishes the filled slots as
`run/qml/components.json` (CONTRACTS.md §4), beside `shell.qml`.

## Named seams (what it exposes)

- **`runtime.nix` — the paint lane.** Renders the Aoide shell surface with
  Quickshell, builds the config from `pkgs/lyra-shell`'s QML
  plus the widget bodies of the songs this host BUILT IN
  (`aoide.songbook.builtIn`, from its own record), and deploys it to
  `$AOIDE_ROOT/run/qml` (`home.activation.aoideDeployQml` — an rsync, not a
  symlink tree: a deploy target the runtime writes into, never a source, so a
  switch reasserts the store over it). It declares
  the surfaces it owns in `aoide.surfaces` (stylix stands down for those),
  publishes `aoide.components` as `components.json`,
  seeds the declared song into both stage twins, reasserts the paint on every
  activation, brings the staged or drafted song back (below), runs the
  healthcheck timer, and installs `pkgs.aoide.rice` — the lyra binary — as the
  `aoide.lyra.enable` fact. It also owns what "built in"
  means at the byte level: the deployed `manifest.json`/`registry.json` cover
  the built-in songs only, `pkgs.lyra-songbook` is overridden to ship just those
  folders plus `builtin.json` (`{ declared, songs, packages }`), and
  `home.activation.aoideSeedSongbook` runs `lyra rice refresh` over them: a
  file the machine never edited takes the repo's change, a built-in song absent
  from the machine is added whole, a file the machine edited (or that has no
  record) is kept and reported, and `takes/`, `drafts/` and symlinks are never
  touched. What it wrote is recorded in
  `$AOIDE_ROOT/song/declared/songbook/<song>.json`, outside the song folder.
  `lyra rice refresh [--check]` is the CLI twin; a dry-run activation runs
  `--check`. The songbook is the one place a song is edited, so the copy is
  writable by its owner. That set is CLOSED UNDER BORROWS
  (`lib/songbook.nix`'s `builtIn`): a
  selected song's records name their slot bodies by `owner`, so the lenders ship
  and seed beside it, which is what makes every manifest record resolve to a
  directory that exists on the host. Selecting the lane is the switch; it sets
  `aoide.lyra.enable` `true`.
- **`lock.nix` — the lock.** `lyra lock` runs one of two lockers, and the
  `lock` slot's vibeos provider installs both and their PAM services: `aoide-lock`
  for the shell's `LockHost.qml` (a service of its own, not `login`'s
  session-and-tty stack, and a name a host can tune for the lock alone) and
  `hyprlock` for hyprlock, whose package goes in directly rather than through
  `programs.hyprlock` (which would also start a config-less hypridle). It
  ships hyprlock's system-wide config, `/etc/xdg/hypr/hyprlock.conf`: the ten
  `$aoide_lock_*` variables at the plain look, `source` of the declared then
  the staged dress (CONTRACTS.md §4), then the widgets that read them, by
  paint keys only. A user's own `~/.config/hypr/hyprlock.conf` wins over it.
- **`shellbridge.nix` — the bridge.** The bidirectional seam between the daemon
  / agents and the live desktop: OUT as atomic JSON under `state/stage/`, IN as
  unix-socket commands, and the only consumer of Hyprland's IPC (QML never
  speaks an agent protocol). Gated on `aoide.enable && aoide.lyra.enable`:
  lyra owns the bridge, and its `ExecStart` execs lyra out of `pkgs.aoide.rice`,
  a separate droppable output.
- **`runtime.nix` — `aoide-rice-reload`, the staged song comes back.** Declared truth is laid
  down first, exactly as the activation always does (the rsync into `run/qml`
  and the seed into both stage twins), and nix never reads `stage/mode.json`.
  Then one user unit runs `lyra reload`, which re-stages whatever the mode names:
  the last staged song (palette, cover, widget bodies, slot map, registry,
  compositor keywords, terminal colours), the routed draft (re-routed and
  re-synced), or, in `declarative`, nothing but the shell's reload. The unit is
  a one-shot ordered Before `aoide-quickshell.service`, so the shell's first
  frame is the staged song, and it runs in the user manager because
  home-manager's activation has no Hyprland environment. It runs under
  `timeout 60`, since a oneshot has no start timeout and `lyra reload` spawns
  children with none, and it stays active even when the reload refuses or is
  cut off. At login the session starts it; on a switch
  `aoideRestartRice` restarts it together with the shell in ONE `try-restart`,
  after home-manager's file and systemd steps and after the QML deploy and both
  seeds (`aoideDeployQml`, `aoideSeedStage`, `aoideSeedSongbook` — the reload
  reads the runtime songbook), which is what puts the reload between the seed
  and the shell's restart.

## The seam with `quickshell`

The shell PROCESS is not this lane's: `modules/dendrites/quickshell/` holds the
quickshell package and the one `aoide-quickshell` user service. What this lane
holds is the surface — what gets painted, from which song, deployed where. The
runtime brings the shell with it by importing the shell lane's plain halves by
path (`../quickshell/system.nix` from `runtime.nix`, `../quickshell/home.nix`
from `runtime-home.nix`), so selecting `lyra` alone runs a shell, and a host
that selects both lanes gets the one module of each.

Beyond those imports the two meet on one fact:

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
rice binary, runs shellbridge, the healthcheck or the rice reload: those are
this lane's, and this lane is what a song needs.

That is also why the song is gated here and not in the shell lane: `aoide.song`
with no `lyra` is a host that says "perform this rice" with nothing to perform
it, and the platform asserts on exactly that (`modules/nucleus/assertions.nix`).
The service itself no longer cares — it starts on a config directory, song or
no song.


Beside this directory: `pkgs/lyra-shell` ships the QML, icons and preview
fixtures; `pkgs/lyra-songbook` ships the built-in songs and their manifests.

## What it consumes

Only the dress (`aoide.livery`), the structure (`aoide.arrangement`), the
surface registry it declares into (`aoide.surfaces`), the component slots it
publishes (`aoide.components`), the core scalars
(`aoide.enable`, `aoide.root`, `aoide.checkout`, `aoide.hostConfig`,
`aoide.user`), the song
selection (`aoide.song`), the derived fact
`aoide.songbook.builtIn` — the songs this host builds in, which is what the
deployed tree, the installed packages, the shipped templates and the seed are
built from — and its own fact: root `AGENTS.md` house rule 5. Component-tier
fallback (null → palette) is applied locally and
never pushed back into the option system. It never reads a `song/` RUNTIME path
at build time (the structural half is `checks.song-runtime-untracked`); committed
songbook score is not a runtime path.

`checks.livery-fanout` guards the activation seed: the stage twin is the active
song's committed livery with the venue's `aoide.livery.override` applied and the
host's geometry laid over the song's own, through `lib/livery.nix`'s
`stagePatch`, and the same jq run publishes the declared twin
(`song/declared/livery.json`) — this lane is its only writer. The same script
publishes the venue's part as data, `song/declared/venue.json`
(`lib/livery.nix`'s `venueDelta`: only the slots the venue recolours and the
geometry it sets, `{}` with neither), which the runtime stage lays over the
runtime songbook's copy of the declared song; `checks.livery-fanout` guards
`venueDelta` beside `stagePatch`. The same script publishes one more declared
artefact, `song/declared/terminal-opacity.conf`
(one `background_opacity` line from the song's `geometry.terminalOpacity`,
deleted when that is null): the kitty dendrite includes it BEFORE the staged
colours, so a host that has never staged a song still opens its terminal at the
song's opacity. That read is why THIS lane carries it — a paint dendrite reads
the dress; the kitty lane names only the path. The seed publishes
`song/declared/hyprlock.conf` the same way: the declared song's lock-screen
dress, rendered at build time by the lyra binary (`lyra livery emit hyprlock`)
from the same venue-recoloured notes and the song's own `lockscreen.<ext>`,
comments only when it ships none. hyprlock's config (`lock.nix`, above) sources it before
`song/stage/hyprlock.conf`, so a host that has never staged a song locks in the
declared one.

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
