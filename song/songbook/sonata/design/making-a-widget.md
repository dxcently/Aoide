# Making a sonata widget — what makes one BELONG

**Song:** sonata
**Read before:** writing any new `widgets/*.qml`, or redesigning an existing one.

> **2026-08-16:** `widgets/notifications.qml` (the per-popup card cited
> throughout as the stele exemplar) is retired — dunst owns notification
> delivery now and the herald lives on as `widgets/herald-center.qml`, the
> dock's notification center, which carries the SAME grammar (Tuscan stele, ❧
> crown, rust signature). Read `herald-center.qml` wherever this file says
> `notifications.qml`; the grammar analysis below stands unchanged.

The other three files in this folder tell you what is *available*
(`greek-grammar.md`), how the machinery *works* (`widget-structure.md`), and
what has *bitten us live* (`hazards.md`). This file is the one that answers the
question those three kept failing to answer: **why does a widget that follows
all the rules still look like it came from a different desktop?**

Everything below is read off the widgets that are on screen today. Every number
is a real number from a real file; every quote is a real line.

---

## 1. The finding: belonging is a BUILD ORDER, not a metaphor

Three widgets in this house were written weeks apart, by different passes, on
three unrelated metaphors — a Tuscan temple stele
(`widgets/notifications.qml`), a two-column temple porch
(`modules/facets/quickshell/qml/AudioColonnade.qml`), and a papyrus scroll
(`widgets/calendar.qml`). Nothing in their concepts overlaps.

Their part lists are the same list, in the same order, at the same sizes:

| # | Part | notifications.qml | AudioColonnade.qml | calendar.qml |
|---|---|---|---|---|
| 1 | cast shadow, offset down-right | `+4/+5`, `paletteFg` @ 0.22 | `+4/+5`, `ink` @ 0.22 | 4px/5px strips, `ink` @ 0.22 |
| 2 | body: flat fill, hard corners | `notifBg`, `radius: 0` | `paletteBg`, `radius: 0` | `paletteBg`, `radius: 0` |
| 3 | 2px ink border | `paletteFg` | `ink` | `ink` |
| 4 | 1px inset keyline in the SIGNATURE hue | `margins: 4` | `margins: 4` | `margins: 4`, clay @ 0.8 |
| 5 | content `Column` | `margins: 10`, `spacing: 4` | `margins: 11`, `spacing: 4` | `margins: 12`, `spacing: 4` |
| 6 | entablature row (crown / carved name / bracket tag) | h 24 | h 24 | h 26 |
| 7 | an ornament course (the frieze) | h 5, plain double rule | h 10, bead-and-reel `Canvas` | h 10, wave-scroll `Canvas` |
| 8 | box-drawing frame line `┌─┤ label ├ … ┐` | h 15 | h 15 | (nav row, h 20) |
| 9 | THE BODY — the only part about this widget's data | title/context/actions | the porch, h 150 | the day grid |
| 10 | ledger line: 1px rule + kaomoji + one live figure | h 18 | h 16 | h 18 |
| 11 | closing line ending on `𝄂` in gold | h 18 | h 18 | h 14 |

The entablature row is the same three elements every time:

```qml
// notifications.qml            AudioColonnade.qml         calendar.qml
crown  ❧ serif 20 bold sig     ♫ music 22 sig             𝄴 music 24 clay
name   "HERALD" serif 13       "AUDIO" serif 15           "KALENDAE" serif 15
       DemiBold, spacing 3     DemiBold, spacing 4        DemiBold, spacing 4
tag    "[ notify ]" mono 10    "[ pipewire ]" mono 10     "[ fasti ]" mono 10
       sig @ 0.55              sig @ 0.9                  clay @ 0.9
```

**That is the whole trick.** The skeleton is fixed and shared; what varies per
widget is exactly four things:

1. a **signature hue** — one `livery.*` role this widget owns, worn by its
   keyline, crown, frieze, frame and ledger rule (`base0F` rust for
   notifications, `base09` amber/clay for AudioColonnade and calendar);
