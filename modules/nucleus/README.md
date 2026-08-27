# modules/nucleus

Core plumbing, discovered and applied unconditionally on every host — no
`mkIf` guard, no per-host opt-in (unlike `dendrites`/`facets`). Every other
module builds against what nucleus declares.

## Named seams (what it exposes)

- `options.nix` — THE option contract: `aoide.livery`, `aoide.arrangement`,
  `aoide.surfaces` — the enumerated, closed set facets are allowed to read
  (root `AGENTS.md` house rule 5). Versioned in CONTRACTS.md (livery schema
  v0). Also carries the non-facet-read option namespaces (`aoide.mcp`,
  `aoide.a2a`, `aoide.usage`, `aoide.lyra`, `aoide.secrets` — deployment/door
  toggles, not part of the facet whitelist).
- `aoided.nix` — the orchestrator daemon service: the neutral event stream,
  default-deny-per-class subscriptions, the user-gated rebuild pipeline, the
  single audit log. Also opens the LAN discovery beacon's inbound UDP port
  (`networking.firewall.allowedUDPPorts`) whenever `aoide.a2a.
  discoveryAdvertise` is on — the stock firewall trusts only `lo`, and a
  beacon never reaches a listening socket on a real interface without it
  (task #98).
- `melete-adapter.nix` — the concrete thin-adapter exemplar on the `aoided`
  event stream.
- `shellbridge.nix` — the bidirectional bridge service: atomic JSON state
  out to `state/stage/*.json` (CONDUCTING files — sessions.json, hooks.json;
  CONTRACTS.md §4), unix-socket commands in, Hyprland IPC consumed here
  only.
- `secrets.nix` (P-V4 of Workstream SECRETS, renamed from "vault" at P-V4b)
  — the secrets broker's deployment: its own system user `aoide-secrets` +
  groups `aoide-secrets`/`aoide-secrets-access`, and a SYSTEM
  `aoide-secrets-serve.service` (own uid, `/var/lib` home — not a user
  unit) running `aoide secrets serve`. Gated on `aoide.secrets.enable`
  (default `false`); `aoide.secrets.members` names who joins
  `aoide-secrets-access`. The broker binary itself stays nix-independent —
  see `pkgs/aoide/crates/secrets/README.md`'s "Deployment" section for the
  non-nix install path this module mirrors. Also declares a graphical-session
  USER unit, `aoide-secrets-watch.service`, running `aoide secrets watch
  --popup` — gated additionally on `aoide.facets.quickshell.enable` (the same
  condition the broker's own `zenity` package pull already uses), since the
  popup is a desktop surface belonging to the logged-in operator, never the
  secrets-uid broker.
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
