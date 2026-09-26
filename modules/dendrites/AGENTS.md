# AGENTS.md — modules/dendrites

Points up to `modules/AGENTS.md` for cross-module invariants (the registry,
aggregate discipline, `_`-prefix shelving, flags-default-off) — this file
covers only what's specific to dendrites.

## Invariants

- **One file, two lanes' worth of structure.** A dendrite file evaluates to
  `{ body; nixos; }` (CONTRACTS.md §2). `body` is today's module: it declares
  `aoide.<name>.*` and guards its config with `aoide.<name>.enable`. `nixos`
  imports `body` and sets the flag `lib.mkDefault true`. Keep the options, the
  guard and the config on `body` — moving any of them into the lane makes the
  full tree and the constructor disagree about the same capability.
- **One enable toggle, default off.** `aoide.<name>.enable = false` is the
  shape every dendrite follows — shipped but inert until a host opts in.
- **Carries its own dependencies; reads no other module.** Not another
  dendrite, not a facet's internals — only `config.aoide.<name>.*` the file
  declares itself, plus stock options.
- **Draws through a bridge, never directly.** A dendrite that produces
  something visible (a notification, a status line) hands data to a
  render surface via a CLI command or stage file (root `AGENTS.md`
  corollary: Quickshell paints, it is never the capability) — it does not
  embed its own rendering.
- **`hosts/` knows dendrites; dendrites never know hosts.** No
  host-conditional logic inside a dendrite file itself.

## Extension points

- **A new dendrite**: a new file here following the lane-record shape, one
  line in `modules/default.nix`'s catalogue, and one line in this directory's
  `default.nix`. Author it as `_name.nix` while work-in-progress — see
  `_example.nix`, the checked-in template.
- **A capability with alternatives**: a directory here whose `default.nix` is
  `{ providers.<p> = <path>; }`; the catalogue entry is the DIRECTORY, and each
  provider file is a lane record of its own.
- **Splitting a tool out of a bundled dendrite** (e.g. `devtools.nix`) is
  warranted the moment a host needs to toggle it independently — see
  `claude-code.nix`'s header for the precedent.

## Docs update required in the same commit

- This `README.md` when the set of dendrites shifts materially (a new
  category of capability, not every single addition).
- `CONTRACTS.md` §2 when the lane-record shape itself moves.
- `modules/AGENTS.md` is the layer above for the aggregate/shelving
  mechanics themselves — not restated here.
