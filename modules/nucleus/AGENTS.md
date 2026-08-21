# AGENTS.md — modules/nucleus

Points up to `modules/AGENTS.md` for the cross-module invariants (walk
discipline, `_`-prefix shelving, the closed read whitelist) — this file
covers only what's specific to nucleus.

## Invariants

- **Discovered unconditionally — no `mkIf` guard.** Unlike a dendrite, a
  nucleus file is not opt-in per host; it applies everywhere. A new nucleus
  file is core plumbing, not a feature toggle.
- **`options.nix` is the one file every other module builds against.**
  Widening the read whitelist (`aoide.livery`/`aoide.arrangement`/
  `aoide.surfaces`) is a decision that touches root `AGENTS.md` house rule 5
  too — don't add a fourth namespace without updating both.
- **Changes here land by upstream merge, not agent edit** (root `AGENTS.md`
  house rule 1 — `song/` is the only agent-writable domain). An agent
  proposing a nucleus change writes the diff and gets it reviewed/merged
  the normal way, same as any other core-structure change.

## Extension points

- **A new piece of core plumbing** (a new always-on service, a new option
  namespace) is a new file here, following the existing pattern: a header
  comment stating scope and what it reads, `mkOption`/`mkEnableOption` as
  needed.

## Docs update required in the same commit

- This `README.md` when a new nucleus file or option namespace is added.
- `CONTRACTS.md` (livery schema) when `options.nix`'s option contract
  changes shape.
