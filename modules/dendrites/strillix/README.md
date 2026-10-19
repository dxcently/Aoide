# modules/dendrites/strillix

Two roles in one directory. The catalogue entry `strillix` is the RICE RENDERER:
it writes the rice's Hyprland rice to the state link the compositor sources.
The strillix runtime is the shell surface, the bridge that feeds it, and the
`strillix` component providers built on them; it is reached only through those
providers, never through the `strillix` entry.

```
modules/dendrites/strillix/
├── default.nix        the catalogue entry: the rice renderer (below)
├── runtime.nix        the strillix runtime, system half: builds the QML tree,
│                      deploys it, watches it; publishes its home half as
│                      `aoide.strillix.home`
├── runtime-home.nix   the runtime's home half: the shell lane's home half
│                      plus `aoide.strillix.home`, read through `osConfig`
├── shellbridge.nix    the bidirectional bridge (socket + stage files)
└── bar.nix  launcher.nix  powermenu.nix  notifications.nix
    dock.nix calendar.nix  switcher.nix   lock.nix
                       the `strillix` provider of each component slot
```

The runtime pair is plain modules with no `habit` key, so any lane that brings
the runtime imports the same two paths and the host gets one of each
(AGENTS.md). `default.nix` imports neither.

## The rice renderer (`default.nix`)

Selecting `strillix` gives a host the rice seams and the state link, and nothing
that paints. `wayland.windowManager.hyprland.extraConfig` gets one line at
`mkAfter`: `source = ~/.local/state/strillix/rice.conf`, plus the host's override
file when it sets one. The renderer owns six options under `rice.hyprland`:
`settings` and `conf` (the rice's, `hostConf` and `host` the host's), and the
two read-only rendered files `file` and `hostFile`.

Render order, fixed (AGENTS.md):

```
rice-hyprland.conf  =  rice hyprland.conf   (rice.hyprland.conf, binds and layout)
                    +  toHyprconf settings  (rice.hyprland.settings, Nix, LAST)

~/.local/state/strillix/rice.conf   ->  <root>/$STRILLIX_ROOT/declared/rice.conf  (the declared twin)
                                    ->  the rendered file above       (the build's rice)
```

A stage repoints the state link without a rebuild: at the rice's own runtime
`hyprland.conf`, or at the empty staging-owned file when the rice has none
(`rice stage`, `rice mode declarative`, `strx reload`; CONTRACTS.md §4). The
renderer's activation relinks the state link only when it is absent or already
names the twin, so a staged link survives a rebuild. The reload
(`hyprctl reload config-only`) runs when the twin's target changed and the link
names it.

