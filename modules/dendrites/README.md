# modules/dendrites

Optional, additive host capabilities. A dendrite is one file (or a directory
with `default.nix`), named once in `modules/default.nix`'s catalogue, and it
self-gates on its own `aoide.<name>.enable`. Today's set spans desktop apps
(firefox, obsidian, qbittorrent, kitty), CLI tooling (git, yazi, starship,
mcfly, nh), agent and desktop AI tooling (claude-code, eidolon, kimi-code,
pi-coding-agent, OpenAI Codex + ChatGPT), local model-serving tooling
(inference: Ollama + llama.cpp), system services (dunst, networkmanager,
audio), and **the paint lanes** — what makes a machine paint at all:
`compositor` (with its `hyprland` provider), `greeter`, `stylix`, `quickshell`
(the shell runtime: the package and the one `aoide-quickshell` service), `lyra`
(the Quickshell surface, its shellbridge, and the song-gated deploy half) and
`wallpaper` (the capability that owns who paints a song's wallpaper, with the
shell's own layer and the external engine as its two providers). A paint lane
reads only what root `AGENTS.md` house rule 5 lists; its
`body` is guarded on a FACT that nucleus declares rather than on an option it
declares itself, so a consumer can ask "is there a shell here?" without reading
the lane that made one.

## The shape (CONTRACTS.md §2, v1)

Each file evaluates to a lane record — `body` (the module itself) plus the
lanes it answers for:

```
modules/dendrites/kitty.nix
├── body   : { config, lib, ... }: { options.aoide.kitty.enable = …; config = mkIf …; }
└── nixos  : { imports = [ body ]; config.aoide.kitty.enable = mkDefault true; }
```

Why both: a host that takes the whole tree merges every `body` (through
`modules/dendrites/default.nix`), and a host the constructor assembles
(`lib/composition.nix`) imports only the `nixos` lane of what it selected.
The two are MUTUALLY EXCLUSIVE in one module list — both import the same
`body`, so `aoide.<name>.enable` would be declared twice and nixpkgs throws
`already declared` rather than merging. Take the aggregate and select
nothing, or select through the catalogue and leave the aggregate out;
`mkNixosModules` refuses the pair by name.
Every dendrite's lane is `nixos`, because each writes its Home Manager
configuration from the NixOS side. A dendrite carries its OWN dependencies in
the lane that needs them: the `stylix` lane imports the Stylix NixOS module,
so no other file has to know the lane exists.

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

## Named seams (what it exposes)

- One file per capability, each declaring `aoide.<name>.enable` (default
  `false`) and carrying its own package/service dependencies. A paint lane
  instead guards on the fact of the same name, declared once in
  `modules/nucleus/options.nix`.
- A capability with ALTERNATIVES is a provider registry: a directory whose
  `default.nix` is `{ providers.<p> = <path>; }`, each provider a lane record
  of its own (see `compositor/README.md`). The catalogue names the directory.
- `default.nix` — this directory's whole-tree aggregate. It names no dendrite:
  its imports are DERIVED from the catalogue
  (`builtins.attrValues (import ../default.nix).catalogue`) in attribute-name
  order — the `LC_ALL=C` order the catalogue is written in — so a dendrite added
  or shelved by its catalogue line needs no edit here. A provider registry
  contributes EVERY alternative's `body`: taking the whole tree means taking
  every alternative, and the constructor is the one place exactly one is
  chosen.
- `_example.nix` — the checked-in template: shelved by its `_` prefix
  (uncatalogued, so nothing imports it), showing the v1 lane record and the
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
