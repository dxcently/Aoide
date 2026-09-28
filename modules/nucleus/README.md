# modules/nucleus

Core plumbing, discovered and applied unconditionally on every host — no
`mkIf` guard, no per-host opt-in (unlike `dendrites`). Every other
module builds against what nucleus declares.

## Named seams (what it exposes)

- `options.nix` — the paint half of the option contract, plus the core door
  toggles whose units still live here: `aoide.livery`,
  `aoide.arrangement`, `aoide.surfaces` — the enumerated, closed set a paint
  are allowed to read (root `AGENTS.md` house rule 5). Versioned in
  CONTRACTS.md (livery schema v0). Also carries the non-paint-read option
  namespaces (`aoide.mcp`, `aoide.a2a`, `aoide.usage`, `aoide.lyra`,
  `aoide.secrets`, `aoide.pairing` — deployment/door toggles, not part of
  the paint whitelist). And it declares the ENABLE FACTS — the seam between
  this layer and the paint lanes (CONTRACTS.md §0): `aoide.quickshell.enable`
  (a shell surface exists here), `aoide.lyra.enable` (the paint/rice binary
  is installed), `aoide.stylix.enable`, `aoide.compositor.enable`,
  `aoide.greeter.enable`, each defaulting `false`, plus
  `aoide.quickshell.config : nullOr str` — the directory a shell runs
  (`null` is the bare case: package installed, no shell service) — and
  `aoide.wallpaper.provider` (default `"quickshell"`), the one fact that names
  an ALTERNATIVE rather than a capability: which provider of the `wallpaper`
  registry paints a song's wallpaper here. The lane
  that owns a fact sets it `mkDefault true` (or, for the provider name,
  `mkDefault "<its own name>"`); every reader — a nucleus file or
  another lane — reads the FACT and never the owning lane's own option. The
  CORE half (`enable`, `root`, `checkout`, `auditLog`, `terminal`, `user`,
  `sessionTarget`) arrives by import from `pkgs/aoide/module/options.nix` — the
  core flake's own option contract, nixpkgs-only and portable to any consumer.
  The import is written by the LANE that closes over Aoide's own inputs
  (`lib/aoideos.nix`'s `nucleusModule`, exported as `nixosModules.nucleus`), not
  by a file here: a file in this directory cannot see the flake's inputs, and an
  `imports` list cannot read a value that `_module.args` supplies. `aoide.config` is the one
  namespace declared elsewhere, in `config.nix` beside the rendering it
  exists for: its `settings` type comes from `pkgs.formats.toml`, so the
  option and the generator are one unit.
- `assertions.nix` — the platform's invariant surface: the twin of the
  constructor's gate pass (`lib/composition.nix`), for the failures only an
  evaluated module graph can see. It reads the identity scalar `aoide.song`
  and the lane facts and declares nothing, so it adds no option, no unit and
  no package. Today: a song named with no `lyra` lane to paint it fails the
  host's own evaluation with the taught fix.
- `aoided.nix` — now carries only the AoideOS deltas for the `aoided` unit
  (`pkgs/aoide/module/aoided.nix` owns the unit itself, its tmpfiles
  rules, and the core session variables): the lyra-gated
  `AOIDE_SONG_TEMPLATES` session variable, and nothing else paint-shaped.
  `aoide.sessionTarget` is no longer set here but by the lane that brings a
  graphical session up (the `quickshell` lane, which names the session when it
  has a shell config to run), so this file reads no
  lane's option either. Below that, still here: every door (mcp, a2a,
  pair-watch) the daemon's event stream
  serves, the discovery-advertisement firewall carve, and the usage
  poller — core *binaries* in a still-AoideOS *deployment*, migrating
  them is a later slice's work, not this one's. Also opens the LAN
  discovery advertisement's inbound UDP port
  (`networking.firewall.allowedUDPPorts`) whenever `aoide.a2a.
  discoveryAdvertise` is on — the stock firewall trusts only `lo`, and an
  advertisement never reaches a listening socket on a real interface
  without it (task #98). Declares a graphical-session USER unit,
  `aoide-pair-watch.service` (P-P5; the popup upgraded to a dialog shaped
  by pairing direction + `lyra`/zenity feature-detection at P-PV3, task
  #132), running `aoide pair watch --popup` — gated on
  `aoide.a2a.enable && aoide.a2a.pairingPopup` (the last DEFAULT FALSE, so
  no host gets the popup without asking for it). NOT gated on
  `aoide.quickshell.enable` (the shell fact), unlike its sibling below: this
  unit needs a graphical session and a dialog binary, and that fact says only
  that Aoide paints a shell here — a host painted by something else (osaka,
  core Aoide beside dxflake's Hyprland) has the session, takes the zenity
  path, and would otherwise be locked out for an unrelated reason. `quickshell` on the
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
- The shellbridge seam is not here: it belongs to the lane that paints a shell
  (`modules/dendrites/lyra/README.md`, "Named seams — the bridge"), which is
  also the lane that gates on a session existing.
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
  --popup` — gated additionally on the host having a graphical session
  (`aoide.sessionTarget == "graphical-session.target"`, the same condition
  the broker's own `zenity` package pull already uses), since the
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

Stock NixOS/Home-Manager options only; nothing from `dendrites`. The
enable facts it reads are its own, declared in `options.nix` — a lane sets
them, which is the only way a lane's existence reaches nucleus.

## How it composes

Every dendrite reads its own options and stock ones; a paint dendrite reads
`aoide.livery`/`aoide.arrangement`/`aoide.surfaces` from here and nothing else of
nucleus's internals; nucleus
reads back only options a lane SETS — an enable fact it declared itself
(`aoide.quickshell.enable`, `aoide.lyra.enable`, …) or the core seam
`aoide.sessionTarget` (`pkgs/aoide/module/options.nix`). In both directions
the subject is an option declared outside the lane: nucleus never reads a
lane's own option, and no lane reads another lane.
