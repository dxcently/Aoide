# Greek Grammar — sonata's drawing vocabulary

**Song:** sonata

The concrete kit an author picks from: faces, glyphs, type tiers, the opacity
ladder, rule idioms, state treatments, and which `notes.*` role plays which
part. Everything here is in live use in `widgets/*.qml` or the shared facet
chrome — nothing is aspirational.

Companions: `making-a-widget.md` (how these pieces assemble into something
that belongs), `widget-structure.md` (mechanism + hard contracts),
`hazards.md` (what has failed live).

---

## 1. The three faces

Declared identically at the top of `notifications.qml`, `powermenu.qml`,
`launcher.qml`, `calendar.qml` and `AudioColonnade.qml`:

```qml
readonly property string faceSerif: "Noto Serif"                // carved marble
readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
readonly property string faceMusic: "Noto Music"                // notation
```

| face | carries |
|---|---|
| `faceSerif` | carved names, titles, body text, day numerals, Greek whispers (italic) |
| `faceMono` | box-drawing frames, `[ tag ]` labels, tallies, figures, hints, folios |
| `faceMusic` | notation glyphs — clefs, rests, note heads, barlines, `♪` marks |

`bar.qml` is the exception: every cell on the strip declares
`font.family: "monospace"` (the generic fontconfig alias), which is where all
three of this codebase's recorded glyph failures happened — see `hazards.md`
§1. A new widget should declare one of the three faces above by name.

The division is semantic, not aesthetic: **serif = carved into the surface,
mono = machine reading out a value, music = notation.** Mixing them inside one
line is normal and intended — `notifications.qml`'s ledger puts a kaomoji
(default face) on the left and a mono clock on the right.

---

## 2. Glyph vocabulary in live use

Only glyphs already on screen somewhere are listed. Anything not on this list
needs a live check before you trust it (`hazards.md` §1).

**State tier — a HARD contract, see §3.** `♪ 𝄐 𝄁 𝄽 𝄂 ·`

**Notation**

| set | glyphs | where |
|---|---|---|
| clefs | `𝄞` treble, `𝄢` bass, `𝄡` alto, `𝄴` | bar head, launcher emanation and `AoidePanel` (`𝄞`); the summoned dock's temples crown themselves — Conductor `𝄞`, Terminals `𝄢`, Meters `𝄡`, Power `ϟ`; calendar `𝄴` |
| rests (deepening) | `𝄽 𝄾 𝄿 𝅀 𝅁 𝅂` plus `𝆑` full, `𝄮` charging | `bar.qml`'s `battIcon()` |
| note heads (solid) | `♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯` plus `𝅘𝅥𝅱` magic, `𝄋` scratch/segno | workspaces, `volIcon()`, rice-mode draft |
| half note | `𝅗𝅥` | volume at 0 |
| barlines / marks | `𝄂` fine, `𝄇` da capo, `𝄌` coda, `𝄐` fermata, `𝄑` fermata below | powermenu endings, closing frames, tray toggle |
| network | `𝆹𝅥𝅮` wifi, `𝆺𝅥𝅯` ethernet | bar right stave |
| stand-ins where no clef fits | `ϟ` koppa (Power), `❧` rotated fleuron (Herald), `♫` (Audio) | crowns |

**Box drawing.** `┌ ┐ └ ┘ ─ ┤ ├` for TUI frames; `═ ║ ▔ ▁` for `GadgetFrame`'s
entablature and stylobate; `▓ ░` for ASCII gauges (`battBar()`,
`LiveryState.ctxBar()`).

**Frieze strings** (powermenu, one per stele, `faceMono` 12px):
`◆───◆───◆` · `⌐¬ ⌐¬ ⌐¬ ⌐¬` · `╌╌ ╌╌ ╌╌ ╌╌` · `▔▁▔▁▔▁▔` · `┏┛┗┓┏┛┗┓` ·
`══════════`

**Greek.** Capital order-marks `Α Β Γ Δ Θ Λ Ξ Π Σ Ω Η` (§4); lowercase
`αβγδεζηθικλμνξοπρστυφχψω` as launcher chapter tabs (bijective base-24, so it
never runs out); weekday initials `Κ Δ Τ Τ Π Π Σ`; running Greek words as
whispers and empty-state lines (`ἡμερολόγιον`, `τί ζητεῖς;`, `οὐδὲν
τοιοῦτον ὄνομα`, `κλείς`, `τέλος`).