2. a **crown glyph** where a clef would go (`❧`, `♫`, `𝄴`, `ϟ`);
3. a **carved name** plus a `[ token ]` tag;
4. the **frieze**, which is the only place a "concept" gets to show.

The metaphor is a *naming layer over the skeleton*. It renames the parts
(entablature, frieze, ledger, stylobate); it never replaces them and never
adds a part that isn't in the list. A widget that reads as foreign is almost
always a widget that skipped parts 1–8 and 10–11 and shipped only part 9.

**What varies legally.** Parts 1–6 and 11 are constant across every
standalone stele on this desktop. Parts 7, 8 and 10 substitute rather than
disappear: `calendar.qml` spends its frame line on a nav row instead
(`‹ month ›  [ ἔτος ⌄ ]  « year »`) because that row is where a scroll's
controls belong, and its ledger is the tally. What no widget does is *drop* a
band and leave the gap — the vertical rhythm is continuous top to bottom.

### The two chrome families

There are exactly two, and a new widget picks one:

- **The self-framed stele** — the eleven parts above. Used by every popout
  body and every card: `notifications.qml`, `calendar.qml`, `AudioColonnade`.
  Hosted bare (`StelePopout`/`SteleLayerPopout`) because it draws all its own
  chrome. Elements *inside* a larger surface wear a subset — `powermenu.qml`'s
  `EndingStele` keeps parts 2–4, 7, 8 and 11 (anchored, not in a `Column`, and
  no cast shadow: it floats on a scrim) and promotes its glyph to the body,
  since the surface's one crown lives in the heading;
  `launcher.qml`'s `GlassPage` keeps the body, border, gloss and keylines and
  puts its identity in a chapter head and a folio. **A surface gets one crown,
  not one per bay.**
- **The ruled sheet** — `bar.qml` only: an opaque strip with drawn structure
  (five staff lines, barlines, playhead) instead of a frame, on which cells
  are written as notation. One surface in the house wears this, and it is the
  bar; a new widget almost certainly wants the stele.

---

## 2. Spend colour like it costs money

Count every `livery.*` read in each widget (`grep -o "livery\.[a-zA-Z0-9]*"`):

| widget | ink + ground | accent | hot | everything else |
|---|---|---|---|---|
| `launcher.qml` | 40 | 19 | **2** | 4 |
| `powermenu.qml` | 17 | 3 | **2** | 6 |
| `notifications.qml` | 8 | 4 | **2** | 4 |
| `calendar.qml` | 4 | 4 | **2** | 3 |
| `bar.qml` | 13 | 10 | 0 | 9 |

The pattern holds across four unrelated files: **most of the surface is ink on
ground.** The laurel `paletteHot` appears exactly twice per file, and both
reads always serve ONE state — the selected row's rule *and* its margin mark
(launcher), the hovered stele's keyline *and* its `♪` tick (powermenu), the
default action's border *and* its 0.16 fill (notifications), today's cell in
the compact grid *and* in the mini grid (calendar). Colour beyond that is a
ROLE doing a job once — aegean for an information layer, murex for one stele's
signature, terracotta for one urgent state.

`bar.qml` is the deliberate exception: its accent count nearly matches its ink
count because khoa directed the window title and volume cell to be primary-hue
ink. That is a documented direction in the file's own header, not a licence to
spread colour elsewhere.

**The rule to author by:** decide up front which single element is the live/
selected one, give it laurel, and let everything else rest in ink at a
stepped opacity. If you can't name the one hot element, the design isn't
finished yet.

---

## 3. The opacity ladder is the real design tool

Nothing in this house dims by eye. The same ladder recurs file to file
(full table in `greek-grammar.md`); this is what it BUYS:

