# AGENTS.md — aoide-cli

## Invariants

- **This is core. It never depends on `aoide-song`/`aoide-screen`.** Adding
  either dependency here reintroduces the graphical surface P-A5 removed
  (golden 87 → 48) — a headless box must be able to build/run this crate
  with no wayland/image/nix weight anywhere in its tree.
- **`commands::all()`'s order is byte-stable.** It reproduces the historical
  `schema.rs` table order; `schema --json` and the MCP tool list must never
  reorder. Append new `register()` calls, never reorder existing ones — see
  `pkgs/aoide/crates/AGENTS.md`.
- **`meta`/`stubs`/`onboard`/`infra` stay here, not in a domain crate.** They
  exist because they read the fully-ASSEMBLED registry (tool count, schema
  dump, `onboard`'s own call into `hooks.install` and the closing guide) — a
  domain crate can't do that without depending on this crate, which would
  invert the DAG.
- **Nix-independent.** No nix shell-outs, no NixOS assumption, anywhere in
  this crate or what it depends on (root `AGENTS.md`, "core is
  nix-independent"). Only `lyra` may be nix-dependent.
- **The audit copy of an outcome is not the outcome.** `dispatch` writes one
  line per dispatch for both doors, and its message is deliberately NOT
  always `Outcome.message`: the pairing ceremony's text carries a
  confirmation/reply code (and whatever an operator typed at that door), and
  `$AOIDE_ROOT/log` is read back (the conductor's LOG panel tails it). Keep
  `audit_message` keyed on the command PATH, never on the message's shape —
  no `NNN-NNN` scan, no "looks like a code" guess: a malformed or re-spaced
  code must be withheld exactly like a well-formed one, and a typed id is no
  different. Never redact by mutating `Outcome.message`; only the stored copy
  is withheld.

## Extension points

- **A new root-coupled command** (one that must read the assembled
  registry) adds a case to `meta`/`infra`, or its own dedicated module
  (`onboard`'s precedent) when the command is substantial enough to warrant
  one; anything else belongs in its domain crate's own `commands` module
  instead.
- **A new special-cased command** (bypassing the generic `Outcome` envelope)
  extends the `special` closure passed to `aoide_protocol::door::run` in
  `run_cli`.

## Docs update required in the same commit

- This `README.md` when the command count, a root-coupled group, or a
  special-cased command changes.
- The golden snapshot in `registry.rs` when the command-path set changes.
- `docs/architecture/PACKAGE-LAYOUT.md`/`CONTRACTS.md §3` when the
  core/lyra split itself shifts.
