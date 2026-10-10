# modules/dendrites

Optional, additive host capabilities. A dendrite is one file (or a directory
with `default.nix`), named once in `modules/default.nix`'s catalogue, and it
self-gates on its own `aoide.<name>.enable`. Today's set spans desktop apps
(firefox, obsidian, qbittorrent, kitty), CLI tooling (git, yazi, starship,
mcfly, nh), agent and desktop AI tooling (claude-code, eidolon, kimi-code,
pi-coding-agent, OpenAI Codex + ChatGPT), local model-serving tooling
(inference: Ollama + llama.cpp), system services (dunst, networkmanager,
tailscale, audio), and **the paint lanes** — what makes a machine paint at all:
`compositor` (with its `hyprland` provider), `greeter`, `stylix`, `quickshell`
(the shell runtime: the package and the one `aoide-quickshell` service), `lyra`
(the Quickshell surface, its shellbridge, and the song-gated deploy half) and
`wallpaper` (the capability that owns who paints a song's wallpaper, with the
shell's own layer and the external engine as its two providers). A paint lane
reads only what root `AGENTS.md` house rule 5 lists; its
config is guarded on a FACT that the nucleus contract declares rather than on an option it
declares itself, so a consumer can ask "is there a shell here?" without reading
the lane that made one.

## The shape (CONTRACTS.md §2)

Each file is a plain module. Its own settings are its system half; what it sets
under `habit.home` is its home half, a Home Manager module for each user it
reaches:

```
modules/dendrites/kitty.nix
└── { config, lib, ... }: {
      options.aoide.kitty.enable = …;
      config = mkMerge [
        { aoide.kitty.enable = mkDefault true; }       # selecting it turns it on
        (mkIf config.aoide.kitty.enable {
          habit.home = { … };                           # the user's kitty
        })
      ];
    }
```

Selecting a dendrite imports its file; an unselected one is never read. A
dendrite carries its OWN dependencies under its guard: `stylix` imports the
Stylix NixOS module only when it is present, so no other file has to know the
dendrite exists. A condition does not cover a home half's `imports` or
`options` (habit refuses one that does): `neovim.nix` states its `imports` in an
unconditional `habit.home`, and `lyra/default.nix` writes its home half as
`config`.

## The shell lane (`quickshell.nix`) and the seam with `lyra`

The shell runtime is two files, split by what each one knows:

```
quickshell.nix    the PACKAGE, and the one `aoide-quickshell` user service.
                  Knows no song, deploys nothing: it starts a shell on the
                  directory `aoide.quickshell.config` names and stops there.
lyra/             the SURFACE. Builds the QML tree from `pkgs/lyra-shell` plus
                  the built-in songs' `widgets/`, deploys it, seeds the stage,
                  restarts the rice, watches it (healthcheck), owns shellbridge
                  and the `aoide.surfaces` registry.
```

They never name each other. `lyra` sets the fact `aoide.quickshell.config` to
its deployed runtime root (`$AOIDE_ROOT/run/qml`); the shell lane reads that
fact, owns `aoide.quickshell.enable` and starts the service. Each side is
removable without a trace on the other, and the shell keeps running the day a
different lane (or a host) supplies the directory instead.

**The two allowed shapes of Quickshell without lyra** — no song, no QML tree,
no rice binary:

| Shape | How | What runs |
|---|---|---|
| own config | the host sets `aoide.quickshell.config` to its own config directory (a store path or a host path) | the package, plus `aoide-quickshell` on that directory, plus the graphical-session anchor `aoided` needs |
| bare | nothing sets it (`null`, the default) | the package only: no service, no session claim |

The service is gated on the config, never on `aoide.song` — a shell that paints
a host's own config has no song and starts exactly the same way. What is
song-gated is `lyra`'s half: deploy, seed, restart.

## The wallpaper capability (`wallpaper/`) and the seam with `lyra`

Who paints a song's wallpaper is a HOST choice, the way the compositor is — a
provider registry, not a song's field:

```
wallpaper/
├── default.nix     { providers.quickshell = ./quickshell.nix;
│                     providers.skwd-wall  = ./skwd-wall.nix; }   ← the registry
├── quickshell.nix  the shell's own layer: the default. It configures NOTHING —
│                   the layer is `lyra`'s — it NAMES the arrangement.
└── skwd-wall.nix   the external engine: its three packages, the `skwd-walld`
                    user service, and the config merge that keeps the engine's
                    own theming off and its library inside `aoide.root`.
```

Both providers set the fact `aoide.wallpaper.provider` to their own name, so
exactly one answer exists on a host; `lyra` publishes that one word as
`song/stage/wallpaper-provider`, and both readers — the CLI
(`aoide-song`'s `wallpaper_provider`) and `LiveryState.qml` — read the same file.
Absent means the shell's own layer.

They never name each other either. The bridge is the CLI both sides already
have: `lyra cover sync` (CONTRACTS.md §4) makes the external provider agree with
the stage files — the staged pick while one applies, the step-aside image when
none does — and it is what the engine's lane runs after every daemon start and
what every pick write runs. Because "applies" is decided per STAGED SONG, a
song switch steps the engine aside with no extra call: the pick simply stops
applying, and the shell's board is back.

Nothing here is declared in `aoide.surfaces`: `wallpaper.owner` stays
`quickshell`, because the shell's surface stays mapped (transparent) while an
external provider has a pick on screen; the provider's own paper surface is the
compositor's, not an `aoide-<slot>`.

## The nucleus members

Six dendrites are members of the `nucleus` aggregation
(`modules/aggregations/nucleus`) beside the command-line members, and are what a
host carries of the core services. They are plain modules like the rest, gated on
the contract's `aoide.enable` and their own door flags rather than on a flag of
their own, so none of them adds an option. The option tree they act on is the
contract in `modules/nucleus`.

- `aoided.nix` — carries only the AoideOS deltas for the `aoided` unit
  (`pkgs/aoide/module/aoided.nix` owns the unit itself, its tmpfiles rules, and
  the core session variables): the lyra-gated `AOIDE_SONG_TEMPLATES` session
  variable, and nothing else paint-shaped. `aoide.sessionTarget` is set by the
  lane that brings a graphical session up (the `quickshell` lane, which names the
  session when it has a shell config to run), so this file reads no lane's
  option either. Below that: every door (mcp, a2a, pair-watch) the daemon's
  event stream serves, the mail adapter (`aoide-mail-adapter`: `aoide mail serve`
  on `127.0.0.1:aoide.mail.adapter.port`, off by default, the one unit a relay
  turns on and the port its TLS-terminating front targets), the
  discovery-advertisement firewall carve (`networking.firewall.allowedUDPPorts`
  whenever `aoide.a2a.discoveryAdvertise` is on — the stock firewall trusts only
  `lo`, and an advertisement never reaches a listening socket on a real
  interface without it), and the usage poller. It also declares a
  graphical-session USER unit, `aoide-pair-watch.service`, running `aoide pair
  watch --popup`, gated on `aoide.a2a.enable && aoide.a2a.pairingPopup` (the
  last DEFAULT FALSE). NOT gated on `aoide.quickshell.enable`: this unit needs a
  graphical session and a dialog binary, and that fact says only that Aoide
  paints a shell here — a host painted by something else has the session, takes
  the zenity path, and would otherwise be locked out for an unrelated reason.
  `quickshell` on the unit's `path` follows `aoide.lyra.enable`, and the popup
  prefers `lyra pair ask` / `lyra pair confirm` over their zenity equivalents,
  the same way the secrets watcher prefers `lyra secrets ask`.
- `melete-adapter.nix` — the concrete thin-adapter exemplar on the `aoided`
  event stream. It orders itself against `aoided.service`, which the core module
  defines, so it evaluates with or without the `aoided` dendrite.
- `aoide-secrets.nix` — the secrets broker's deployment: its own system user
  `aoide-secrets` + groups `aoide-secrets`/`aoide-secrets-access`, and a SYSTEM
  `aoide-secrets-serve.service` (own uid, `/var/lib` home) running `aoide
  secrets serve`. Gated on `aoide.secrets.enable` (default `false`);
  `aoide.secrets.members` names who joins `aoide-secrets-access`. The broker
  binary itself stays nix-independent — see `pkgs/aoide/crates/secrets/README.md`'s
  "Deployment" section for the non-nix install path this module mirrors. Also
  declares a graphical-session USER unit, `aoide-secrets-watch.service`, running
  `aoide secrets watch --popup`, gated additionally on the host having a
  graphical session (`aoide.sessionTarget == "graphical-session.target"`), since
  the popup is a desktop surface belonging to the logged-in operator, never the
  secrets-uid broker.
- `aoide-config.nix` — nix as ONE authoring front-end for the PORTABLE runtime
  config (`$AOIDE_ROOT/config.toml`, CONTRACTS.md §4's own subsection). It
  renders `aoide.config.settings` (declared in the contract's
  `config-options.nix`) through `pkgs.formats.toml` to a read-only store path and
  points `AOIDE_CONFIG` at it — for interactive shells
  (`environment.sessionVariables`) and for every aoide user unit at once (the
  user manager's own `DefaultEnvironment`). Nix POINTS, never copies. Gated on
  `aoide.config.enable`, default `false`, and whole-file-or-nothing: enabled, nix
  owns the entire config and the CLI's own write refuses with a taught error
  naming this option.
- `aoide-nix.nix` — flake-native nix settings (Aoide IS a flake; every rebuild/
  check/`aoide update` runs through the flake CLI).
- `aoide-packages.nix` — puts the `aoide`/`lyra` binaries on the system `PATH`
  (systemd units ExecStart absolute store paths instead).

A consumer that takes `nixosModules.nucleus` gets the contract and none of these;
it selects them by name from its own aggregation.

## Named seams (what it exposes)

- One file per capability, each declaring `aoide.<name>.enable` (default
  `false`) and carrying its own package/service dependencies. A paint lane
  instead guards on the fact of the same name, declared once in
  `modules/nucleus/options.nix`.
- A capability with ALTERNATIVES is a provider registry: a directory whose
  `default.nix` is `{ providers.<p> = <path>; }`, each provider a plain
  module of its own (see `compositor/README.md`). The catalogue names the directory.
- `_example.nix` — the checked-in template: shelved by its `_` prefix
  (uncatalogued, so nothing imports it), showing the plain-module shape and the
  enable-with-one-line convention.
- A dendrite that draws (e.g. `dunst.nix`'s notification popups) hands off
  to a render surface via a bridge (a CLI command, a stage file) rather
  than drawing itself — `dunst.nix` pipes through `aoide herald push` into
  `state/stage/herald.json`, which the Quickshell herald widget reads.

## What it consumes

`openai.nix` installs Codex and the local `pkgs/chatgpt-linux` package.
`aoide.openai.enable` enables both tools. ChatGPT uses the official Linux
release pinned by version and hash, with its bundled desktop runtime.

`inference.nix` installs plain `ollama` and `llama-cpp` — CPU/RAM inference,
no GPU override. Neither package needs `allowUnfree` (both MIT).

`config.aoide.*` options it declares itself, plus stock NixOS/Home-Manager
options. A dendrite reads no other module's internals — not even another
dendrite's. A paint dendrite reads the dress (`aoide.livery`), the structure
(`aoide.arrangement`), the surface registry (`aoide.surfaces`), the identity
scalars and its own fact; nothing else (root `AGENTS.md` house rule 5).

## How it composes

A host selects a dendrite by name — one line in `hosts/<host>/default.nix`
(`aoide.<name>.enable = true`) in the full tree, or the same name in an
aggregation's `members` once the constructor assembles the host. Dendrites
know nothing about hosts; hosts know dendrites. A tool graduates from a shared
file (e.g. `devtools.nix`) into its own dendrite the moment a host needs to
toggle it independently. The paint lanes with more than one file carry their own
directories and charters (`compositor/README.md`, `lyra/README.md`); the
one-file lanes — `greeter.nix`, `stylix.nix`, `quickshell.nix` — are charted
here, above.