**Marks and ornaments.** `·` neutral/unknown and the resting margin dot; `◆`
corner rosettes; `✦ ⟡ ❈ ✧ ✶` launcher emanations; `‹ › « »` paging guillemets
(rotated 90° in `calendar.qml` — see the note below); `⌃ ⌄` mono direction
wedges; `←  →  ⏎  ⌘` key hints; `✎` the sessions cell.

**Colour glyphs, sanctioned once.** `U+1F311`–`U+1F318` moon phases in
`calendar.qml` render as real colour emoji — the one family wearing fixed
colour outside `notes.*`. `🗒` is the launcher's clipboard bookmark tab.

*Rotation note:* a rotated `Text`'s layout box does NOT rotate with it, so
`calendar.qml` rides every rotated guillemet inside a fixed-size wrapper
`Item`. Rotating an existing glyph beat introducing new code points — the
vertical CJK guillemets `︿ ﹀ ︽ ︾` arrive full-width and sans-weight through
a Noto Sans CJK fallback, and `˄ ˅` are modifier letters, too small to hit.
**Prefer rotating a proven glyph over adopting an unproven one.**

---

## 3. The state-glyph contract (hard)

Lifted VERBATIM from `pkgs/aoide/crates/conductor/src/theme.rs`
(`state_glyph`/`classify`) and kept in lockstep with the Rust. The grammar
reuses it; it never redefines it.

| glyph | state |
|---|---|
| `♪` | working (running / active / tool) |
| `𝄐` | awaiting (await / block / notification) |
| `𝄁` | stopped (the turn just ended, < 1h ago) |
| `𝄽` | idle (cold: stopped > 1h, or fresh/resumed) |
| `𝄂` | done (the session itself ended) |
| `·` | unknown |

`stopped` is a genuinely separate fifth state, not a `done` alias — the bridge
splits "the turn ended" (decaying to idle after an hour) from "the session
ended". Consumers switch on the daemon's canonical
`working|awaiting|stopped|idle|done` strings; no regex derivation.

