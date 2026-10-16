# modules/dendrites/compositor

The compositor capability: one provider registry, and the Hyprland provider
that answers it today.

```
modules/dendrites/compositor/
├── default.nix            { providers.hyprland = ./hyprland; }   ← the registry
└── hyprland/
    ├── default.nix        the LOOK      (`compositor` selects this)
    └── behaviour.nix      the BEHAVIOUR (its own catalogue name, `hyprland`)
```

## Named seams (what it exposes)

- **The registry** names each alternative once. The constructor
  (habit's composition) reads it before any module graph exists and imports
  only the provider the selecting host named; the whole-tree aggregate
  contributes every provider's `body`. Adding an alternative is one directory
  plus one line here; removing it is deleting both, with no other edit in the
  tree. Each provider file is a plain module of its own.
- **`hyprland/default.nix` — the LOOK.** Wires Hyprland as the NixOS Wayland
  compositor (`programs.hyprland`, the home-manager `wayland.windowManager.
  hyprland` config, `package = null` so the binary is installed once, and
  `hyprland` as the display manager's default session, since the package also
  registers a uwsm session this lane does not enable), applies
  compositor-side livery (gaps/radius/borders/blur, the `aoide-*` layerrules,
  hyprglass, kitty rounding) at build time, and exposes the Hyprland IPC
  socket for shellbridge to consume. Selecting it is the switch; it sets no fact.
  The hyprglass block's two enable keys are the song's own
  `geometry.blurEnabled`, so a blur-off song is glassless in the bake exactly as
  it is after a stage.
- **`hyprland/behaviour.nix` — the BEHAVIOUR.** Keybinds, input devices, tiling
  layout, misc quality-of-life, behavioural window rules — everything a re-rice
  must not disturb. Its options (`aoide.hyprland.monitors`,
  `.scrollingMonitor`) stay a host-facing surface, which is why the catalogue
  keeps the name `hyprland` for this file while the look is reached through
  `compositor`.
- **`modules/dendrites/greeter.nix` — the greeter.** When the declared song
  ships `widgets/greeter.qml` (`aoide.songbook.greeter`), greetd running
  `cage` + Quickshell as its greeter user on a store tree (the lane's
  `GreeterHost.qml`, the face and the song's `icons/` from the shipped
  songbook, the dress, and the
  display manager's session wrapper with the default session's Exec — the
  default session must be named, evaluation fails otherwise; cage types in
  the host's `services.xserver.xkb` layout); otherwise `ly` on tty1 with the livery's colours, plus the
  `systemd.services.display-manager.wantedBy` line the nixpkgs ly module omits.
  Either way it authenticates before it launches that same Hyprland session.
  It is a separate lane because a host can want the compositor without a
  display-manager greeter; selecting it is the switch.

## What it consumes

Exactly the dress (`aoide.livery` — palette and component tiers, plus the
override tier through `lib/livery.nix`'s `resolve`), the structure
(`aoide.arrangement` — declared widget/surface types drive the generated
layerrules), the identity scalar `aoide.user`, and its own fact. Nothing else,
from no other module (root `AGENTS.md` house rule 5). The venue recolour is a
read: `resolve` rewrites colours equal to an overridden anchor's authored value
in one pass, with no option-system recursion, so a host with a venue set stages
and paints the recoloured look.

## How it composes

Select `compositor` for the look and `hyprland` for the behaviour — an
aggregation that wants the desktop selects both. A host with no compositor
keeps both bodies in the tree (the aggregate imports every `body`) and both
guards false; a host assembled by the constructor imports only the lanes it
selected. Enabling the behaviour without the look is harmless: home-manager
only writes `hyprland.conf` when its own hyprland module is enabled.
