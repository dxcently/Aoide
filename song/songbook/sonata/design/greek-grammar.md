# Greek Grammar — the sonata house design language

**Song:** sonata

---

## 1. The motif vocabulary

### Greek-letter order-marks (semantic labels)

Capital Greek letters label surfaces the way the current lowercase callout ids
do — a stable per-surface **order-mark** carried at the pediment apex, WITH the
orchestration token preserved as the inscription beneath it (identity/search
survive). Assignment:

| order-mark | surface / callout token      |
| ---------- | ---------------------------- |
| `Α`        | conductor.control            |
| `Β`        | terminals.roster             |
| `Γ`        | dag.trace                    |
| `Δ`        | meters.pulse                 |
| `Θ`        | power.reserve                |
| `Λ`        | volume.level                 |
| `Ξ`        | battery.gauge                |
| `Π`        | calendar.sheet               |
| `Σ`        | nowplaying.score             |
| `Ω`        | gadgets.case (the dock body) |

Lowercase Greek (α β γ …) is available for sub-marks (e.g. a session's tier) but
is NOT required; the order-mark is the load-bearing use.

The full table lives verbatim in `GadgetFrame`'s `orderMarks` map, but only
`Ξ` (battery.gauge, still hosted in a `BarPopout`/`GadgetFrame` bay) is LIVE
on screen today — plus the `·` fallback the wallpaper picker's
`wallpaper.summon` falls to. `Α Β Γ Δ Θ Ω` never carry a live surface: the
dock temples are self-framed and inscribe their names in carved serif
(CONDUCTOR · TERMINALS · METERS · POWER) instead of a pediment order-mark.
`Λ` (volume.level) and `Π` (calendar.sheet) went dormant too, once those
popouts moved to bare stele hosting (`StelePopout`/`SteleLayerPopout` — no
`GadgetFrame`, so no pediment left to carry a mark); `Σ` (nowplaying.score)
has no live popout wiring at all right now. A mark stays live only for as
long as its surface is actually hosted inside a `GadgetFrame` bay.

### The musical motifs coexist (unchanged)

The music glyphs are **inscriptions carved into the stone**, not replaced:

- the **state tier** `♪ 𝄐 𝄽 𝄂 ·` (theme.rs contract, §2) reads like a letter set
  into a frieze — it sits inside the body where it already sits.
- the **clef** `𝄞` (bar power key), the **rests** `𝄽 𝄾 𝄿 𝅀 𝅁 𝅂 𝆑`, the **note
  heads** `♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯`, and the network marks `𝆹𝅥𝅮 𝆺𝅥𝅯` all stay verbatim. (The old
  dock's `𝄂𝄚𝅦𝄚` seam run retired with the colonnade — no surface draws it today.)
- each temple carries its OWN clef as its crown: `𝄞` treble (Conductor),
  `𝄢` bass (Terminals), `𝄡` alto (Meters) — and Power crowns with the Greek
  koppa `ϟ`, a letter that reads as Zeus's lightning, standing in where no clef
  fits.
- the crown convention outlives the (dormant, per above) dock temples: two of
  the actually-live per-song flavor widgets crown themselves the same way.
  `widgets/notifications.qml` crowns with `❧` (a rotated floral
  notice-ornament, U+2767) — "the same way Power's `ϟ` does when no clef fits
  — a herald has no voice range to notate," in the widget's own words.
  `widgets/calendar.qml` crowns with `𝄴` beside its carved "KALENDAE" label.
  Both render on screen today; none of the dock clefs above currently do.
- where a meander frieze and a musical seam would both appear, the musical seam
  wins (it carries meaning); the meander yields.

---

## 2. The glyph grammar — state tier is a HARD contract

Identical to Pantheon §2, restated so this grammar stands alone. The state
vocabulary is lifted VERBATIM from `pkgs/aoide/crates/conductor/src/theme.rs`
(`state_glyph`/`classify`) and kept in lockstep with the Rust — the grammar
**reuses** it, never replaces it:

| glyph | state                                       |
| ----- | ------------------------------------------- |
| `♪`   | working (running / active / tool)           |
| `𝄐`   | awaiting (await / block / notification)     |
| `𝄁`   | stopped (the turn just ended, < 1h ago)     |
| `𝄽`   | idle (cold: stopped > 1h, or fresh/resumed) |
| `𝄂`   | done (the session itself ended)             |
| `·`   | unknown                                     |

Used by `ConductorGadget` and `TerminalsGadget` (both switch on the daemon's
canonical `working|awaiting|stopped|idle|done` strings — no regex derivation).
`stopped` is a genuinely separate fifth state, not a `done` alias — the bridge
splits "the turn ended" (stopped, decaying to idle after an hour of no activity)
from "the session ended" (done). The colours partnering the glyph are a fixed
shared spread: working → gold (`paletteAccent`), awaiting → terracotta
(`paletteUrgent`), stopped → aegean (`holoBlue`), idle → murex (`violet`), done
→ verdigris (`wireCyan`), unknown → dim ink — identical in every temple, so a
"working" note reads the same in every house. The emphasized/traced row
overrides its glyph to laurel (§3's restraint rule). No Greek motif touches
the glyphs themselves.

---

## 3. The role palette in the Greek key

Which Greek hue plays which `notes` role (full derivation in
`design/intent.md`):

| role (notes)    | slot   | Greek hue         | where it works                                                                                                                                                                                  |
| --------------- | ------ | ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paletteBg`     | base00 | marble ground     | every glass body                                                                                                                                                                                |
| `paletteFg`     | base05 | plum-charcoal ink | all text, column rails at rest                                                                                                                                                                  |
| `paletteAccent` | base0A | Attic gold        | active states, toggles, active workspace fill, running marks                                                                                                                                    |
| `paletteUrgent` | base08 | terracotta        | urgent / blocked                                                                                                                                                                                |
| `paletteHot`    | base0B | laurel leaf-green | the ONE blaze (traced element)                                                                                                                                                                  |
| `wireCyan`      | base0C | bronze-verdigris  | columns, meanders, structural rules                                                                                                                                                             |
| `holoBlue`      | base0D | aegean deep-blue  | preview / information — the workspace preview ring, links, cool syntax ("the sea between the columns"); NOT chrome. Also the `stopped` state colour (§2) — a second job, not a chrome exception |
| `violet`        | base0E | Tyrian/murex      | DAG project steles                                                                                                                                                                              |
| `glitchPink`    | base08 | terracotta        | urgent pulse (== urgent here)                                                                                                                                                                   |

**Restraint (the core rule):** laurel leaf-green `hot` is the single traced
colour; everything else rests dim. The multicolour field is ROLES, not
decoration.

**The structural cap, still enforced, no longer written down here.** A ≤0.5-
alpha ceiling on `wireCyan`'s STRUCTURAL use (never the hue itself — a hue
can still appear at full strength as, say, a stele's signature colour) was
once an explicit invariant in this file's own §5, before the 2026-08-14
retcon; `design/intent.md`'s iteration log is the only place that still
records it existed. It is still live in the actual widget code: every
current structural `wireCyan` read sits at exactly 0.35 —
`widgets/powermenu.qml`'s "ΕΞΟΔΟΣ" title rule and its own header's rationale
for the LOGOUT stele's signature hue ("the ≤0.5-alpha cap binds the
STRUCTURAL role, not the hue... stays ≤0.35"), and `widgets/launcher.qml`'s
Grimoire page's inner illuminated border. Both widgets restate the ceiling in
their own header comments independently of each other and of this file.
Restated here since this file no longer does — see `design/widget-structure.md`
§8 for the full per-role breakdown against live widget code.

---

## 4. The herald tiers — notification card reading order

The notifications card (the Tuscan fifth order, `widgets/notifications.qml`)
renders three differentiated tiers, top to bottom, so program and message
never run together:

1. **Program title** — the box-drawing top frame `┌─┤ <parent> ├───┐`,
   resolved smartly: desktop-entry id (stripped of `.desktop`) first, then
   appName, then "notice". Terminal-forwarded OSC-9 notifications (kimi via
   kitty) carry the forwarder as appName — the real program is not in the
   payload.
2. **Notification title** — bold serif, full ink. A "Title: message" summary
   with an empty body (the OSC-9 mash shape) splits generically at the first
   ": " (multi-word head only, 6–59 chars) into title + context.
3. **Context** — smaller, dimmer (0.85), indented behind a 1px signature
   hairline flush with the title's left edge. The cut rule between the
   program frame and the title is the entablature/shaft boundary of the
   stele; the Tuscan frieze above the frame stays the only ornament.

Rhythm: frame → cut → title → context gaps hold one tempo; the `[ notify ]`
tag is ghosted (0.55) so the HERALD wordmark owns the entablature band.
Card width 360 (stack and card together). The ledger's right-hand ink is a
live arrival clock (HH:MM, refreshed while the card lives); the urgency word
appears only in the bottom frame label — nothing reads twice.
