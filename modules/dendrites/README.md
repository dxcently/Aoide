# modules/dendrites

Optional, additive host capabilities. A dendrite is one file (or a directory
with `default.nix`), named once in `modules/default.nix`'s catalogue, and it
self-gates on its own `aoide.<name>.enable`. Today's set spans desktop apps
(firefox, obsidian, qbittorrent, kitty), CLI tooling (git, yazi, starship,
mcfly, nh), agent and desktop AI tooling (claude-code, eidolon, kimi-code,
pi-coding-agent, OpenAI Codex + ChatGPT), local model-serving tooling
(inference: Ollama + llama.cpp), system services (dunst, networkmanager,
audio), and **the paint lanes** — what makes a machine paint at all:
`compositor` (with its `hyprland` provider), `greeter`, `stylix`, `lyra`
(the Quickshell surface and its shellbridge). A paint lane reads only what root
`AGENTS.md` house rule 5 lists; its `body` is guarded on a FACT that nucleus
declares rather than on an option it declares itself, so a consumer can ask
"is there a shell here?" without reading the lane that made one.

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
toggle it independently. The paint lanes have their own directories with their
own charters: `compositor/README.md`, `lyra/README.md`.