```qml
// calendar.qml — a 42-cell grid stays readable with no boxes and no colour
color: parent.isToday   ? root.livery.paletteBg                 // on laurel fill
     : parent.isCurrent ? (parent.isSunday ? withA(clay, 0.95)  // rubric
                                           : root.ink)          // 1.0 — this month
                        : withA(root.ink, 0.3)                  // spill days
```

Four tiers, one hue family, and the month reads instantly. The ISO week rail
beside it sits at `0.32`, dropping to `0.65` only on the row holding today —
the information layer is *present but never competes*.

`launcher.qml` does the same at row level: an unselected entry is
`paletteFg` at `opacity: 0.88` over a `0.13` hairline rule; selecting it moves
the text to `1.0`/Bold and ignites the rule to 2px laurel. Two steps of
opacity plus one colour is an entire selection system.

**If a widget needs a box, a fill or a second colour to separate two things,
try one step of the ladder first.** That is what makes the family look like
one hand drew it.

---

## 4. Rest must read; hover only PROMOTES

Every element that matters is fully drawn before the pointer arrives:

- `powermenu.qml`'s six steles carry their glyph, name, Greek whisper and
  frieze at rest; hover adds `scale: 1.045` (OutBack, 170ms), a 7px lift,
  the laurel keyline and a `♪` tick that **fades** in so the name never jumps.
- `launcher.qml`'s rows show name, icon and generic-name gloss at rest;
  selection thickens the rule and bolds the name.
- `bar.qml`'s cells all show glyph + value at rest; hover adds opacity and an
  underline (`font.underline: root.audioShown`).

No widget hides its identity behind hover. A surface whose content only
appears on hover reads as unfinished, because at rest it *is* unfinished.

---

## 5. Texture comes from live data, not ornament

Every stele carries at least one small, live, computed figure — mono, 8–11px,
in the corner where a ledger would keep it:

- `notifications.qml` — the arrival clock, `HH:MM`, refreshed on a 30s
  `Timer` "so a persistent critical card never shows a stale time". The
  urgency word deliberately moved out of the ledger into the bottom frame
  label, because "nothing reads twice".
- `calendar.qml` — `day 15 of 31` at home, `wound +3m` while browsing,
  `meton 12 of 19 · 12 months` when expanded, plus the memento's
  `N days · M weeks left` computed from the REAL today, not the browsed year.
- `launcher.qml` — folios: `XLII δαίμονες` on the left, `II / VII` on the
  right, in Roman numerals.
- `AudioColonnade` — `└─┤ out 63 · in 40 ├` in the closing frame.

This is what makes the objects feel inhabited rather than decorated. It is
also cheap: one function and one `Text`. **A new widget with no live figure
anywhere is missing the ingredient that carries the most weight per line.**

The second texture source is a state-keyed kaomoji, one per widget, always
tied to a live read — never free decoration (`kaomojiFor()` in
`notifications.qml`/`calendar.qml`/`AudioColonnade`, `kaomojiSet` in
`bar.qml`, `scatterFaces` in `launcher.qml`).

---

## 6. The two ways this goes wrong

Both are documented failures, on this desktop, in one sitting: three separate
redesigns of ONE bar popout (the system tray) were each rejected on sight by
khoa on 2026-08-15, and the popout body was then removed from `bar.qml`
entirely, leaving only the toggle and a comment. The three misses were not
random — they were two failure modes, one of them twice.

### Failure mode A — OVER-CONCEPTUALIZING

*What it looked like:* a new invented object (a votive inscribed tablet in a
temple bay), a carved wordmark, an invented grid system for the icons, an
invented rubrication scheme for the attention state, and a ~30-line design
record in the file header arguing for all of it.
*Verdict:* "nothing like the other widgets", "does not look the same quality".

*Why it happens:* the existing widgets have rich metaphors and long headers,
so the metaphor and the header look like the thing to copy. They are not. The
metaphor in a good file is *downstream* of the skeleton — `notifications.qml`
picked "Tuscan" precisely because Tuscan is the order that carries NO ornament
frieze, i.e. the concept was chosen to justify doing LESS, and the file's
frieze is two flat rules 4px apart.

