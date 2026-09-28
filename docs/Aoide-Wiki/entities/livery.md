---
type: entity
created: 2026-07-26
updated: 2026-08-25
tags: [aoide, livery, theming, base16]
---

# livery — the design tokens, and the engine that stamps them

`livery` is Aoide's **design-token layer**: one name for both the values and
the act of stamping them. The same word names the native engine that
validates, resolves, and emits the tokens. A song authors `aoide.livery.*`;
the paint lanes read `aoide.livery` and nothing else; the runtime seam is
`stage/livery.json`. Naming rationale lives in [[Lexicon]].

The livery merge (LIVERY-MERGE.md) ported the standalone Node engine into
native Rust inside `crates/song/src/livery/`, taking the name **livery**
across the option namespace, songbook data files, and the live stage
contract in one pass (the previous option namespace kept evaluating via a
`mkRenamedOptionModule` alias until Phase 4 closed the transition window).

## The seam between score and performance

livery is where the frozen nix layer and the live desktop meet: values
frozen into the crystal, sounded at runtime. The container stays W3C
design-tokens format; livery is Aoide's name for what fills it.

Every paint lane consumes livery and nothing else. No module reads another
module. The coupling discipline is contractual, not polite.

## Two fan-outs, one source

```
livery (single source)
    ├── stage/livery.json  →  Quickshell + hyprctl + kitty socket  (rehearsal / live)
    └── rice.nix → Stylix   →  every nix-manageable app             (recording / adopted)
```

[[Stylix]] is the baked fan-out. `rice.nix` feeds one base16 scheme into
Stylix (plus fonts, cursor, wallpaper) and Stylix themes every nix-manageable
target — GTK/Qt, terminal, editors, browser, boot. The engine keeps only the
live side.

