# modules/nucleus

Core plumbing, discovered and applied unconditionally on every host — no
`mkIf` guard, no per-host opt-in (unlike `dendrites`/`facets`). Every other
module builds against what nucleus declares.

## Named seams (what it exposes)

- `options.nix` — the paint half of the option contract, plus the core door
  toggles whose units still live here: `aoide.livery`,
  `aoide.arrangement`, `aoide.surfaces` — the enumerated, closed set facets
  are allowed to read (root `AGENTS.md` house rule 5). Versioned in
  CONTRACTS.md (livery schema v0). Also carries the non-facet-read option
  namespaces (`aoide.mcp`, `aoide.a2a`, `aoide.usage`, `aoide.lyra`,
  `aoide.secrets`, `aoide.pairing` — deployment/door toggles, not part of
  the facet whitelist). The CORE half (`enable`, `root`, `checkout`,
  `auditLog`, `terminal`, `user`) arrives by import: one `imports = [
  inputs.aoide.nixosModules.default ]` line pulls it in from
  `pkgs/aoide/module/options.nix` — the core flake's own option contract,
  nixpkgs-only and portable to any consumer. `aoide.config` is the one
  namespace declared elsewhere, in `config.nix` beside the rendering it
  exists for: its `settings` type comes from `pkgs.formats.toml`, so the
  option and the generator are one unit.
- `aoided.nix` — the orchestrator daemon service: the neutral event stream,
  default-deny-per-class subscriptions, the user-gated rebuild pipeline, the
  single audit log. Also opens the LAN discovery advertisement's inbound
  UDP port (`networking.firewall.allowedUDPPorts`) whenever `aoide.a2a.
  discoveryAdvertise` is on — the stock firewall trusts only `lo`, and an
  advertisement never reaches a listening socket on a real interface
  without it (task #98). Declares a graphical-session USER unit,
  `aoide-pair-watch.service` (P-P5; the popup upgraded to a dialog shaped
  by pairing direction + `lyra`/zenity feature-detection at P-PV3, task
  #132), running `aoide pair watch --popup` — gated on
  `aoide.a2a.enable && aoide.a2a.pairingPopup` (the last DEFAULT FALSE, so
  no host gets the popup without asking for it). NOT gated on
  `aoide.facets.quickshell.enable`, unlike its sibling below: this unit
  needs a graphical session and a dialog binary, and the facet is Aoide's
  own SHELL — a host painted by something else (osaka, core Aoide beside
  dxflake's Hyprland) has the session, takes the zenity path, and would
  otherwise be locked out for an unrelated reason. `quickshell` on the
  unit's `path` follows `aoide.lyra.enable`, the same flag that decides
  whether `lyra` is there to spawn it at all. Otherwise the same unit shape
  `secrets.nix`'s own
  `aoide-secrets-watch.service` below holds — including its
  `lyra`/`quickshell` `path` entries and `AOIDE_RICE_BIN` env, since the
  popup now prefers `lyra pair ask` (inbound, typed-code entry) or `lyra
  pair confirm` (outbound, a single Approve/Reject over the already-known
  code) over their zenity equivalents, the same way the secrets watcher
  prefers `lyra secrets ask`.
- `melete-adapter.nix` — the concrete thin-adapter exemplar on the `aoided`
  event stream.
- `shellbridge.nix` — the bidirectional bridge service: atomic JSON state
  out to `state/stage/*.json` (CONDUCTING files — sessions.json, hooks.json;
  CONTRACTS.md §4), unix-socket commands in, Hyprland IPC consumed here
  only. Its shared `$XDG_RUNTIME_DIR/aoide` directory survives bridge stops
  and restarts (`RuntimeDirectoryPreserve=yes`): independently running
  conductors own the session sockets inside it. It also carries the polkit
  grant its power actions need: a systemd
  user unit has no logind session, so `allow_active` never applies to the
  `login1` actions and the powermenu's `systemctl reboot` would otherwise be
  refused for want of interactive auth.
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
- `config.nix` (task #135 P-C) — nix as ONE authoring front-end for the
  PORTABLE runtime config (`$AOIDE_ROOT/config.toml`, CONTRACTS.md §4's own
  subsection). Core is cargo-buildable on any Linux with no NixOS
  assumption, so a core command's configuration cannot LIVE in a NixOS
  option; this module renders `aoide.config.settings` through
  `pkgs.formats.toml` to a read-only store path and points `AOIDE_CONFIG` at
  it — for interactive shells (`environment.sessionVariables`) and for every
  aoide user unit at once (the user manager's own `DefaultEnvironment`,
  rather than an enumeration of unit names that would drift each time a unit
  is added). Nix POINTS, never copies: immutability is the provenance, and
  the two worlds never write the same file, so a rebuild cannot eat an
  `aoide config set` edit. Gated on `aoide.config.enable`, default `false`
  (same house policy as the MCP façade / A2A door / usage poller / secrets
  broker) — and whole-file-or-nothing: enabled, nix owns the entire config
  and the CLI's own write refuses with a taught error naming this option.
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