*Antidote:*
- Build parts 1–11 first, at the sizes in §1. Then choose a name for the
  parts. If the concept demands a part that isn't in the list, drop the
  concept, not the list.
- A header records decisions and grounding, not arguments. Length is not the
  tell: `notifications.qml`'s order account is four bullets over 19 lines. The
  tell is that every one of them settles something and says what it cost —
  ORDER (no ornament frieze, and why that is the identity), SIGNATURE (the one
  accentSpread slot no other surface claims), CROWN (a stand-in where no clef
  fits), NO ICONS (dated, dropped, with what replaced them). None of it argues
  that the concept is legitimate.
- Never invent a second layout system. There is one: `Column`/`Row`/`Item`
  with explicit `width: parent.width` and anchors between siblings.

### Failure mode B — UNDER-REALIZING

*What it looked like, twice:* (i) bare SNI icons in a `Flow`, one 1px rule,
one footnote that only appeared on hover; (ii) one row per item with an
icon, a name, a dim detail line and a per-row rule.
*Verdict:* "looks exactly like how it was" — and, asked directly, khoa chose
"too plain, needs real character". The third, richer attempt still drew
"nothing like the other widgets on the system".

*Why it happens:* after a rejection for excess, stripping feels like the safe
correction, and a small payload (a handful of tray icons) feels like it can't
carry chrome. Both are wrong here. Attempt (i) also failed the §4 rule — the
only thing distinguishing it from a bare icon row was gated behind hover, so
at rest there was nothing to read.

*Antidote — the payload argument, settled:* `AudioColonnade`'s entire data
payload is **two integers and two booleans**, and it is 634 lines: a distyle
porch with two distinct column orders, a fill meter per shaft, a ruin state
per shaft, a shared architrave and stylobate, tallies, a bead-and-reel frieze,
a kaomoji and two box-drawing frames. Any list of live items is a richer
payload than that. "There wasn't enough content to justify chrome" is not
available as a reason.

The floor for ANY new stele, however small its data, is the eleven parts. That
is roughly 120 lines of chrome before the body starts, and that is correct.

### The one-line test for both

Screenshot the new widget next to `notifications.qml` on the same desktop
(§12). If the two do not look like they were drawn by the same hand at the same
scale, the answer is never "add a concept" or "remove things" — it is
"the part list, at the sizes in §1".

---

## 7. Worked example — `widgets/notifications.qml` (the quality bar)

544 lines for a card that shows an app name, a title, a message, up to a few
buttons and a clock. Held up repeatedly as what the others should match.
What it actually does:

**Width is fixed at 360** — `width: 360`, matching the stack that hosts it
(`AoideNotifications.qml`: `implicitWidth: 360`, `Column { width: 360 }`).
The card never negotiates size with its host; it declares its footprint and
the host follows. `implicitHeight: stele.height + 5` — the +5 is exactly the
cast shadow's overhang, so the stack's spacing stays honest.

**The reading order is three differentiated tiers, and nothing reads twice.**

```
// schematic, not a capture — the three tiers, the cut between them,
// and what each is drawn in:
//  ┌─┤ firefox ├──────────┐     ← program frame, mono 11, signature @0.95
//  ─────────────────────────    ← the cut, 1px signature @0.55
//  Download finished            ← title, SERIF BOLD 14, full ink
//  │ archive.tar.gz  (2.1 MB)   ← context, 12px @0.85, indented 10px behind
//                                 a 1px signature hairline @0.45
```

The program name is resolved smartly rather than trusted:
`desktopEntry` (with `.desktop` stripped) → `appName` → `"notice"`, because
terminal-forwarded OSC-9 notifications carry the forwarder as `appName`. And
when the body is empty and the whole `"Title: message"` string landed in
`summary`, `splitAt()` splits it at the first `": "` — but only when the head
looks like a real title (`idx > 5 && idx < 60`, and it contains a space, so
`"Re:"` and `"12:30"` are left alone). **Data shaping is part of the design
work here.** Two tiers on screen only exist because the file went and got
them out of a mashed string.

