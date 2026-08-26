# ONBOARD — first boot as an install script, core/lyra split intact

The design authority for the onboarding lane (implements the `onboard`
stub; absorbs two open flags: the installed binary cannot link the
skill without a checkout, and `hooks.install`'s schema summary does not
advertise the skill link). Everything in "Settled decisions" was
User-decided through the 2026-08-24 design grill (inquire ruling
2026-08-26); executors do not relitigate it. Where this document and a
brief conflict, this document wins.

## The problem

`aoide onboard` is a stub that promises "register the clone, seed
songbook, print the guide" — and a fresh box needs more than that: the
harness hooks wired, and, on a NixOS box that runs lyra, the module
options discovered and configured. Today that knowledge lives across
`hooks install`, the wiki, and the modules' option declarations; a new
user assembles it by hand. Separately, every interactive prompt in the
tree is hand-rolled (`protocol::pick`'s numbered picker, two y/N
confirms in client, secrets' hidden input), each with its own reprompt
and fallback behavior.

## Settled decisions

1. **`onboard` becomes the first-boot install-script flow.** The stub's
   summary is honored as written: register the clone, seed the
   songbook, print the guide — plus harness wiring and the nix half
   below. Golden count holds at 80; `implemented` flips (stubs 8→7).
2. **It generates AND teaches.** The nix half emits one file a user can
   read top to bottom: every configurable option with its default and a
   one-line comment. Reading the file IS the teaching. More install
   concerns join later; the file is where new knobs appear.
3. **"lyra enabled" = `protocol::bin::rice_bin()` resolves** (env →
   sibling → PATH), then a wiring-state check. Three outcomes: skip
   (no lyra — one line, nothing nix-shaped is ever spoken), configure,
   already-configured.
4. **Core delegates the nix half.** `aoide onboard` runs the shell-only
   half itself and, when `rice_bin()` resolves, delegates to
   `lyra onboard` — a registered lyra command (golden 42→43), never a
   hidden subcommand. Nix knowledge stays lyra-side; core remains
   cargo-buildable on any Linux (the hard constraint).
5. **The vars file is a generated nix module, `aoide.nix`.** Every
   `aoide.*` option the modules declare, DERIVED from the declarations
   themselves — never a hand-list — each entry spelled with its default
   and one-line description. Env knobs (`AOIDE_CONDUCT_AUTOGATE`,
   `AOIDE_TERMINAL`, `AOIDE_CORE_BIN`/`AOIDE_RICE_BIN`, discovery
   advertise) ride along as commented entries.
6. **Emit and teach, never edit.** The file lands at a stated path
   (default `./aoide.nix`, `--out` overrides); onboard prints the
   `imports = [ ./aoide.nix ]` line for the user's own flake. The
   user's flake is never touched.
7. **Harness wiring is asked, not assumed.** Onboard asks which
   harnesses to wire (the `AgentProfile` set, preselected by what is on
   PATH) and runs `hooks install` for the chosen ones. The
   `hooks.install` schema summary is updated — deliberately, as part of
   this lane — to advertise the skill link it already performs.
8. **Re-run regenerates in place.** Warn, copy the old file to
   `aoide.nix.bak` (single file, overwritten next time), regenerate.
   Nothing cleverer; new features arrive by other means.
9. **Prompts run on `inquire`** (crates.io/crates/inquire) — the first
   User-authorized break of the zero-new-deps discipline for UX
   (the second overall, after ed25519-dalek for pairing). The executor
   verifies the current version and minimal features. Adoption is
   throughout the CLI, password prompts included.
10. **`onboard` runs from a checkout.** Its own contract says "register
    the clone", and the skill link needs the repo (the package does not
    ship the skill dir — proven by the sandbox `skill_source()` probe).
    A taught refusal names the requirement when run elsewhere.

## The flow