The partnering colours are fixed and shared, so a "working" note reads the
same in every surface: working → gold (`paletteAccent`), awaiting → terracotta
(`paletteUrgent`), stopped → aegean (`holoBlue`), idle → murex (`violet`),
done → verdigris (`wireCyan`), unknown → dim ink. An emphasized/traced row
overrides its glyph to laurel (§5's restraint rule).

**A glyph may serve a second register on a different surface**, as long as the
two can't collide in one place: `bar.qml` uses `𝄽` for both mute and
network-down on its strip, and reads `𝄐`/`𝄑` as the tray hold (a fermata holds
a note; the popout's token is `tray.held`). The file says so in its own
comment. Two readings of one glyph two cells apart is what the rice-mode cell
explicitly avoided.

---

## 4. Greek-letter order-marks

A stable per-surface capital-Greek mark carried at a `GadgetFrame` pediment's
apex, with the orchestration token preserved as the inscription beneath it, so
identity and search survive.

| order-mark | surface / callout token |
|---|---|
| `Α` | conductor.control |
| `Β` | terminals.roster |
| `Γ` | dag.trace |
| `Δ` | meters.pulse |
| `Θ` | power.reserve |
| `Η` | audio.control |
| `Λ` | volume.level |
| `Ξ` | battery.gauge |
| `Π` | calendar.sheet |
| `Σ` | nowplaying.score |
| `Ω` | gadgets.case (the dock body) |

The table lives verbatim in `GadgetFrame.qml`'s `orderMarks` map. **A mark is
live only while its surface is actually hosted inside a `GadgetFrame` bay.**
Today only `Ξ` (battery.gauge, `bar.qml`'s one remaining `BarPopout`) is on
screen, plus the `·` fallback for unmapped titles. The dock temples are
self-framed and inscribe carved serif names instead; `Λ` and `Π` went dormant
when those popouts moved to bare stele hosting; `Σ` has no live wiring.

**The `<noun>.<qualifier>` token is the general popout-naming convention**, not
something scoped to the eleven marked surfaces — `bar.qml`'s tray toggle names
its popout's token `tray.held` in its own comment, a token with no assigned
mark (and, as of 2026-08-15, no popout body: that popout was torn out for a
from-scratch pass and only the toggle remains). The Greek letters are a
labeling layer over an already-general callout-id scheme.

Lowercase Greek is available for sub-marks but carries nothing load-bearing
today except the launcher's chapter tabs.

---

## 5. The role palette

Full derivation of the hues in `intent.md`; this is which role plays what.

| role (`notes.*`) | slot | Greek hue | its job |
|---|---|---|---|
| `paletteBg` | base00 | marble ground | every body fill; also the INK for text over the dark scrim |
| `paletteFg` | base05 | plum-charcoal ink | all text, all borders, all structural rules |
| `paletteAccent` | base0A | Attic gold | open / active / live: active workspace, open toggles, tallies, closing `𝄂`, keylines |
| `paletteUrgent` | base08 | terracotta | urgent / blocked / muted |
| `paletteHot` | base0B | laurel leaf-green | THE ONE BLAZE — the selected/traced element, nothing else |
| `wireCyan` | base0C | bronze-verdigris | structural chrome: column rails, title rules, inner illuminated borders |
| `holoBlue` | base0D | aegean deep-blue | preview / information — hover-preview ring, mic cell, lunar layer, generic-name gloss. NOT chrome. Also the `stopped` state colour (§3) |
| `violet` | base0E | Tyrian / murex | DAG project steles; powermenu SUSPEND signature |
| `glitchPink` | base08 | terracotta | urgent pulse (identical hue to `paletteUrgent` in this key) |
| `base09` | base09 | clay / amber | calendar's rubric, AudioColonnade's signature |
| `base0F` | base0F | rust | notifications' signature |
| `noteColor(id)` | base08–base0F | the 8-hue accent spread | one distinct hue per small integer id (bar workspaces) |

Alongside these, `LiveryState` exposes the component tiers
`notifBg/notifFg/notifUrgent`, `barBg/barFg/barAccent` and
`windowBorder/windowBorderInactive`. In sonata they are all-null in the song,
so they resolve to the palette values; `notifications.qml` reads the `notif*`
names anyway, which is what lets a future song re-skin the card without
touching the palette.

Three names resolve to the same terracotta here (`paletteUrgent`,
`glitchPink`, `notifUrgent`). Pick by intent, not by value — the name is what
survives a re-key.

**Restraint (the core rule).** Laurel is the single traced colour; everything
else rests dim. The multicolour field is ROLES, not decoration. Census: see
`making-a-widget.md` §2 — laurel appears exactly twice per widget file, and
both reads always serve one state.

**The structural cap (invariant, live).** `wireCyan` in a STRUCTURAL role
renders at ≤ 0.5 alpha at all times, so the verdigris can never out-read the
laurel blaze on warm marble. The cap binds the ROLE, not the hue — a hue may
appear at full strength as, say, one stele's signature colour. Every live
structural read sits at exactly `0.35`: `powermenu.qml`'s `ΕΞΟΔΟΣ` title rule,
`launcher.qml`'s inner illuminated page border; `GadgetFrame.qml` holds all of
its chrome at `0.5 * chromeOpacity`.

**Gold is a fill/line colour, never running body text** — 3.54:1 against the
marble ground clears the 3:1 bar for fills and lines but not the 4.5:1 AA bar
for body copy (`intent.md`, Contrast). Same standing rule as the verdigris cap.

---

## 6. Type tiers actually in use

| tier | size | face / weight | used for |
|---|---|---|---|
| crown | 20–24 | music or serif, per-widget | the clef-position glyph |
| carved name | 13–15 | serif DemiBold, `letterSpacing: 3–4` | HERALD, AUDIO, KALENDAE, LOCK, GRIMOIRE |
| display | 27 | serif DemiBold, `letterSpacing: 12` | `ΕΞΟΔΟΣ` — the one full-screen title |
| title / body | 13–14 | serif bold (title) or regular; mono on the bar | notification title (serif bold 14), launcher entry name (serif 14), bar strip cells (mono 13–14) |
| chapter head | 13 | serif, `letterSpacing: 3` | launcher running headers |
| secondary | 11–12 | serif or mono | context text (12 @0.85), day numerals (12), friezes (12) |
| frame / figure | 10–11 | mono | box-drawing frames, tallies, hints, folios (10), `[ tag ]` (10) |
| whisper | 10–11 | serif ITALIC | Greek answer-lines (`ἡμερολόγιον` 10, powermenu whispers 11) |
| micro | 7–9 | mono (serif for the phase glyphs) | ISO week rails (8 compact / 7 mini), memento (8), mini-grid numerals (8), phase glyphs (9 / 7) |

Rules that hold across all of them:

- Letter-spacing belongs to carved names only. Nothing else spaces.
- Italic marks the Greek answer-line, and nothing else.
- Bold is a STATE, not a style: `font.bold: parent.isToday`,
  `font.weight: row.isSel ? Font.Bold : Font.Medium`. (And it is a rendering
  hazard on rare glyphs — `hazards.md` §1.)
- Micro tiers (7–9px) are legal only for informational layers. Nothing you
  came to read lives below 10px.

---

## 7. The opacity ladder

Steps in live use, and what sits on each. Author from this list rather than
inventing intermediate values.

| alpha | what |
|---|---|
| 1.0 | the live/selected element; current-month numerals; the value you came for |
| 0.95 | frame corners and top-frame text in the signature hue; board inscriptions |
| 0.88–0.9 | resting glyph in a signature hue; kaomoji; Greek whisper; `[ tag ]`; unselected row text (0.88) |
| 0.8 | frame connector rules; frieze strings; bottom-frame label ink; gold keylines |
| 0.7 | resting inset keyline; unhovered nav arrows; scrim-borne hint text |
| 0.55 | the cut rule; closing connector; `[ notify ]` tag; the resting rice-mode cell |
| 0.5 | key hints; memento; sparse-state filler; `GadgetFrame`'s entire chrome ceiling |
| 0.45 | context hairline; folio; chapter head rule |
| 0.3–0.35 | ledger rule (0.3); margin dot (0.3); structural verdigris cap (0.35); TUI corner rules (0.35); spill-month day numerals (0.3) |
| 0.22 | the cast shadow — one value, every widget |
| 0.16–0.18 | action-button fills; hover pill under a bar cell |
| 0.13 | blank manuscript ruling |

Two reads of the same thing at different tiers is how the house shows
relationship — e.g. `notifications.qml`'s frieze is the same 1px rule twice,
at 0.7 and 0.35, 4px apart.

---

## 8. Rules, dividers and frames

The house draws separators five ways; a sixth is a new idiom and needs a
reason:

1. **The 1px full-width rule** — `Rectangle { height: 1; color: withA(sig,
   0.3) }` as the ledger's top edge; `withA(clay, 0.55)` as the calendar's
   closing rule. This is the default divider.
2. **The broken rule** — a label sits in the gap and rules run to it from
   both sides (`GadgetFrame`'s architrave, `launcher.qml`'s chapter head with
   its 52×11 Greek-key `Canvas` knot, powermenu's `┌ ┤ term ├ ┐`).
3. **The box-drawing frame line** — corner glyphs anchored left/right, the
   label as `"┌─┤ " + name + " ├"`, and a `Rectangle` bridging
   `anchors.left: tfL.right` to `anchors.right: tfR.left`. The top frame's
   label names the source; the bottom frame closes on a `𝄂` — gold and set
   apart at the right on a popout stele (`notifications.qml`,
   `AudioColonnade`), dim ink inside the centred `┤ 𝄂 ├` of a powermenu bay.
4. **The vertical hairline** — 1px, anchored to a text block's top and bottom,
   as an indent guide (`notifications.qml`'s context tier at `sig @0.45`).
5. **The double rule** — two 1px lines 4px apart at stepped alpha (the Tuscan
   frieze); or, on the bar, the 1.5px + 3px final barline pair.

An ornament course (frieze) may be a `Canvas` instead: bead-and-reel
(AudioColonnade), Vitruvian wave-scroll (calendar), Greek-key knot (launcher).
Keep it to `lineWidth: 1–1.3` in the signature hue, 10px tall.

---

## 9. State treatments

| state | how it reads |
|---|---|
| rest | full identity, ink at 0.88–1.0, resting hue at 0.7–0.9 |
| hover (cell) | opacity to 1.0 + `font.underline` (bar), or a soft accent pill at 0.18 behind the glyph |
| hover (stele) | `scale: 1.045` OutBack 170ms + 7px lift + keyline to laurel 2px + a `♪` tick fading in |
| selected | laurel: rule 1px→2px, margin `·`→`♪`, weight Medium→Bold, text 0.88→1.0 |
| active / open | `paletteAccent` on the glyph or fill; the toggle's own glyph may change (`𝄐`→`𝄑`) |
| preview / informational | `holoBlue`, and a DIFFERENT KIND of mark from the active one — a hollow ring vs a solid pill, so both can show at once |
| urgent / blocked | `paletteUrgent`/`glitchPink` + a 600ms breathing pulse between 0.35 and 1.0 |
| critical (card) | signature hue swapped to terracotta THROUGHOUT + a 700ms breathe between 0.8 and 1.0 |
| unavailable | ink at 0.22–0.4, and the value replaced by `—` |
| muted / off | a rest glyph `𝄽` in place of the value; or, where there is room, a silhouette change (AudioColonnade's broken column) |

Two rules govern all of them:

- **A state must survive colour removal.** Active vs preview differ in mark
  SHAPE (solid pill vs hollow ring); muted differs in GLYPH or silhouette;
  selected differs in WEIGHT and rule thickness.
- **Nothing jumps.** Cross-fade `opacity` instead of toggling `visible` on
  anything inside a centred row; ride transforms rather than anchors when an
  element must move.

---

## 10. Two recurring texture idioms

**The state-keyed kaomoji.** Every widget carries at least one, always tied to
a live read, never free decoration: `kaomojiFor()` keyed on urgency
(notifications), on browse direction (calendar), on mute/level (AudioColonnade);
`kaomojiSet` re-rolled on every active-window change (bar); `scatterFaces`
re-scattered on every query change (launcher). Stay inside the kana and
punctuation vocabulary `MoodFaces.qml` already proves safe on this system
(`ω ｏ ノ ｀ ´ ˘ ･` and friends) — an earlier pick used Thai and Hangul and
rendered as tofu-adjacent junk live.

**The bilingual inscription.** A carved Latin/English name answered beneath by
a small italic Greek line — `AoidePanel`'s `AOIDE ⁄ ᾠδή` idiom. In use:
`KALENDAE` / `ἡμερολόγιον`; `ΕΞΟΔΟΣ` / `the closing song — πῶς τελευτᾷ ἡ
ᾠδή;`; `MOST SUMMONED` / `ἕξις · σελίς I`; `GRIMOIRE ·` / `βίβλος` split
across the two cover boards. Three of five widgets use it.
`notifications.qml` deliberately does not — its austerity is the point. It is
an available idiom, not a requirement.

---

## 11. Reading order

Every surface with more than one text tier orders them explicitly, and
**nothing reads twice**. The worked case is the notification card
(`making-a-widget.md` §7), whose three tiers are:

1. **Program title** — the box-drawing top frame `┌─┤ <parent> ├───┐`,
   resolved desktop-entry id → appName → `"notice"`.
2. **Notification title** — bold serif, full ink. A `"Title: message"` summary
   with an empty body splits generically at the first `": "` (multi-word head
   only, 6–59 chars) into title + context.
3. **Context** — smaller, dimmer (0.85), indented behind a 1px signature
   hairline flush with the title's left edge.

The generalized rule: **frame → cut → title → context**, one tempo between the
gaps, one ornament (the frieze) above the frame, and any value shown in the
ledger removed from the frame label. The card's `[ notify ]` tag is ghosted to
0.55 so the `HERALD` wordmark owns the entablature band; the ledger's
right-hand ink is a live arrival clock, and the urgency word appears only in
the bottom frame — because it would otherwise read twice.
