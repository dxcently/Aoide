---
type: entity
created: 2026-07-26
updated: 2026-08-14
aliases: [aoide-notes, Notes, notes package, note engine]
tags: [aoide, livery, theming, base16]
---

# livery — the design tokens, and the engine that stamps them

`livery` is Aoide's **design-token layer** — one name for the whole thing:
the values and the act of stamping them are one thing. The same word names
the native engine that validates, resolves, and emits the tokens. A song
authors `aoide.livery.*`; facets read `aoide.livery` and nothing else; the
runtime seam is `stage/livery.json`. "Notes" survives only as the musical
image ([[Lexicon]]) — the option, the schema, and every shipped artifact are
`livery`.

The livery merge (LIVERY-MERGE.md) ported the
standalone Node engine into native Rust inside `crates/song/src/livery/`, and
the engine took the name **livery** across the whole surface — option
namespace, songbook data files, and the live stage contract — in one pass
(the previous option namespace kept evaluating via a
`mkRenamedOptionModule` alias until Phase 4 closed the transition window).

## The seam between score and performance

livery is where the frozen nix layer and the live desktop meet: values
frozen into the crystal, sounded at runtime. The container stays W3C
design-tokens format; livery is Aoide's name for what fills it.

**Why a livery.** The industry term for this layer is *design tokens*, and a
token stands for a single value. But the engine dresses *every*
surface — terminal, compositor, GTK, the Quickshell stage, any config file —
in the one song's identity, and a token does not clothe a stage. A **livery**
is the single set of house colours a whole retinue wears in unison, so a
servant, a ship, and a herald are read at a glance as one household's. One
word for the values *and* the wearing of them. See [[Lexicon#Why the seam is
a livery]].

Every facet consumes livery and nothing else. No module reads another
module. The coupling discipline is contractual, not polite.

## Two fan-outs, one source

```
livery (single source)
    ├── stage/livery.json  →  Quickshell + hyprctl + terminal OSC  (rehearsal / live)
    └── rice.nix → Stylix   →  every nix-manageable app             (recording / adopted)
```

[[Stylix]] is the baked fan-out. `rice.nix` feeds one base16 scheme into
Stylix (plus fonts, cursor, wallpaper) and Stylix themes every nix-manageable
target — GTK/Qt, terminal, editors, browser, boot. The engine keeps only the
live side.