```
aoide onboard                        (Door::Cli only, from a checkout)
  |
  |- 1. shell half (core, no nix knowledge)
  |     register the clone . seed songbook . hooks install
  |     ASK: which harnesses?   <- inquire MultiSelect, preselected by
  |         PATH presence; --harness <name> (repeatable) + --yes for
  |         non-interactive runs
  |
  |- 2. lyra probe: rice_bin() resolves?
  |     |- no  -> one line, skip the nix half entirely
  |     `- yes -> wiring-state check -> skip / configure / already-configured
  |              `- delegate: exec `lyra onboard` (registered, 42->43)
  |
  |- 3. lyra half: generate aoide.nix (decision 5), emit to --out
  |     (default ./aoide.nix), print the imports line; re-run warns
  |     and backs up to aoide.nix.bak
  |
  `- 4. print the guide
```

## Prompt substrate — wrap, don't scatter

`inquire` lands in `protocol` behind the existing seams; callers keep
one seam and no crate imports `inquire` directly except `protocol`:

- `protocol::pick::{choose, choose_many}` — backed by
  `inquire::Select`/`MultiSelect` on a tty; the piped/non-tty path
  keeps the current `BufRead` behavior byte-identical (the
  table-driven `choose_reading` tests stay the contract).
- A new `protocol::pick::confirm` — `inquire::Confirm` on a tty, the
  current stdin y/N read otherwise; `client`'s `confirm_spawn` and
  `confirm_sas` retrofit onto it.
- A new hidden-input helper — `inquire::Password` (no confirmation
  echo) on a tty; `secrets put`'s value entry and `secrets watch`'s
  TOTP entry retrofit onto it. `secrets put`'s piped-bytes contract
  (byte-identical reads, `--force` for non-interactive overwrite)
  survives untouched.

Every `--yes`/`--force` escape and every non-tty fallback is
load-bearing: agents run the identical flow non-interactively.
`inquire` stays out of `aoided` and the secrets broker — prompting is
a CLI-door concern only.

## The vars-file generator

The derivation mechanism keeps nix knowledge in nix: the flake grows
an options output (nixosOptionsDoc-style JSON over an `evalModules` of
`modules/{nucleus,facets}`, filtered to the `aoide.*` namespace), and
`lyra onboard` shells to `nix` to realize it, then formats the JSON
into the commented `aoide.nix`. A new option appears in the file
because it exists in a module — no hand-list to drift. If
`evalModules` fights option declarations that reach into `pkgs`/
`config`, the fallback is a scoped eval through `nixosSystem` with a
minimal host; the executor climbs that ladder and records which rung
held in the commit body.

## Phases

Serialized (cargo lane, one holder at a time); Sonnet executes, a
different Sonnet reviews P-I2/P-I3; pathspec commits; goldens updated
in the same commit as the change they witness.

- **P-I1 (M) inquire substrate.** Dep into `protocol` only; the three
  seam retrofits above. No schema change, goldens 80/42 unchanged.
  Gate: `choose_reading` tests green unmodified; piped `secrets put`
  round-trip byte-identical; `cargo tree` shows `inquire` in protocol
  only; behavior on `TERM=dumb` verified and stated in the commit.
- **P-I2 (M) core `onboard`.** The shell half + probe + delegate +
  guide; `--harness`/`--yes`/`--out` flags; from-a-checkout refusal;
  the `hooks.install` summary update (schema-visible, deliberate).
  Golden 80 holds, stub count 8→7. Gate: non-interactive run green in
  a scratch HOME; lyra-less box output contains no nix spelling.
- **P-I3 (M) `lyra onboard` + generator.** The registered command
  (golden 42→43), the flake options output, the aoide.nix formatter,
  re-run warn+backup. Gate: generated file evaluates (`nix eval` a
  host importing it), every `aoide.*` option present, re-run produces
  the .bak.
- **P-I4 (S) docs.** Guide text, wiki onboarding/Agent-Interface
  pages, CONTRACTS if any schema-visible text moved. README/AGENTS of
  touched dirs ride each phase's commit as usual; this phase is the
  wiki sweep only.

## Deferred

More install-script concerns (systemd enablement checks, peer-mesh
bootstrap, secrets-broker setup) — the vars file is where their knobs
will surface. Onboard re-run sophistication beyond warn+backup.
Shipping the skill dir in the package (the from-a-checkout contract
covers today's need).