Because both fan-outs derive from the same livery values, staged state and
adopted state cannot diverge. This is the "zero drift" guarantee. Beyond
`rice stage`/`cover set` writing `stage/livery.json` live (while `rice mode
staging` allows it — see [[Self-Ricing#Staging vs Declarative Mode]]), the lyra
lane's `home.activation.aoideSeedStage` reasserts it from the active song's
committed `song/songbook/<song>/livery.json` on every activation (see
[[Codebase#Runtime contracts (socket + stage files)]]), through
`lib/livery.nix`'s `stagePatch` — the same `aoide.livery.override` venue
recolour the Stylix/compositor fan-outs apply through `resolve` — so a
freshly booted host carries a correctly recoloured stage twin even before
`rice stage` ever runs. The same run also publishes the DECLARED twin,
`song/declared/livery.json` (CONTRACTS.md §4) — those same bytes under their
own name, its `"song"` field naming the song the venue declared. That field
is the whole scope: the runtime writers (`rice stage`, `rice mode
stage`/`declarative`, `reload`'s staging arm) derive the DECLARED song's
notes from the twin, so re-staging it reproduces the venue recolour, while a
song the twin does not name still re-derives from that song's own committed
notes.
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
| Font tier | Settled, nix only | `aoide.livery.fonts` — see below |

The open schema question is scoped to the semantic and component tiers only.
The palette tier is closed.

## The geometry tier

`aoide.livery.geometry` (`modules/nucleus/options.nix`) carries the
compositor's shape values: `gapsOut`, `gapsIn`, `borderSize`, `rounding`,
`blurEnabled`, `blurSize`, `blurPasses`, plus `terminalOpacity` — the one field
whose fallback owner is the kitty dendrite rather than the compositor. Every
field is `nullOr`, so a song that sets none of them yields the same
`hyprland.conf` as one that omits the block entirely. The compositor lane
(`modules/dendrites/compositor/hyprland/default.nix`)
reads the tier directly and falls back field-by-field to its own opinionated
defaults (`8`/`6`/`2`/`0`/`true`/`8`/`3`) for anything unset. The tier is
carried through `stage/livery.json` alongside the palette/component values for
`aoide`'s own live-apply seam (below); the staged schema version stays `"0"`,
the same additive-optional posture as the base16 block. `livery lint` checks
exactly one field of it — `terminalOpacity`, which must be a plain number in
[0, 1] or `null`, since it is written into a kitty config line; the rest of the
tier is outside the engine's schema and reaches Hyprland through
`live::geometry_keywords`, which types each field itself.

Staging applies geometry and the window-border colours to the running
compositor directly: `lyra rice stage` builds one `hyprctl --batch`
`keyword` list, in a fixed order (gaps → border size → border colours →
rounding → blur, then a second batch for the hyprglass pair), emitting a
keyword only for a field that actually resolves — an unset geometry field
sends no keyword, so the call never fights a host's baked config or a user's
own live tweak. Two fields are the exception, both in §1: the hyprglass pair
is always sent (a song with no `blurEnabled` opinion restores the baked
default rather than leaving the previous song's glass in place), and
`terminalOpacity`, not a Hyprland keyword at all, rides
`song/stage/terminal-colors.conf` — the song's value, or the kitty dendrite's
baked `0.86` when it has none (§4). The call is a no-op off
Hyprland (guarded on `HYPRLAND_INSTANCE_SIGNATURE`) and never fails the
staging outcome. It never runs `hyprctl reload` — every field it touches is
live-settable via `keyword`, and a reload would re-read the baked
`hyprland.conf` from disk, discarding whatever else the compositor is
carrying live.

## The font tier

`aoide.livery.fonts` (`modules/nucleus/options.nix`) carries the face a song
wears, keyed by [[Stylix]] font role: `fonts.monospace = { package, name } |
null` — the terminal, where agents live, and the one role a song uses today.
The role's shape is Stylix's own, so the lane passes the value through
untranslated; `package` is a literal nix value, the `wallpaper` note's
precedent. The remaining roles (`sansSerif`, `serif`, `emoji`, `sizes`) stay
the lane's, and `null` means "the lane's own face" — Linux Libertine Mono O —
so a song that sets no face bakes the theme it baked before.

This tier is BAKED ONLY, the cover note's posture rather than geometry's: it
has no `stage/livery.json` twin and no engine schema entry, because nothing at
runtime reads a face and a terminal cannot be re-faced mid-session — and no
`rice lint` rule makes that so. The livery schema closes `palette`, `base16`,
each component group and each widget record, but it never walks a document's
top-level keys, so a stray `fonts` key in a song's `livery.json` passes lint
and rides the activation seed into the stage file unread; the tier lives in
`rice.nix` alone by convention. A face change lands on the user-gated rebuild,
and staging another song does not re-face the terminal. A live face would be a
new emitter (OSC 50 against the terminal), not a change to this tier.

`cadenza` sets it to a CRT console face — its terminal is the key's own
voice, not the desktop's default serif. Widget faces are the song's own
business (`song/songbook/<song>/widgets/Kit.js`) and do not move with this
tier.

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
- `emit/{stage,hyprctl,osc,file,kitty}.rs` — five pure emitters behind one
  `Emitter` trait + registry; a new backend is one file + one registry line.
  `file` (arbitrary config-file template, `{{palette.bg}}` placeholders) is
  the generalization proof; `gtk`/`gsettings` host-mutating apply stays
  deferred to the `management` seam.

The resolver is the same fallback the nix paint lanes apply independently for the
baked side ([[Stylix]], compositor) — identical rules, so both fan-outs agree
and staged and adopted state cannot diverge. Pure emit vs. host apply stays
split: the emitters only produce bytes; `live::apply_live` /
`shellbridge::atomic_write` are the effectful half.

## Commands

The engine's surface is the `lyra livery` command group, native inside the
CLI's `Invocation`/`Outcome` shell:

- **`lyra livery lint [<song>|<path>]`** — validate a livery file
  against the authoritative v0 schema. The nix option type in
  `modules/nucleus/options.nix` is a permissive gate; *this* is the real
  validator. It enforces the closed palette tier
  (`bg/fg/accent/urgent`, unknown keys rejected) and the optional component
  tier (`bar.*` / `notif.*` / `window.*`, each field `nullOr` hex), accepting
  both bare hex strings and W3C `{ $value, $type }` token objects, and treating
  `{group.name}` alias references as valid pending resolution. `lyra rice
  lint` runs this engine natively — no binary locate, no shell-out.
- **`lyra livery resolve [<song>|<path>]`** — print the fully-resolved,
  flattened livery set.
- **`lyra livery emit <target> [<song>|<path>]`** — run one of the five
  emitters (`stage` · `hyprctl` · `osc` · `file` · `kitty`); `--out PATH`
  writes atomically, `--template` supplies the file backend's template.

No argument defaults to the staged livery. Exit codes align with the CLI
convention: `0` ok · `2` usage · `1` error. [[Rice-and-Livery]] carries each
command's full I/O — reads, writes, output shape. The three commands ship inside
[[lyra]], the paint binary that hosts every command this page's engine
backs.

## The emitters

All five consume the *same* fully-resolved livery set (from `resolve.rs`), so
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
5. **`kitty`** → `song/stage/terminal-colors.conf`, kitty's own colour syntax:
   the tinted-kitty base16 template Stylix bakes, key for key. A note with no
   base16 tier gets the scheme the stylix lane synthesises from its palette,
   so every slot is written either way (CONTRACTS.md §4).

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

The [[Quickshell]] lane declares the surfaces it owns; [[Stylix]] disables
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
