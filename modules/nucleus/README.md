# modules/nucleus

Core plumbing, discovered and applied unconditionally on every host — no
`mkIf` guard, no per-host opt-in (unlike `dendrites`/`facets`). Every other
module builds against what nucleus declares.

## Named seams (what it exposes)

- `options.nix` — THE option contract: `aoide.livery`, `aoide.arrangement`,
  `aoide.surfaces` — the enumerated, closed set facets are allowed to read
  (root `AGENTS.md` house rule 5). Versioned in CONTRACTS.md (livery schema
  v0).
- `aoided.nix` — the orchestrator daemon service: the neutral event stream,
  default-deny-per-class subscriptions, the user-gated rebuild pipeline, the
  single audit log.
- `melete-adapter.nix` — the concrete thin-adapter exemplar on the `aoided`
  event stream.
- `shellbridge.nix` — the bidirectional bridge service: atomic JSON state
  out to `song/stage/*.json`, unix-socket commands in, Hyprland IPC
  consumed here only.
- `nix.nix` — flake-native nix settings (Aoide IS a flake; every rebuild/
  check/`aoide update` runs through the flake CLI).
- `packages.nix` — puts the `aoide`/`aoided`/`lyra` binaries on the system
  `PATH` (systemd units ExecStart absolute store paths instead).

## What it consumes

Stock NixOS/Home-Manager options only; nothing from `dendrites`/`facets`.

## How it composes

Every dendrite and facet reads `aoide.livery`/`aoide.arrangement`/
`aoide.surfaces` from here and nothing else of nucleus's internals. Nucleus
never reads a dendrite or facet back.
