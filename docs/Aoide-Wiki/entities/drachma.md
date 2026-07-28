---
type: entity
created: 2026-07-26
aliases: [aoide-notes, notes package, note engine]
tags: [aoide, notes, theming, base16, node]
---

# drachma (the note engine)

`drachma` — the package formerly named `aoide-notes` — is the live-side note
engine: the standalone Node package that validates, resolves, and emits Aoide's
[[Notes]]. The token engine takes the Greek coin's name (notes are the values;
drachma mints, validates, and emits them — see [[Lexicon]]). It is the concrete
implementation of the note package described in [[Notes]]: it wraps Style
Dictionary rather than reimplementing a resolver, and it owns the authoritative
v0 schema validator that `aoide rice lint` delegates to.

*Grounded in the repo at commit f3ceadf (renamed since). It lives at
`pkgs/drachma/`, packages as `buildNpmPackage` (pname `aoide-drachma`) with a
single runtime dependency (`style-dictionary@4.3.0`), and is exposed as the
flake output `packages.drachma` and — via the overlay `lib/mkHost.nix` injects —
as the nixpkgs attr `pkgs.drachma`, the same `callPackage` path the flake uses,
so there is one source. The derivation installs the package under `libexec/` and
wraps a `drachma` launcher on PATH, pinning the exact `nodejs` so
`node_modules` resolves from any cwd.*

## Subcommands

The `drachma` binary (`src/cli.js`) has three subcommands, all reading a v0
note container (a W3C design-tokens JSON file):

- **`lint <notes.json>`** — validate against the authoritative v0 schema
  (`src/schema.js`). The nix option type in `modules/nucleus/options.nix` is a
  permissive gate; *this* is the real validator. It enforces the closed palette
  tier (`bg/fg/accent/urgent`, unknown keys rejected) and the optional component
  tier (`bar.*` / `notif.*` / `window.*`, each field `nullOr` hex), accepting
  both bare hex strings and W3C `{ $value, $type }` note objects, and treating
  `{group.name}` alias references as valid pending resolution.
- **`resolve <notes.json>`** — print the fully-resolved, flattened note set.
- **`emit <target> <notes.json>`** — run one of the three emitters.

Exit codes align with the CLI convention: `0` ok · `2` usage · `1` error.

## The three emitters

All three consume the *same* fully-resolved note set (from `src/resolve.js`), so
the three live targets can never disagree:

1. **`stage`** → `song/stage/notes.json` for [[Quickshell]]. With `--out PATH` it
   writes atomically (write to a temp file in the same dir, then rename over the
   target) so a Quickshell hot-reload never reads a torn file (the `CONTRACTS.md`
   §4 discipline). Component fallbacks are already applied, so the stage file
   carries concrete colours — Quickshell never reads `null`.
2. **`hyprctl`** → shell-ready `hyprctl keyword` lines for the compositor window
   border colours (`general:col.active_border` / `col.inactive_border`), colours
   formatted as `rgb(rrggbb)`.
3. **`osc`** → terminal OSC colour sequences (OSC 10/11/12 for fg/bg/cursor plus
   OSC 4 palette slots), mapping the v0 palette onto conventional ANSI slots.

## The resolver — wrap, don't rewrite

`src/resolve.js` hands the raw container to Style Dictionary purely to
dereference `{a.b}` aliases (via its programmatic `exportPlatform` API, no files
touched), then applies the v0 component-tier `null → palette` fallback itself.
This is the same fallback the nix facets apply independently for the baked side
([[Stylix]], compositor) — identical rules, so both fan-outs agree and preview
and adopted state cannot diverge.

## Note schema v0

The schema (`src/schema.js`, `SCHEMA_VERSION = "0"`) sits inside the W3C
design-tokens container: palette is base16-closed; the component tier is the
`bar/notif/window` groups with the fallback map declared in one place
(`COMPONENT_FALLBACK`) and shared by the resolver and validator. The build
smoke-tests all four operations (`lint`, `resolve`, `emit stage`, `emit hyprctl`,
`emit osc`) against a v0 fixture during `doInstallCheck`.

## Related

- [[Notes]]
- [[aoide-cli]]
- [[Quickshell]]
- [[Stylix]]
- [[Self-Ricing]]
- [[Codebase]]