Under `package = null` (the compositor lane's setting) the renderer runs no
`hyprctl` itself on hosts where Home Manager has no Hyprland package; the
compositor's own file watcher is then the only reload, and it is unverified on a
retargeted symlink (the declarative arm of `strx reload` asks for the reload
explicitly).

## The strx component providers

A desktop is made of component slots, each a provider set in the catalogue
(`bar = { providers.strillix = ./dendrites/strillix/bar.nix; };`). The `strillix`
provider of a slot is that Quickshell component on the runtime's bridges: it
imports the runtime pair by path, records itself in `aoide.components`, and
declares the surface it owns. Two strillix slots on one host share one runtime:
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
| `wallpaper` | `../wallpaper/strillix.nix` | `wallpaper` |
| `greeter` | `../greeter.nix` | `greeter` |

The greeter is the one exception to "a strillix provider imports the runtime":
it runs in greetd, before any session, as the greeter user under cage in its
own process on a store tree of its own, so `greeter.nix` does NOT import the
runtime and fills its slot on a host with no shell. A rice with no greeter face gets `ly`.

The surfaces no slot answers for — `osd`, `agentWidgets`, `sessionGraph` —
stay the runtime's own. `runtime.nix` publishes the filled slots as
`run/qml/components.json` (CONTRACTS.md §4), beside `shell.qml`.

## Named seams (what it exposes)

- **`runtime.nix` — the paint lane.** Renders the Aoide shell surface with
  Quickshell, builds the config from `pkgs/strillix-shell`'s QML
  plus the widget bodies of the rices this host BUILT IN
  (`aoide.rices.builtIn`, from its own record), and deploys it to
  `$AOIDE_ROOT/run/qml` (`home.activation.aoideDeployQml` — an rsync, not a
  symlink tree: a deploy target the runtime writes into, never a source, so a
  switch reasserts the store over it). It declares
  the surfaces it owns in `aoide.surfaces` (stylix stands down for those),
  publishes `aoide.components` as `components.json`,
  seeds the declared rice into both stage twins, reasserts the paint on every
  activation, brings the staged or drafted rice back (below), runs the
  healthcheck timer, and installs `pkgs.aoide.strillix` — the strillix binary — as the
  `aoide.strillix.enable` fact. It also owns what "built in"
  means at the byte level: the deployed `manifest.json`/`registry.json` cover
  the built-in rices only, `pkgs.strillix-rices` is overridden to ship just those
  folders plus `builtin.json` (`{ declared, rices, packages }`), and
  `home.activation.aoideSeedRices` runs `strx rice refresh` over them: a
  file the machine never edited takes the repo's change, a built-in rice absent
  from the machine is added whole, a file the machine edited (or that has no
  record) is kept and reported, and `history/`, `drafts/` and symlinks are never
  touched. What it wrote is recorded in
  `$STRILLIX_ROOT/declared/rices/<rice>.json`, outside the rice folder.
  `strx rice refresh [--check]` is the CLI twin; a dry-run activation runs
  `--check`. The rices is the one place a rice is edited, so the copy is
  writable by its owner. That set is CLOSED UNDER BORROWS
  (`lib/rices.nix`'s `builtIn`): a
  selected rice's records name their slot bodies by `owner`, so the lenders ship
  and seed beside it, which is what makes every manifest record resolve to a
  directory that exists on the host. Selecting the lane is the switch; it sets
  `aoide.strillix.enable` `true`.
- **`lock.nix` — the lock.** `strx lock` runs one of two lockers, and the
  `lock` slot's strillix provider installs both and their PAM services: `aoide-lock`
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
  speaks an agent protocol). Gated on `aoide.enable && aoide.strillix.enable`:
  strillix owns the bridge, and its `ExecStart` execs strillix out of `pkgs.aoide.strillix`,
  a separate droppable output.
- **`runtime.nix` — `aoide-rice-reload`, the staged rice comes back.** Declared truth is laid
  down first, exactly as the activation always does (the rsync into `run/qml`
  and the seed into both stage twins), and nix never reads `stage/mode.json`.
  Then one user unit runs `strx reload`, which re-stages whatever the mode names:
  the last staged rice (palette, wallpaper, widget bodies, slot map, registry,
  compositor keywords, terminal colours), the routed draft (re-routed and
  re-synced), or, in `declarative`, nothing but the shell's reload. The unit is
  a one-shot ordered Before `aoide-quickshell.service`, so the shell's first
  frame is the staged rice, and it runs in the user manager because
  home-manager's activation has no Hyprland environment. It runs under
  `timeout 60`, since a oneshot has no start timeout and `strx reload` spawns
  children with none, and it stays active even when the reload refuses or is
  cut off. At login the session starts it; on a switch
  `aoideRestartRice` restarts it together with the shell in ONE `try-restart`,
  after home-manager's file and systemd steps and after the QML deploy and both
  seeds (`aoideDeployQml`, `aoideSeedStage`, `aoideSeedRices` — the reload
  reads the runtime rices), which is what puts the reload between the seed
  and the shell's restart. `aoideMigrateRoots` runs before every seed and
  before `strillixRice`: it runs `strillix --help` once under
  `STRILLIX_MIGRATE=1`, so the pre-rename roots move into the new ones before any
  seed creates them (a dry run skips it). Outside that activation the rice move
  waits for `strx migrate`.
  Once the rice tree has moved, `$AOIDE_ROOT/song` is a marker file, and the
  migration reads it as already done. Nothing here creates `song/` again.

## The seam with `quickshell`

The shell PROCESS is not this lane's: `modules/dendrites/quickshell/` holds the
quickshell package and the one `aoide-quickshell` user service. What this lane
holds is the surface — what gets painted, from which rice, deployed where. The
runtime brings the shell with it by importing the shell lane's plain halves by
path (`../quickshell/system.nix` from `runtime.nix`, `../quickshell/home.nix`
from `runtime-home.nix`), so selecting `strillix` alone runs a shell, and a host
that selects both lanes gets the one module of each.

Beyond those imports the two meet on one fact:

```
strillix      sets  aoide.quickshell.config = "$AOIDE_ROOT/run/qml"   (this lane's deploy target)
quickshell reads it, installs the package, runs  quickshell -p <config>/shell.qml,
          and anchors `aoided` at graphical-session.target
```

So a shell with no strillix is a supported host, in either of two shapes: **own
config** — the host sets `aoide.quickshell.config` to a config directory of its
own (a store path or a path in someone's home) and gets the package, the
service, and the session anchor; or **bare** — nothing is set, and the host gets
the package and nothing else. Neither shape builds a QML tree, installs the
rice binary, runs shellbridge, the healthcheck or the rice reload: those are
this lane's, and this lane is what a rice needs.

That is also why the rice is gated here and not in the shell lane: `aoide.rice`
with no `strillix` is a host that says "perform this rice" with nothing to perform
it, and the platform asserts on exactly that (`modules/nucleus/assertions.nix`).
The service itself no longer cares — it starts on a config directory, rice or
no rice.


Beside this directory: `pkgs/strillix-shell` ships the QML, icons and preview
fixtures; `pkgs/strillix-rices` ships the built-in rices and their manifests.

## What it consumes

Only the dress (`aoide.theme`), the structure (`aoide.layout`), the
surface registry it declares into (`aoide.surfaces`), the component slots it
publishes (`aoide.components`), the core scalars
(`aoide.enable`, `aoide.root`, `aoide.checkout`, `aoide.hostConfig`,
`aoide.user`), the rice
selection (`aoide.rice`), the derived fact
`aoide.rices.builtIn` — the rices this host builds in, which is what the
deployed tree, the installed packages, the shipped templates and the seed are
built from — and its own fact: root `AGENTS.md` house rule 5. Component-tier
fallback (null → palette) is applied locally and
never pushed back into the option system. It never reads a `rice/` RUNTIME path
at build time (the structural half is `checks.rice-runtime-untracked`); committed
rices score is not a runtime path.

`checks.theme-fanout` guards the activation seed: the stage twin is the active
rice's committed theme with the venue's `aoide.theme.override` applied and the
host's geometry laid over the rice's own, through `lib/theme.nix`'s
`stagePatch`, and the same jq run publishes the declared twin
(`/declared/theme.json`) — this lane is its only writer. The same script
publishes the venue's part as data, `/declared/venue.json`
(`lib/theme.nix`'s `venueDelta`: only the slots the venue recolours and the
geometry it sets, `{}` with neither), which the runtime stage lays over the
runtime rices's copy of the declared rice; `checks.theme-fanout` guards
`venueDelta` beside `stagePatch`. The same script publishes one more declared
artefact, `/declared/terminal-opacity.conf`
(one `background_opacity` line from the rice's `geometry.terminalOpacity`,
deleted when that is null): the kitty dendrite includes it BEFORE the staged
colours, so a host that has never staged a rice still opens its terminal at the
rice's opacity. That read is why THIS lane carries it — a paint dendrite reads
the dress; the kitty lane names only the path. The seed publishes
`/declared/hyprlock.conf` the same way: the declared rice's lock-screen
dress, rendered at build time by the strillix binary (`strx theme emit hyprlock`)
from the same venue-recoloured notes and the rice's own `lockscreen.<ext>`,
comments only when it ships none. hyprlock's config (`lock.nix`, above) sources it before
`/stage/hyprlock.conf`, so a host that has never staged a rice locks in the
declared one.

## How it composes

Naming no rice performs no rice: `aoide.rice` defaults to null, no rice's body
is imported, and this lane's deploy/seed/restart half is gated on the performed
rice — so a host that names nothing
gets no QML tree and nothing deployed, rather than an empty surface. The shell
service itself is the `quickshell` lane's and rides the config directory
instead: it starts on whatever directory was named, so a host with its own
config runs a shell and names no rice. Shellbridge rides the strillix fact, not the
rice. QML is a render surface: state, policy, IPC and
system access live behind bridges reachable with only a shell (CONTRACTS.md §0's
paint test), so deleting every `.qml` leaves every capability reachable from a
terminal.
