# pkgs/strillix-rices

The shipped strillix runtime data: what `strillix` reads on a machine with no Aoide
checkout.

## What it is

Three payloads under `$out/share/strillix/`, one per consumer:

| path | what reads it |
|---|---|
| `rices/` — the rice folders this instance ships, plus `manifest.json`, `registry.json` and (when a host selected rices) `builtin.json` | `aoide_storage::fs::rice_templates_dir`, through `STRILLIX_TEMPLATES` or the exe-sibling `../share/strillix/rices` |
| `nix/` — the OFFLINE generator: `manifest.nix` + copies of `lib/rices.nix` and `lib/rice.nix` + the locked nixpkgs `lib/` (tests dropped) | `strillix-rice::widgets`'s `plan_stage`, which evaluates `nix/manifest.nix` with a plain `nix-instantiate --eval --strict --json` over the machine's own rices |
| `aoide-options.json` — `lib/options.nix`'s output | `strillix`'s `onboard`, through `fs::strillix_share_dir` (this dir) |

`strillix-rice` and `strillix` read it. The installed strillix
(`modules/dendrites/strillix/package.nix`) names the `rices/` path as its default, and
the strillix dendrite's units that exec `strillix` spell the same path.

## Knobs

| argument | default | what it does |
|---|---|---|
| `rices` | `null` | `null` ships every discovered rice; a list ships those folders |
| `builtin` | `null` | `{ declared, rices, packages }`, written to `builtin.json`; `null` writes none |
| `rices` | `../../rices` | the directory those names live in |
| `riceDirs` | `null` | name to rice directory, when already discovered (a merged rices's registry); `null` discovers `rices`. Reading `rices` itself at eval would make a merged rices import-from-derivation |

The flake's own `packages.strillix-rices` uses the defaults. The `strillix` lane
overrides both per host (`lib/rices.nix`'s `builtIn`), which is what makes the
package the byte-level meaning of "built in": a host that performs one rice ships
one rice's folders and records the selection in `builtin.json`.

`aoideOptions` is a third, REQUIRED argument — the `aoide.*` option doc list,
which `callPackage` cannot derive (it comes from the flake's `inputs`). It
arrives by name from each build context: the flake's `packages`/`checks`
through `lib/pkgs.nix`'s `extra`, and a host through the strillix lane's own
`prev.callPackage`. A context that forgets it fails loudly at eval.

## The `nix/` payload — one generator, shipped

`nix/manifest.nix` is a thin entry (`{ rices }: { manifest; registry;
packages; }`) over `nix/rices.nix` and `nix/rice.nix`, which are COPIES of
`lib/rices.nix` and `lib/rice.nix` made at build time, and `nix/lib/` is the
locked nixpkgs `lib/`. So the machine evaluates the same generator this build
deployed — no second implementation to drift, and no subset that a rice's
`_widgets/` shelf could silently outgrow.

`packages` is per rice and CLOSED UNDER BORROWS: a rice whose slot borrows
another rice's body needs that lender's packages too (`lib/rices.nix`'s
`builtIn`), which is exactly the arithmetic the host's `builtin.json.packages`
carries — the two sides of `strx rice stage`'s needs-check.

## What a derivation reads — the part that is not tidiness

Each shipped rice enters the build as **its own store path**
(`builtins.path { path = <rice>; name = "strillix-rice-<rice>"; }`), never as a
subdirectory of the whole `rices` tree, and the baked
`manifest.json`/`registry.json` are `builtins.toFile` over the shipped set only.

That is a contract with every host, not a micro-optimisation. This package's
store path is a STRING in the installed strillix (`modules/dendrites/strillix/package.nix`,
its `STRILLIX_TEMPLATES` default), in the units that spell it, and in the
`aoideSeedRices` activation entry, so the system derivations that name it move with
it — which means anything that rehashes this derivation moves the host's
**toplevel drvPath** even when no configuration changed. While the build read the rices directory,
that was true of every rice in the repo: editing a rice a host does not even
build in moved that host's system. Now it is true only of the rices it ships.

## Consumers

- a NixOS host, through the `strillix` lane's override (above);
- the flake's `packages.strillix-rices`, for a consumer that wants the whole
  rices without a host;
- `strx rice compose` / `strillix-rice::widgets`'s regeneration,
  `strx onboard`, and `strx rice stage`'s needs-check — all of them on a
  machine with no checkout and no flake.
