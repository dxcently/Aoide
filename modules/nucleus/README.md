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

- `options.nix` — the paint half of the option contract: `aoide.theme`,
  `aoide.layout`, `aoide.surfaces` — the enumerated, closed set a paint
  lane is allowed to read (root `AGENTS.md` house rule 5). Versioned in
  CONTRACTS.md (theme schema v0). The door and secrets settings are not declared
  here: each lives in the member dendrite that reads it. And it declares the ENABLE FACTS — the seam
  between this layer and the paint lanes (CONTRACTS.md §0):
  `aoide.quickshell.enable` (a shell surface exists here), `aoide.strillix.enable`
  (the paint/rice binary is installed), `aoide.stylix.enable`, each a fact defaulting `false`,
  plus `aoide.quickshell.config : nullOr str` — the directory a shell runs
  (`null` is the bare case: package installed, no shell service) — and
  `aoide.wallpaper.provider` (default `"quickshell"`), the one fact that names
  an ALTERNATIVE rather than a capability: which provider of the `wallpaper`
  registry paints a song's wallpaper here. Beside the facts it declares
  `aoide.components` — `attrsOf { provider : str; actions : attrsOf (listOf
  str) }`, keyed by component slot — which each slot's chosen provider writes
  for itself; it is a contract option rather than the strillix runtime's because
  a consumer's own provider writes it on a host with no runtime. The dendrite that owns a fact sets it
  `true`. A host write that takes effect fails eval (`options.nix`'s `mkFact`:
  a plain or `mkForce` write, or any write with no owner selected); a
  `mkDefault` the owner outranks is dropped, not refused. Every reader — a member
  dendrite or another lane — reads the FACT and never the owning lane's own option. The CORE half (`enable`, `root`, `checkout`,
  `auditLog`, `terminal`, `user`, `sessionTarget`) arrives by import from
  `pkgs/aoide/module/options.nix` — the core flake's own option contract,
  nixpkgs-only and portable to any consumer. The import is written by the
  module that closes over Aoide's own inputs (`lib/aoideos.nix`'s
  `nucleusModule`, exported as `nixosModules.nucleus`), not by a file here: a
  file in this directory cannot see the flake's inputs, and an `imports` list
  cannot read a value that `_module.args` supplies.

- `assertions.nix` — the platform's invariant surface: the twin of the
  constructor's gate pass (habit's composition), for the failures only an
  evaluated module graph can see. It reads the identity scalar `aoide.rice`
  and the lane facts and declares nothing, so it adds no option, no unit and
  no package. Today: a song named with no `strillix` lane to paint it fails the
  host's own evaluation with the taught fix, and forcing the three facts fails
  a host write that takes effect, whether or not a lane reads that fact.

## The nucleus aggregation

The four services are dendrites, named in the
catalogue and members of the `nucleus` aggregation beside the command-line
members (`modules/dendrites/README.md`, "The core-service dendrites"):

| catalogue name   | file                                    | what it is                                             |
|------------------|-----------------------------------------|--------------------------------------------------------|
| `aoided`         | `modules/dendrites/aoided.nix`          | the AoideOS deltas on the core `aoided.service` (which `aoide.enable` alone provides): the a2a/mcp/mail doors, the pair-watch popup, the usage poller and timer, the discovery firewall carve, `STRILLIX_TEMPLATES` |
| `melete-adapter` | `modules/dendrites/melete-adapter.nix`  | the thin adapter on the `aoided` event stream          |
| `aoide-nix`      | `modules/dendrites/aoide-nix.nix`       | flake-native nix settings                              |
| `aoide-packages` | `modules/dendrites/aoide-packages.nix`  | the `aoide`/`strillix` binaries on the system `PATH`       |

`aoide-secrets` (the secrets broker and its popup watcher) and `aoide-config`
(renders `aoide.config.settings` and points `AOIDE_CONFIG` at it) are catalogue
dendrites a host selects by name (`habit.dendrites.<name>.enable = true`); the
nucleus aggregation does not carry them.

A consumer that takes `nixosModules.nucleus` gets the contract alone and
selects these from its own aggregation (`tests/consumer/aggregations/nucleus`).

## What it consumes

Stock NixOS/Home-Manager options only; nothing from `dendrites`. The enable
facts it declares are its own — a lane sets them, which is the only way a lane's
existence reaches the contract.

## How it composes

Every dendrite reads its own options and stock ones; a paint dendrite reads
`aoide.theme`/`aoide.layout`/`aoide.surfaces` from here and nothing else of
the contract's internals. The contract reads back only options a lane SETS — an
enable fact it declared itself (`aoide.quickshell.enable`, `aoide.strillix.enable`,
…) or the core seam `aoide.sessionTarget` (`pkgs/aoide/module/options.nix`). In
both directions the subject is an option declared outside the lane: the contract
never reads a lane's own option, and no lane reads another lane.