**Where colour is spent:** the whole card is signature-hue chrome plus ink
text. Two elements break out — the arrival clock (gold) and the FIRST action
button (laurel border + a 0.16 fill of the same hue), with every other action
in gold. One laurel per surface, and it marks the default choice.

**What it deliberately does NOT have**, all four recorded in its header:

- **no icons.** An earlier version rendered the sender's themed `appIcon`;
  this build "carries the app name straight into the TUI top-frame label
  instead, matching how the other temples caption their own body."
- **no ornament frieze** — the Tuscan order's bareness IS its identity, so the
  frieze is two flat rules (`sig @0.7` and `sig @0.35`, 4px apart).
- **no Greek subtitle**, unlike calendar/powermenu/launcher — "consistent with
  its own stated austerity".
- **no `QtQuick.Layouts`** — a v1 card hit a real sizing-negotiation loop
  (`RangeError: Maximum call stack size exceeded`, live in journalctl) the
  moment wrapped body text rendered. See `hazards.md`.

**State is legible without a badge:** critical swaps the signature hue to
terracotta *throughout* (keyline, crown, frieze, frames, title, clock) and
breathes the whole stele on a 700ms `SequentialAnimation on opacity` between
0.8 and 1.0 — "so a persistent card still reads as LIVE, not stuck." No
"CRITICAL" label anywhere; the urgency word appears once, in the bottom frame.

**Behaviour is grounded, not guessed.** `closeNotification()` never writes
`tracked = false`, and the comment traces the exact `server.cpp` /
`notification.cpp` call path proving why a first version's `tracked = false`
emitted a second, wrong-reason `NotificationClosed` D-Bus signal. Action
clicks call `invoke()` alone, because `invoke()` already closes the
notification — a first version also called `dismiss()` and logged "Cannot
close destroyed notification" on every click. **That is what a good header
line looks like: a fact that was checked, and what broke before it was.**

---

## 8. Worked example — `widgets/powermenu.qml` (six of one thing)

The problem: six actions that must read as siblings AND be told apart at a
glance. The solution is the general answer for any repeated element in this
house — **one shared chrome, four per-item variables, driven off a data
array:**

```qml
readonly property var endings: [
    { action: "lock",    name: "LOCK",    term: "fermata",
      greek: "κλείς",    glyph: "𝄐", gs: 56,
      frieze: "◆───◆───◆", hue: root.livery.paletteAccent },
    …
]
```

Per item: a real notation glyph for how music stops, its musical term, a Greek
whisper, its own frieze string, and a signature hue. Shared: the 196×306 bay,
`radius: 0`, 2px `paletteFg` border, gloss sheen, inset keyline at
`margins: 5`, the `┌ ┤ term ├ ┐` top frame and the `└ ┤ 𝄂 ├ ┘` bottom frame,
the carved 15px DemiBold name at letterSpacing 4.

Severity is encoded in the ORDER of the hues left to right — gold, verdigris,
murex, aegean, clay, terracotta — not in six differently-shaped cards.

**The one laurel** is the selection, and it is three coordinated changes on
one `lit` flag: keyline colour and width (1px hue@0.7 → 2px `paletteHot`), the
`♪` tick fading in beside the name, and the glyph going to full alpha. The
tick is anchored OUTSIDE the centring and fades rather than toggling
`visible`, "so the name never jumps when the laurel arrives" — that kind of
care is most of the difference between "fine" and "same quality".

**Inverted ink on the scrim.** The heading sits on the dimmed desktop, not on
a stele, so it flips: `ΕΞΟΔΟΣ` is `paletteBg` at 27px/letterSpacing 12, the
subtitle `paletteBg @0.75`, the stay-line `paletteBg @0.7`. Ink is for
surfaces; over the ink scrim, the ground colour becomes the ink.