Because both fan-outs derive from the same livery values, staged state and
adopted state cannot diverge. This is the "zero drift" guarantee. Beyond
`rice stage`/`cover set` writing `stage/livery.json` live (while `rice mode
staging` allows it — see [[Self-Ricing#Staging vs Declarative Mode]]), the quickshell
facet's `home.activation.aoideSeedStage` reasserts it from the active song's
committed `song/songbook/<song>/livery.json` on every activation (see
[[Codebase#Runtime contracts (socket + stage files)]]), so a freshly booted
host carries a correct stage twin even before `rice stage` ever runs.
`stage/livery.json` is canonical; the legacy-mirror write and the fallback
reads were dropped when Phase 4 closed the transition window
(LIVERY-MERGE.md).

## Tier structure

| Tier | Status | Detail |
|---|---|---|
| Palette (base16) | Settled | [[Stylix]] consumes natively; no open questions |
| Semantic tier | Open (v1 design-system work) | Names meanings, survives transposition |
| Component tier | Open (v1 design-system work) | Maps semantics to specific surfaces |
| Geometry tier | Settled, nix + CLI only | `aoide.livery.geometry` — see below |

The open schema question is scoped to the semantic and component tiers only.
The palette tier is closed.

## The geometry tier

`aoide.livery.geometry` (`modules/nucleus/options.nix`) carries the
compositor's shape values: `gapsOut`, `gapsIn`, `borderSize`, `rounding`,
`blurEnabled`, `blurSize`, `blurPasses` — every field `nullOr`, so a song
that sets none of them yields the same `hyprland.conf` as one that omits the
block entirely. The compositor facet (`modules/facets/compositor/default.nix`)
reads the tier directly and falls back field-by-field to its own opinionated
defaults (`8`/`6`/`2`/`0`/`true`/`8`/`3`) for anything unset. This tier sits
outside the engine's own schema — it is never validated by `livery lint`,
only carried through `stage/livery.json` alongside the palette/component
values for `aoide`'s own live-apply seam (below); the staged schema version
stays `"0"`, the same additive-optional posture as the base16 block.

Staging applies geometry and the window-border colours to the running
compositor directly: `aoide rice stage` builds one `hyprctl --batch`
`keyword` list, in a fixed order (gaps → border size → border colours →
rounding → blur), emitting a keyword only for a field that actually resolves
— an unset geometry field is skipped, not defaulted, so the call never fights
a host's baked config or a user's own live tweak. The call is a no-op off
Hyprland (guarded on `HYPRLAND_INSTANCE_SIGNATURE`) and never fails the
staging outcome. It never runs `hyprctl reload` — every field it touches is
live-settable via `keyword`, and a reload would re-read the baked
`hyprland.conf` from disk, discarding whatever else the compositor is
carrying live.

## Prior art — the Node engine that was folded in

The engine began as a standalone Node package wrapping
Style Dictionary. The livery merge ported
it into `crates/song/src/livery/` as native Rust and deleted the package —
no new crate, no Node toolchain. What moved, in place:

- `schema.rs` — the authoritative v0 schema validator (ported from
  `schema.js`, exact error strings preserved).
- `resolve.rs` — the flat resolver: single-level `{group.key}` alias deref
  (cycle-guarded, replacing Style Dictionary's `exportPlatform`) + the
  component `null → palette` fallback.
- `emit/{stage,hyprctl,osc,file}.rs` — four pure emitters behind one
  `Emitter` trait + registry; a new backend is one file + one registry line.
  `file` (arbitrary config-file template, `{{palette.bg}}` placeholders) is
  the generalization proof; `gtk`/`gsettings` host-mutating apply stays
  deferred to the `management` seam.

The resolver is the same fallback the nix facets apply independently for the
baked side ([[Stylix]], compositor) — identical rules, so both fan-outs agree
and staged and adopted state cannot diverge. Pure emit vs. host apply stays
split: the emitters only produce bytes; `live::apply_live` /
`shellbridge::atomic_write` are the effectful half.

## Verbs

The engine's surface is the `aoide livery` verb group (native, inside the
CLI's `Invocation`/`Outcome` shell — the standalone note CLI's
subcommands, native):

- **`aoide livery lint [<song>|<path>]`** — validate a note container
  against the authoritative v0 schema. The nix option type in
  `modules/nucleus/options.nix` is a permissive gate; *this* is the real
  validator. It enforces the closed palette tier
  (`bg/fg/accent/urgent`, unknown keys rejected) and the optional component
  tier (`bar.*` / `notif.*` / `window.*`, each field `nullOr` hex), accepting
  both bare hex strings and W3C `{ $value, $type }` note objects, and treating
  `{group.name}` alias references as valid pending resolution. `aoide rice
  lint` runs this engine natively — no binary locate, no shell-out.
- **`aoide livery resolve [<song>|<path>]`** — print the fully-resolved,
  flattened note set.
- **`aoide livery emit <target> [<song>|<path>]`** — run one of the four
  emitters (`stage` · `hyprctl` · `osc` · `file`); `--out PATH` writes
  atomically, `--template` supplies the file backend's template.

No argument defaults to the staged notes. Exit codes align with the CLI
convention: `0` ok · `2` usage · `1` error.

## The emitters

All four consume the *same* fully-resolved note set (from `resolve.rs`), so
the live targets can never disagree:

1. **`stage`** → `song/stage/livery.json` for [[Quickshell]]. With `--out PATH` it
   writes atomically (write to a temp file in the same dir, then rename over the
   target) so a Quickshell hot-reload never reads a torn file (the `CONTRACTS.md`
   §4 discipline). Component fallbacks are already applied, so the stage file
   carries concrete colours — Quickshell never reads `null`.
2. **`hyprctl`** → shell-ready `hyprctl keyword` lines for the compositor window
   border colours (`general:col.active_border` / `col.inactive_border`), colours
   formatted as `rgb(rrggbb)`.
3. **`osc`** → terminal OSC colour sequences (OSC 10/11/12 for fg/bg/cursor plus
   OSC 4 palette slots), mapping the v0 palette onto conventional ANSI slots.
4. **`file`** → a caller-supplied template rendered through
   `{{group.key}}` placeholders (e.g. `{{palette.bg}}`, `{{window.border}}`);
   an unknown placeholder is a structured error, never a panic.

## The livery schema v0

The schema (`livery/schema.rs`, `SCHEMA_VERSION = "0"`) sits inside the W3C
design-tokens container: palette is base16-closed; the component tier is the
`bar/notif/window` groups with the fallback map declared in one place
(`COMPONENT_FALLBACK`) and shared by the resolver and validator. The engine
smoke-tests all four operations (`lint`, `resolve`, `emit stage`, `emit
hyprctl`, `emit osc`) against a v0 fixture in the crate tests.

The provisional v0 stands until the design-system v1 lands: palette (`bg`,
`fg`, `accent`, `urgent`) plus component overrides (`bar.*`, `notif.*`,
`window.*`). The update playbook migrates songbook modules from v0 to v1 when
v1 supersedes. The rename to `livery` carried **no version bump** — the
shape is unchanged; the update playbook records the namespace/file rename.

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
- [[Codebase]]
