---
type: entity
created: 2026-07-26
updated: 2026-07-28
aliases: [aoide-drachma, aoide-notes, Notes, notes package, note engine]
tags: [aoide, drachma, theming, base16, node]
---

# drachma — the design tokens, and the engine that mints them

`drachma` is Aoide's **design-token layer** — one name for the whole thing:
the values and the mint are one thing, named for the Greek coin. The same
word names the standalone Node package that validates, resolves, and emits
the tokens. A song authors `aoide.drachma.*`; facets read `aoide.drachma` and
nothing else; the runtime seam is `stage/drachma.json`. "Notes" survives only
as the musical image ([[Lexicon]]) — the option, the package, the schema, and
every shipped artifact are `drachma`.

## The seam between score and performance

drachma is where the frozen nix layer and the live desktop meet: values
frozen into the crystal, sounded at runtime. The container stays W3C
design-tokens format; drachma is Aoide's name for what fills it.

Every facet consumes drachma and nothing else. No module reads another
module. The coupling discipline is contractual, not polite.

## Two fan-outs, one source

```
drachma (single source)
    ├── stage/drachma.json  →  Quickshell + hyprctl + terminal OSC  (rehearsal / live)
    └── rice.nix → Stylix   →  every nix-manageable app             (recording / adopted)
```

[[Stylix]] is the baked fan-out. `rice.nix` feeds one base16 scheme into
Stylix (plus fonts, cursor, wallpaper) and Stylix themes every nix-manageable
target — GTK/Qt, terminal, editors, browser, boot. The drachma package keeps
only the live side.

Because both fan-outs derive from the same drachma values, preview state and
adopted state cannot diverge. This is the "zero drift" guarantee.

## Tier structure

| Tier | Status | Detail |
|---|---|---|
| Palette (base16) | Settled | [[Stylix]] consumes natively; no open questions |
| Semantic tier | Open (v1 design-system work) | Names meanings, survives transposition |
| Component tier | Open (v1 design-system work) | Maps semantics to specific surfaces |

The open schema question is scoped to the semantic and component tiers only.
The palette tier is closed.

## Prior art — wrap, don't rewrite

Style Dictionary and the W3C design-tokens format already provide tiered
reference resolution and multi-format emission. The package wraps these
rather than reimplementing a resolver; it owns the authoritative v0 schema
validator that `aoide rice lint` delegates to. The genuinely missing pieces
are the Aoide-specific emitters (QML/stage, hyprctl, OSC).

*It lives at `pkgs/drachma/`, packages as `buildNpmPackage` (pname
`aoide-drachma`) with a single runtime dependency (`style-dictionary@4.3.0`),
and is exposed as the flake output `packages.drachma` and — via the overlay
`lib/mkHost.nix` injects — as the nixpkgs attr `pkgs.drachma`, the same
`callPackage` path the flake uses, so there is one source. The derivation
installs the package under `libexec/` and wraps a `drachma` launcher on PATH,
pinning the exact `nodejs` so `node_modules` resolves from any cwd.*

## Subcommands

The `drachma` binary (`src/cli.js`) has three subcommands, all reading a v0
note container (a W3C design-tokens JSON file):

- **`lint <drachma.json>`** — validate against the authoritative v0 schema
  (`src/schema.js`). The nix option type in `modules/nucleus/options.nix` is a
  permissive gate; *this* is the real validator. It enforces the closed palette
  tier (`bg/fg/accent/urgent`, unknown keys rejected) and the optional component
  tier (`bar.*` / `notif.*` / `window.*`, each field `nullOr` hex), accepting
  both bare hex strings and W3C `{ $value, $type }` note objects, and treating
  `{group.name}` alias references as valid pending resolution.
- **`resolve <drachma.json>`** — print the fully-resolved, flattened note set.
- **`emit <target> <drachma.json>`** — run one of the three emitters.

Exit codes align with the CLI convention: `0` ok · `2` usage · `1` error.

## The three emitters

All three consume the *same* fully-resolved note set (from `src/resolve.js`), so
the three live targets can never disagree:

1. **`stage`** → `song/stage/drachma.json` for [[Quickshell]]. With `--out PATH` it
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

## The drachma schema v0

The schema (`src/schema.js`, `SCHEMA_VERSION = "0"`) sits inside the W3C
design-tokens container: palette is base16-closed; the component tier is the
`bar/notif/window` groups with the fallback map declared in one place
(`COMPONENT_FALLBACK`) and shared by the resolver and validator. The build
smoke-tests all four operations (`lint`, `resolve`, `emit stage`, `emit hyprctl`,
`emit osc`) against a v0 fixture during `doInstallCheck`.

The provisional v0 stands until the design-system v1 lands: palette (`bg`,
`fg`, `accent`, `urgent`) plus component overrides (`bar.*`, `notif.*`,
`window.*`). The update playbook migrates songbook modules from v0 to v1 when
v1 supersedes. Building against no schema at all was rejected.

## Stylix overlap resolution

The [[Quickshell]] facet declares the surfaces it owns; [[Stylix]] disables
derivation for those surfaces from that declaration. The flake's `checks`
assert that no surface has two owners. They fail eval if any module reads
`song/` runtime paths at build time — `stage/` can never become load-bearing
for the nix build.

## Related

- [[Self-Ricing]]
- [[Song-Vocabulary]]
- [[Snowflake-Anatomy]]
- [[aoide-cli]]
- [[Quickshell]]
- [[Stylix]]
- [[Self-Ricing]]
- [[Codebase]]