**Motion is staging, not decoration.** One linear master clock (`rise`), each
card windowing its own slice (`stag = 0.085`) and applying OutCubic *inside*
its window, unswinging a true Y-axis `Rotation` from 72° to flat. Opening
560ms, closing 200ms. Then one flourish per glyph while hovered — the fermata
breathes, the coda rocks, the grand pause dies away, the tacet sinks, da capo
wheels, fine cinches — every one resetting its animated value in `onStopped`,
and the sinking/cinching riding transforms "so the text stack below never
moves".

---

## 9. Worked example — `widgets/calendar.qml` (density)

1293 lines, and the reason to study it is that it puts a 42-cell grid, an ISO
week rail, eight moon phases, a computed Attic lunisolar year and a memento
mori on one sheet without any of it turning to noise.

**How density stays readable:**

- Cells are `26×17` (compact) and `15×11` (expanded mini-grids) with `radius:
  0` and `color: "transparent"` — no cell boxes at all. The only filled cell
  in the entire grid is today, in `paletteHot`, with its numeral in
  `paletteBg`. One fill, one hue, whole month.
- The opacity ladder does the separating (§3).
- Type sizes step hard: serif 12 for day numerals, mono 8 for the week rail,
  mono 7 for the mini rail, 7–9px for phase glyphs. **Micro-tiers are legal
  here as long as they are informational** — nothing at 7px is ever the thing
  you came for.
- The information layer is one hue, aegean (`holoBlue`), and it never carries
  chrome: noumenia marks, Attic month names, phase-glyph fallback ink.

**Where it draws the line:** the moon phases are the ONE glyph family wearing
fixed colour outside `livery.*` — real emoji `U+1F311`–`U+1F318`, sanctioned
explicitly in the header, with the monochrome fallback (`U+FE0E` → DejaVu
Sans, inked aegean) recorded as the rollback path. A sanctioned exception
looks like that: named, scoped to one family, tested against alternatives, and
carrying its own way back.

**Sizing discipline as a design constraint:** `implicitWidth`/`implicitHeight`
are constants WITHIN a mode. Paging never resizes the window; the open-unfurl
animates painted heights only; the compact-to-expanded toggle is the single
sanctioned window resize and it STEPS the window, sequenced around the morph,
because Hyprland re-maps an xdg_popup on the first resize of a visible popup
and plays its popup animation over the remap. Motion is paint, not geometry —
see `hazards.md`.

**Paging is material, not a swap:** `page(delta)` grabs the body's current
face with `grabToImage`, swaps the data underneath, and slides ghost and live
layer together through one clip — "forward in time pulls new days up from the
bottom roll". 220ms OutCubic. A failed grab degrades to an instant swap. That
is the house standard for "make the change feel physical without moving the
window".

---

## 10. Worked example (short) — `AudioColonnade.qml`, the bar-popout reference

Facet-side (`modules/facets/quickshell/qml/AudioColonnade.qml`), not a song
widget — but it is the live precedent for **a bar popout body**, so a new
popout should be read against it rather than against the bar itself.

- Hosted BARE through `StelePopout` (khoa, 2026-07-31 standing direction: new
  bar widgets converge their outer panel on the stele family, not
  `GadgetFrame` — wrapping a self-framed stele in a frame double-frames it).
- Declares `width: 268` and `implicitHeight: stele.height + 5`; the host sizes
  itself from those two numbers.
- Wears the full eleven parts for a payload of four values.
- Differentiates its two channels by SHAPE as well as hue — Ionic volute
  capital + aegean for the input, Doric echinus + gold for the output — so the
  two are told apart with the colour removed.
- Encodes a boolean state as a silhouette: a muted channel is a BROKEN COLUMN
  (capital gone, shaft snapped along a jagged break, rubble scattered at the
  foot). No slash, no red, no "MUTED" badge on the column itself; the word
  appears once, in the tally beneath. **This is the house's best example of
  "make the state legible from the shape".**

