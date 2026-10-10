# modules/nucleus

The contract: the option tree and the platform assertions every host carries.
`lib/aoideos.nix`'s `nucleusModule` (exported as `nixosModules.nucleus`) imports
this directory and is handed to the constructor as an `extraModules` entry, so
it reaches a host whatever the host selects. It declares options and checks
invariants; it configures nothing. Every other module builds against what it
declares.

What a host carries and may deselect is the `nucleus` aggregation
(`modules/aggregations/nucleus`), whose members are catalogue entries: the
daemon and its units, the secrets broker, the nix settings, the binaries on the
PATH, and the command line. The contract is the part no host can drop; the
aggregation is the part any host can.

## Named seams (what it exposes)

- `options.nix` — the paint half of the option contract, plus the core door
  toggles whose units live in a member dendrite: `aoide.livery`,
  `aoide.arrangement`, `aoide.surfaces` — the enumerated, closed set a paint
  lane is allowed to read (root `AGENTS.md` house rule 5). Versioned in
  CONTRACTS.md (livery schema v0). Also carries the non-paint-read option
  namespaces (`aoide.mcp`, `aoide.a2a`, `aoide.mail.adapter`, `aoide.usage`,
  `aoide.lyra`, `aoide.secrets`, `aoide.pairing` — deployment/door toggles, not
  part of the paint whitelist). And it declares the ENABLE FACTS — the seam
  between this layer and the paint lanes (CONTRACTS.md §0):
  `aoide.quickshell.enable` (a shell surface exists here), `aoide.lyra.enable`
  (the paint/rice binary is installed), `aoide.stylix.enable`,
  `aoide.compositor.enable`, `aoide.greeter.enable`, each defaulting `false`,
  plus `aoide.quickshell.config : nullOr str` — the directory a shell runs
  (`null` is the bare case: package installed, no shell service) — and
  `aoide.wallpaper.provider` (default `"quickshell"`), the one fact that names
  an ALTERNATIVE rather than a capability: which provider of the `wallpaper`
  registry paints a song's wallpaper here. The lane that owns a fact sets it
  `mkDefault true` (or, for the provider name, `mkDefault "<its own name>"`);
  every reader — a member dendrite or another lane — reads the FACT and never
  the owning lane's own option. The CORE half (`enable`, `root`, `checkout`,
  `auditLog`, `terminal`, `user`, `sessionTarget`) arrives by import from
  `pkgs/aoide/module/options.nix` — the core flake's own option contract,
  nixpkgs-only and portable to any consumer. The import is written by the
  module that closes over Aoide's own inputs (`lib/aoideos.nix`'s
  `nucleusModule`, exported as `nixosModules.nucleus`), not by a file here: a
  file in this directory cannot see the flake's inputs, and an `imports` list
  cannot read a value that `_module.args` supplies.
- `config-options.nix` — `aoide.config.{enable,settings}`, the portable runtime
  config's nix front-end. It sits beside `options.nix` and not in it because the
  `settings` type comes from `pkgs.formats.toml`; the `aoide-config` dendrite
  renders it.
- `assertions.nix` — the platform's invariant surface: the twin of the
  constructor's gate pass (habit's composition), for the failures only an
  evaluated module graph can see. It reads the identity scalar `aoide.song`
  and the lane facts and declares nothing, so it adds no option, no unit and
  no package. Today: a song named with no `lyra` lane to paint it fails the
  host's own evaluation with the taught fix.

## The nucleus aggregation

The six services that used to be nucleus files are dendrites, named in the
catalogue and members of the `nucleus` aggregation beside the command-line
members (`modules/dendrites/README.md`, "The nucleus members"):

| catalogue name   | file                                    | what it is                                             |
|------------------|-----------------------------------------|--------------------------------------------------------|
| `aoided`         | `modules/dendrites/aoided.nix`          | the doors, the mail adapter, the usage poller, the pair-watch popup, the discovery firewall carve |
| `aoide-secrets`  | `modules/dendrites/aoide-secrets.nix`   | the secrets broker and its popup watcher               |
| `melete-adapter` | `modules/dendrites/melete-adapter.nix`  | the thin adapter on the `aoided` event stream          |
| `aoide-config`   | `modules/dendrites/aoide-config.nix`    | renders `aoide.config.settings` and points `AOIDE_CONFIG` at it |
| `aoide-nix`      | `modules/dendrites/aoide-nix.nix`       | flake-native nix settings                              |
| `aoide-packages` | `modules/dendrites/aoide-packages.nix`  | the `aoide`/`lyra` binaries on the system `PATH`       |

A consumer that takes `nixosModules.nucleus` gets the contract alone and
selects these from its own aggregation (`tests/consumer/aggregations/nucleus`).

## What it consumes

Stock NixOS/Home-Manager options only; nothing from `dendrites`. The enable
facts it declares are its own — a lane sets them, which is the only way a lane's
existence reaches the contract.

## How it composes

Every dendrite reads its own options and stock ones; a paint dendrite reads
`aoide.livery`/`aoide.arrangement`/`aoide.surfaces` from here and nothing else of
the contract's internals. The contract reads back only options a lane SETS — an
enable fact it declared itself (`aoide.quickshell.enable`, `aoide.lyra.enable`,
…) or the core seam `aoide.sessionTarget` (`pkgs/aoide/module/options.nix`). In
both directions the subject is an option declared outside the lane: the contract
never reads a lane's own option, and no lane reads another lane.
