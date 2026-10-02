# modules/dendrites/wallpaper

The wallpaper capability: one provider registry, and the two providers that
answer it today — the shell's own layer, and the external engine.

```
modules/dendrites/wallpaper/
├── default.nix      { providers.quickshell = ./quickshell.nix;
│                      providers.skwd-wall  = ./skwd-wall.nix; }   ← the registry
├── quickshell.nix   the shell's own layer (the default)
└── skwd-wall.nix    the external `skwd-wall` engine
```

## Named seams (what it exposes)

- **The registry** names each alternative once. The constructor
  (habit's composition) reads it before any module graph exists and imports
  only the provider the selecting host named; the whole-tree aggregate
  contributes every provider's `body`. Adding an alternative is one file plus
  one line here; removing it is deleting both, with no other edit in the tree.
  Each provider file is a lane record of its own.
- **The fact `aoide.wallpaper.provider`** (`modules/nucleus/options.nix`) is the
  name of the provider that answers here: each provider sets it to its own name
  `mkDefault`, so a host may stand a provider's config down by overriding it
  without deselecting it. `lyra` publishes it to the runtime as
  `song/stage/wallpaper-provider`; the CLI and the QML read that one word, and
  absent means `quickshell`.
- **`quickshell.nix` — the shell's own layer.** Configures nothing: the
  `aoide-wallpaper` layer is the `lyra` lane's, and this provider is the NAME of
  that arrangement. It is also the option's default, so a host with no provider
  selected at all behaves exactly as it always did.
- **`skwd-wall.nix` — the external engine.** The picker, the control daemon and
  the paper renderer from the flake input (`flake.nix`), one `skwd-walld` user
  service of this lane's own (never the upstream NixOS module, which installs
  the whole suite and a second unit of the same name), and an activation-time
  jq merge that enforces the engine config keys Aoide owns while keeping the
  user's own settings — the engine rewrites that file in place, so it is seeded
  under `aoide.root`, never a store symlink. A 1x1 fully transparent PNG,
  exported as `AOIDE_SKWD_WALL_STANDIN`, is the step-aside: the `clear` verb
  this release does not have is stood in for by applying an image that paints
  nothing, and the shell's own layer shows through it.
- **The bridge is `lyra cover sync`** (CONTRACTS.md §4), not a link between
  files: whichever provider is active, the CLI reconciles the provider with
  `song/stage/cover.json` — the staged pick while one applies to the staged
  song, nothing (the step-aside image) otherwise. The engine lane calls it from
  its unit's `ExecStartPost`, and every pick write calls it; it is the door that
  waits for a provider still coming up (~5 s), where the pick writes each make a
  single attempt.
- **Re-asserted after every activation.** `home.activation.aoideResyncSkwdWall`
  runs after `reloadSystemd` and `try-restart`s `skwd-walld.service`, so the
  unit's own `ExecStartPost` syncs with the environment the unit was built for.
  A running daemon is therefore bounced once per switch — accepted: the engine
  re-applies the staged state on the way back up, and `try-restart` no-ops when
  it is stopped, so a headless activation starts nothing.
