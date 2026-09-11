# AGENTS.md — modules/dendrites

Points up to `modules/AGENTS.md` for cross-module invariants (aggregate
discipline, `_`-prefix shelving, flags-default-off) — this file covers only
what's specific to dendrites.

## Invariants

- **One enable toggle, default off.** `aoide.<name>.enable = false` is the
  shape every dendrite follows — shipped but inert until a host opts in.
- **Carries its own dependencies; reads no other module.** Not another
  dendrite, not a facet's internals — only `config.aoide.<name>.*` the file
  declares itself, plus stock options.
- **Draws through a bridge, never directly.** A dendrite that produces
  something visible (a notification, a status line) hands data to a
  facet-owned surface via a CLI command or stage file (root `AGENTS.md`
  corollary: Quickshell paints, it is never the capability) — it does not
  embed its own rendering.
- **`hosts/` knows dendrites; dendrites never know hosts.** No
  host-conditional logic inside a dendrite file itself.

## Extension points

- **A new dendrite**: a new `.nix` file here (or `_name.nix` while
  work-in-progress — see `_example.nix`), following the template's shape,
  plus one line in `default.nix`.
- **Splitting a tool out of a bundled dendrite** (e.g. `devtools.nix`) is
  warranted the moment a host needs to toggle it independently — see
  `claude-code.nix`'s header for the precedent.

## Docs update required in the same commit

- This `README.md` when the set of dendrites shifts materially (a new
  category of capability, not every single addition).
- `modules/AGENTS.md` is the layer above for the aggregate/shelving
  mechanics themselves — not restated here.
