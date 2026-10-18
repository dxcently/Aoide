# pkgs/strillix-rices

The shipped strillix runtime data: what `strillix` reads on a machine with no Aoide
checkout.

## What it is

Three payloads under `$out/share/strillix/`, one per consumer:

| path | what reads it |
|---|---|
| `rices/` — the song folders this instance ships, plus `manifest.json`, `registry.json` and (when a host selected songs) `builtin.json` | `aoide_storage::fs::song_templates_dir`, through `STRILLIX_TEMPLATES` or the exe-sibling `../share/strillix/rices` |
| `nix/` — the OFFLINE generator: `manifest.nix` + copies of `lib/rices.nix` and `lib/song.nix` + the locked nixpkgs `lib/` (tests dropped) | `strillix-rice::widgets`'s `plan_stage`, which evaluates `nix/manifest.nix` with a plain `nix-instantiate --eval --strict --json` over the machine's own rices |
| `aoide-options.json` — `lib/options.nix`'s output | `strillix`'s `onboard`, through `fs::strillix_share_dir` (this dir) |

Only `strillix-rice`/`strillix` read any of it, and only `strillix` links them, so
the package is wired onto the units whose process actually execs `strillix`.

## Knobs

| argument | default | what it does |
|---|---|---|
| `songs` | `null` | `null` ships every discovered song; a list ships those folders |
| `builtin` | `null` | `{ declared, songs, packages }`, written to `builtin.json`; `null` writes none |
| `rices` | `../../rices` | the directory those names live in |
| `songDirs` | `null` | name to song directory, when already discovered (a merged rices's registry); `null` discovers `rices`. Reading `rices` itself at eval would make a merged rices import-from-derivation |

The flake's own `packages.strillix-rices` uses the defaults. The `strillix` lane
overrides both per host (`lib/rices.nix`'s `builtIn`), which is what makes the
package the byte-level meaning of "built in": a host that performs one song ships
one song's folders and records the selection in `builtin.json`.

`aoideOptions` is a third, REQUIRED argument — the `aoide.*` option doc list,
which `callPackage` cannot derive (it comes from the flake's `inputs`). It
arrives by name from each build context: the flake's `packages`/`checks`
through `lib/pkgs.nix`'s `extra`, and a host through the strillix lane's own
`prev.callPackage`. A context that forgets it fails loudly at eval.

## The `nix/` payload — one generator, shipped

`nix/manifest.nix` is a thin entry (`{ rices }: { manifest; registry;
packages; }`) over `nix/rices.nix` and `nix/song.nix`, which are COPIES of
`lib/rices.nix` and `lib/song.nix` made at build time, and `nix/lib/` is the
locked nixpkgs `lib/`. So the machine evaluates the same generator this build
deployed — no second implementation to drift, and no subset that a song's
`_widgets/` shelf could silently outgrow.

`packages` is per song and CLOSED UNDER BORROWS: a song whose slot borrows
another song's body needs that lender's packages too (`lib/rices.nix`'s
`builtIn`), which is exactly the arithmetic the host's `builtin.json.packages`
carries — the two sides of `aoide rice stage`'s needs-check.

## What a derivation reads — the part that is not tidiness

Each shipped song enters the build as **its own store path**
(`builtins.path { path = <song>; name = "strillix-song-<song>"; }`), never as a
subdirectory of the whole `rices` tree, and the baked
`manifest.json`/`registry.json` are `builtins.toFile` over the shipped set only.

That is a contract with every host, not a micro-optimisation. This package's
store path is a STRING in a host's `environment.sessionVariables`
(`STRILLIX_TEMPLATES`) and in its `aoideSeedRices` activation entry, so both
the `etc` and `activate` derivations of a NixOS system name it — which means
anything that rehashes this derivation moves the host's **toplevel drvPath**
even when no configuration changed. While the build read the rices directory,
that was true of every song in the repo: editing a song a host does not even
build in moved that host's system. Now it is true only of the songs it ships.

## Consumers

- a NixOS host, through the `strillix` lane's override (above);
- the flake's `packages.strillix-rices`, for a consumer that wants the whole
  rices without a host;
- `strx rice compose` / `strillix-rice::widgets`'s regeneration,
  `strx onboard`, and `aoide rice stage`'s needs-check — all of them on a
  machine with no checkout and no flake.
