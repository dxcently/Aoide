# modules/dendrites

Optional, additive host capabilities — every dendrite self-gates on its own
`aoide.<name>.enable` and is discovered by `lib/walk.nix`'s recursive walk,
never an import list. Today's set spans desktop apps (firefox, obsidian,
kitty), CLI tooling (git, yazi, starship, mcfly, nh), agent CLIs
(claude-code, kimi-code, pi-coding-agent, codex), and system services (dunst,
networkmanager, audio).

## Named seams (what it exposes)

- One file per capability, each declaring `aoide.<name>.enable` (default
  `false`) and carrying its own package/service dependencies.
- `_example.nix` — the checked-in template: shelved by its `_` prefix
  (invisible to the walker), documents the v0 dendrite shape and the
  enable-with-one-line-in-`hosts/`convention.
- A dendrite that draws (e.g. `dunst.nix`'s notification popups) hands off
  to a facet-owned surface via a bridge (a CLI command, a stage file) rather
  than drawing itself — `dunst.nix` pipes through `aoide herald push` into
  `state/stage/herald.json`, which the Quickshell herald widget reads.

## What it consumes

`config.aoide.*` options it declares itself, plus stock NixOS/Home-Manager
options. A dendrite reads no other module's internals — not even another
dendrite's.

## How it composes

`hosts/<host>/default.nix` opts a host into a dendrite with one line
(`aoide.<name>.enable = true`). Dendrites know nothing about hosts; hosts
know dendrites. A tool graduates from a shared file (e.g. `devtools.nix`)
into its own dendrite the moment a host needs to toggle it independently.