---

## 11. The bench check

Run this before calling a widget done. Every line is a rule with a live
precedent above.

**Skeleton**
- [ ] All eleven parts present or deliberately substituted (§1), at the stated
      sizes, in order — no band dropped leaving a gap.
- [ ] One signature hue, one crown glyph, one carved name, one `[ tag ]`.
- [ ] `radius: 0` on every panel/card/cell; 2px `paletteFg` border on the body.
- [ ] Cast shadow `+4`/`+5` at ink 0.22, and `implicitHeight` clears it (`+5`).
- [ ] Content `Column` at `margins: 10–12`, `spacing: 4`.

**Colour**
- [ ] Every colour from `livery.*` (exceptions are labeled in the header).
- [ ] Ink + ground dominate the read; laurel marks exactly one thing (its two
      reads serve one state); every other role does one job.
- [ ] Opacity steps come off the ladder in `greek-grammar.md`, not by eye.

**Legibility**
- [ ] Everything identifying is visible at rest; hover only promotes.
- [ ] At least one live computed figure, mono, in a ledger position.
- [ ] One state-keyed kaomoji, tied to a real state read.
- [ ] Each state readable with colour removed (shape, weight or position
      carries it too).

**Craft**
- [ ] Nothing reads twice (a value shown in the ledger is not repeated in the
      frame label).
- [ ] Nothing jumps when a state changes (fade or animate; don't toggle
      `visible` on something inside a centred layout).
- [ ] Motion sits in the house tiers (`widget-structure.md` §9).
- [ ] Every non-ASCII glyph you introduced has been seen on screen, at the
      face and weight you used (`hazards.md` §1).

**Header**
- [ ] Identity, slot, anchor kind, extras.
- [ ] Order/signature/crown/frieze in a few lines — decisions, not arguments.
- [ ] Dated iteration log entries for what changed and why.
- [ ] Every claim about engine/service behaviour cites what was checked.

---

## 12. Verifying it on screen

Agents cannot see the screen by default. A widget is not done until you have
looked at a screenshot of it. The loop, all verified on yomi-strix:

```bash
# 0. staging must be unlocked, or `rice stage` refuses with
#    reason "declarative-mode-locked" (the default mode is declarative)
aoide rice mode status
aoide rice mode stage sonata

# 1. push the edited widget bodies into the live runtime tree.
#    This syncs song/songbook/<song>/widgets/*.qml -> run/qml/songs/<song>/
#    AND auto-triggers a Quickshell IPC reload *when a widget body changed*
#    (a no-op re-stage deliberately skips the reload).
aoide rice stage sonata

# 2. force a reload yourself when you need one anyway. The command is
#    `quickshell reload` (renamed from `shell reload` — `shell` collided
#    with `--agent shell`). No systemd restart; no-ops if qs isn't running.
aoide quickshell reload

# 3. a BRAND-NEW slot file the manifest has never seen needs a real restart:
systemctl --user restart aoide-quickshell.service

# 4. QML errors, binding loops, RangeErrors, console.warn from the slot
#    anchors — all land here:
journalctl --user -u aoide-quickshell.service -n 80 --no-pager

# 5. look at it
grim /tmp/full.png
magick /tmp/full.png -crop 800x600+1100+400 +repage /tmp/widget.png
```

Then read the crop back and judge it yourself before showing anyone. For a
popout you must trigger yourself (a hover popout, a summoned overlay), drive
it and screenshot in one shell line, or force the state in the QML
temporarily; `hyprctl dispatch global aoide:powermenu` and
`aoide:launcher` summon those two directly.

A parse error means the slot silently renders NOTHING — `WidgetSlot` logs
`[aoide/widgetslot] failed to load slot <name> - <error>` and moves on. An
empty-looking surface is a journalctl question first, a design question
second.
