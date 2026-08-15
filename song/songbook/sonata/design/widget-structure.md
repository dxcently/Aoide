# Widget Structure — how a sonata flavor widget is built

**Song:** sonata

Companion to `design/greek-grammar.md` (the motif/vocabulary doc). This file
covers the other half: the mechanism a widget file goes through from disk to
live render, and the structural/authoring conventions every current
`widgets/*.qml` file actually follows — verified against the live files
(`bar.qml`, `calendar.qml`, `notifications.qml`, `powermenu.qml`,
`launcher.qml`) as they stand today, not a shared blueprint imposed on them.
Where a convention is universal across all five it's stated as a rule; where
it isn't, the exception is named.

---

## 1. From file to screen, briefly

The full mechanism is `modules/facets/quickshell/qml/slots.md` and
`CONTRACTS.md §5` — this section only orients a widget author, it doesn't
restate them. A `song/songbook/<name>/widgets/<slot>.qml` file is carried by
the quickshell facet's build to `$out/qml/songs/<name>/<slot>.qml` and listed
in a generated `manifest.json`; nothing renders until a host surface embeds a
`WidgetSlot` (for an `Item`-rooted slot) or `SurfaceSlot` (for a
`PanelWindow`-rooted one) for that exact slot name. `StagingEngine`'s
`resolveSong` walks a two-rung baseline chain — the active song's own file,
else **sonata's** (this song IS the floor every other song falls back to) —
so a slot sonata dresses is never blank, even for a song that hasn't authored
it. `aoide rice stage`/`preview` hot-syncs an edit to an *existing* widget
file live (no rebuild); a brand-new slot file still needs a service restart
(the manifest is read once, at startup).

## 2. The fixed contract, from a widget author's seat

Every widget declares `required property var notes` and `required property
var bridge` — both anchors inject these unconditionally, so leaving either
out is a straight load error, not a silent gap. Slot-specific extras beyond
that pair are NOT uniform — check `slots.md`'s table for what a given slot's
anchor actually passes:

- `notifications` gets one extra, `notification` (the tracked model item).
- `launcher` gets `clipboard` and `ledger`.
- `bar` gets `stagingEngine`, `powermenu`, and `shared`.
- `calendar` and `powermenu` get nothing beyond the universal pair.

Not every extra is declared `required`, either — `bar.qml` declares
`stagingEngine` and `powermenu` as `required property var` (the bar cannot
function without them) but `shared` as a plain `property var shared: null`,
commented "Optional/null-safe so a standalone bar load never errors." A
widget author choosing between `required` and a null-defaulted optional is
choosing whether the widget should hard-fail or degrade gracefully when that
extra isn't wired — `bar.qml` demonstrates both choices side by side.

No widget receives `config.*`, ever — a song widget is store-copied score,
structurally incapable of reaching nix options through this surface
(`WidgetSlot.qml`/`SurfaceSlot.qml`/`StagingEngine.qml` all restate this in
their own headers).

## 3. Root type follows anchor kind

`WidgetSlot` is an `Item` that sizes itself off the loaded child's
`implicitWidth`/`implicitHeight` and parents it into an existing layout — so
a `WidgetSlot`-hosted widget's root must be an `Item` (`bar.qml`,
`calendar.qml`, `notifications.qml`). `SurfaceSlot` is a non-visual
`QtObject` that does no parenting/sizing at all — it just creates the widget
and hands the live instance back as `.item` — so a `SurfaceSlot`-hosted
widget's root IS its own `PanelWindow`, owning its own Overlay layer,
`WlrLayershell.namespace`, keyboard focus, and `GlobalShortcut`
(`powermenu.qml`, `launcher.qml`). The namespace a `PanelWindow`-rooted
widget declares (`aoide-powermenu`, `aoide-launcher`) is not a free choice —
it's what the compositor facet's layerrules match on for glass (§11 below),
so it travels with the slot as part of its contract, catalogued in
`slots.md`.

## 4. The comment-header block

Every widget file opens with a substantial `//` block before its `import`
lines. Reading across all five, the header consistently carries, though not
every widget needs every element:

- **Identity** — filename, slot name, one line on what it is.
- **Order breakdown** — which classical order it wears and why (ORDER /
  SIGNATURE / CROWN / FRIEZE, `notifications.qml`'s Tuscan account and
  `calendar.qml`'s Composite/volute account are the fullest examples).
- **Wiring** — anchor kind, extras needed, cross-references to `slots.md`/
  `CONTRACTS.md §5` rather than restating them.
- **A dated iteration log** — `calendar.qml`'s header carries six same-day
  dated rounds (2026-08-13) recording what changed and why round over round;
  this is the widget's own design record now that `greek-grammar.md` no
  longer carries a shared per-widget map (its own closing note says so
  explicitly: "this header is this widget's design record now").
- **Grounding citations** — a claim about engine behavior is backed by
  something checked, not assumed: `notifications.qml`'s header cites the
  compiled `quickshell-service-notifications.qmltypes`; its `closeNotification`
  comment traces the exact `server.cpp`/`notification.cpp` call path that
  made an earlier version's `tracked = false` write actively harmful, not
  just redundant.
- **A "why" for anything a first reader would question** — `bar.qml`
  explains why `WorkspaceRow` is a nested `component` and not the sibling
  file it used to be (§6 below); `powermenu.qml` explains why the wireCyan
  LOGOUT hue stays under the structural alpha cap even though it's a "hue,"
  not the traced role.

## 5. Section-marker banners — two tiers, verified

Every widget uses single-line box-drawing banners
(`// ── label ─────────…`, U+2500) to mark subsections — all five files carry
these, 11 to 24 times each. Four of the five (`bar.qml`, `calendar.qml`,
`powermenu.qml`, `launcher.qml`) additionally use a heavier double-line
banner (`// ══ LABEL ═══════…`, U+2550) for top-level zones — `bar.qml`'s
`HEAD`/`LEFT STAVE`/`CENTRE`/`RIGHT STAVE`/`POPOUTS`, `launcher.qml`'s
`THE INCANTATION STRIP`/`THE BOOK`. `notifications.qml` is the one file that
uses only the single-line tier throughout — it's also the shortest and
flattest of the five (one `Column` of parts, no competing zones to divide),
so the lighter banner alone is enough to read it. Not a violated rule, a
convention that scales down cleanly.

## 6. Helper sub-components: nested, not sibling files

`slots.md` documents a sibling-file mechanism for multi-file widgets — an
UPPERCASE filename alongside the slot file, carried by the build but never
independently resolvable as a slot. **No widget currently uses it.** Every
helper component that exists today — `bar.qml`'s `WorkspaceRow` and
`Barline`, `calendar.qml`'s `Roller`, `powermenu.qml`'s `EndingStele`,
`launcher.qml`'s `GlassPage`, `PageStack`, `BoardSegment`, `ManuscriptRow`,
and `BookPage` — is declared as a `component Name: BaseType { … }` block
nested directly inside the file's root `Item`/`PanelWindow`.

`bar.qml`'s header explains why, and the explanation is load-bearing for any
new widget that needs its own helper: `WorkspaceRow` **was** a sibling
`WorkspaceRow.qml`, but Quickshell's dynamic `Component.createObject(url)`
loading (how `WidgetSlot`/`SurfaceSlot` load every widget file) does not
reliably grant a loaded file visibility into custom types living in its own
directory — confirmed live, both the implicit same-directory rule and a
top-level file-scoped `component` failed ("WorkspaceRow is not a type" /
"Syntax error"). Nesting the component inside the root item sidesteps
cross-file resolution entirely. The sibling-file mechanism `slots.md`
catalogs is still build-supported and not deprecated — it just hasn't been
the mechanism any widget actually reaches for since this was discovered; a
new widget needing a helper should default to nesting, matching every
existing precedent, not the catalog's example.

## 7. Colour discipline: `notes.*` only, with two labeled exceptions

Every colour in every widget reads from `notes.*` (or a same-named local
alias, e.g. `calendar.qml`'s `readonly property color ink: notes.paletteFg`)
— confirmed across all five files, no bare hex in ordinary chrome. Two
exceptions exist, and both are explicitly labeled in their file's own header
as sanctioned, scoped departures, not oversights:

- **`bar.qml`'s structural ink is literal `#000000`.** The header states it
  directly: "the structural ink stays BLACK — staff lines, barlines,
  playhead — but the TEXT ink is now the song's umber (`notes.paletteFg`)."
  The manuscript-strip metaphor treats printed staff lines as ink-on-paper,
  not a themed chrome role, so it's the one widget where geometry ink is a
  literal, not a note.
- **`calendar.qml`'s moon-phase glyphs (`U+1F311`–`U+1F318`) render as real
  color emoji**, the file's header calling this out as "the ONE glyph family
  wearing fixed color outside the `notes.*` roles" — the three declared
  faces have no monochrome moon glyphs (fc-list checked), bare code points
  fall back to fixed-color Noto Color Emoji regardless, and the
  themeable-but-legible alternative (`U+FE0E` text presentation → DejaVu
  Sans monochrome) was overridden on khoa's explicit call in favor of the
  real emoji. The header records the monochrome fallback (aegean ink) as
  the documented rollback path should color emoji ever leave the rig.

A widget breaking the `notes.*`-only rule without a comment like these two is
the anomaly worth flagging in review, not a silent norm.

## 8. The role palette, in practice

`greek-grammar.md §3` is the canonical role→hue table; this is what it looks
like read off real widgets, one concrete example per role actually
exercised:

- **`paletteHot` (laurel) is the one-hot selection/today mark, everywhere.**
  `launcher.qml`'s selected `ManuscriptRow` ignites its rule and margin dot
  in `paletteHot`; `powermenu.qml`'s hovered `EndingStele` ticks its ♪ mark
  in `paletteHot`; `calendar.qml`'s "isToday" cell fills `paletteHot`;
  `notifications.qml`'s first (or default) action button is the one
  `paletteHot`-bordered "laurel" button among its actions. Same role, same
  read, four unrelated widgets.
- **`paletteAccent` (gold) marks open/active/live states.** `bar.qml`'s
  active workspace note-head fills it; `launcher.qml`'s incantation strip's
  ♪ prompt and gold keyline sit in it; `calendar.qml`'s browsed year number
  sits in it.
- **`paletteUrgent`/`glitchPink` marks blocked/critical states.**
  `bar.qml`'s blocked `✎N` cell and low-battery readout pulse it;
  `notifications.qml`'s critical card overrides its whole signature hue to
  it.
- **`holoBlue` (aegean) is the preview/information role, never chrome.**
  `bar.qml`'s hover-preview ring on the workspace row and the mic cell both
  read it; `calendar.qml`'s `aegean` alias (`notes.holoBlue`) inks every
  noumenia/moon-phase mark — the lunar layer is explicitly "the information
  layer," per the widget's own comment; `launcher.qml`'s generic-name gloss
  text reads it; `powermenu.qml`'s HIBERNATE stele hue is it.
- **`wireCyan` (verdigris) is structural, and its alpha cap is still real —
  see below.**

**The structural cap, still enforced, no longer written down.** A ≤0.5-alpha
ceiling on `wireCyan`'s *structural* use (never the hue itself) was once an
explicit invariant in this file's own §5, before the 2026-08-14 retcon —
`design/intent.md`'s iteration log still records that it existed. It isn't
restated anywhere in `design/` today, but it's still followed in the actual
code: every live structural `wireCyan` read sits at exactly 0.35 —
`powermenu.qml`'s "ΕΞΟΔΟΣ" title rule and, independently, its LOGOUT stele's
signature-hue rationale ("the ≤0.5-alpha cap binds the STRUCTURAL role, not
the hue... stays ≤0.35"); `launcher.qml`'s Grimoire page's inner illuminated
border. Both widgets cite the ≤0.5 ceiling in their own header comments —
the discipline survived the retcon in the code and in scattered widget
headers, just not in the design doc that used to state it once, centrally.
This file restates it since nothing else in `design/` currently does.

## 9. Geometry idioms

- **`radius: 0` everywhere, no exceptions found.** Every stele, card, popout
  body, and page in all five widgets is hard-cornered — matching the
  compositor facet's own `decoration.rounding = 0`
  (`modules/facets/compositor/default.nix`) and its comment: "Edged
  windows... The Pantheon wireframe language wants hard outlines too." A
  rounded corner anywhere in a widget would be the one visibly foreign
  element on this desktop.
- **Every stele's frame is a 2px `paletteFg` (or its `ink` alias) border.**
  `notifications.qml`'s card, `powermenu.qml`'s `EndingStele`,
  `calendar.qml`'s sheet, and `launcher.qml`'s `GlassPage`/incantation strip
  all draw `border.width: 2` in that colour — the one geometry constant
  every "stele" in this house shares regardless of which order it wears.
- **The offset cast-shadow is a shared, precisely-matched idiom.**
  `notifications.qml` draws it as one rectangle offset `+4`/`+5` at
  `withA(paletteFg, 0.22)`, citing "shared pantheon idiom
  (PowerVitalsGadget/MetersGadget)." `calendar.qml` draws the same shadow as
  two strips (right + bottom, not a full offset rect — its sheet cuts a
  transparent window over the day grid now, so an underlapping rect would
  tint that view) but at the identical `0.22` alpha and identical `4`/`5` px
  offsets, citing "shared stele idiom." Two files, months apart, landing on
  the same two magic numbers is the idiom, not a coincidence.
- **The Aero gloss-sheen gradient marks glass, not paper.** A four-stop
  white gradient (`rgba(1,1,1,0.13 → 0.03 → 0.00 → 0.03)`) rides
  `powermenu.qml`'s `EndingStele` and `launcher.qml`'s `GlassPage`/
  incantation strip — both are compositor-blurred `PanelWindow` overlays.
  It's absent from `notifications.qml` and `calendar.qml` (bare
  `StelePopout`/`SteleLayerPopout` hosting, opaque "paper" reads by
  design) and from `bar.qml` (the manuscript strip's gloss was removed
  outright — `intent.md`'s iteration log: "the Aero-style white gloss
  gradient overlay on the sheet is REMOVED entirely"). The sheen is a glass
  tell, not a universal stele finish.

## 10. Recurring texture

- **Every widget carries at least one state-keyed kaomoji.** `bar.qml`'s
  `kaomojiSet` (empty-title), `notifications.qml`'s `kaomojiFor()`
  (urgency-keyed), `calendar.qml`'s `kaomojiFor()` (browse-direction-keyed),
  `launcher.qml`'s `scatterFaces` (no-match search) and empty-message
  kaomoji, `powermenu.qml`'s stay-line "esc μένε — stay ( ˘ω˘ )ﾉ". Never
  decoration alone — always tied to a live state read.
- **A bilingual Latin/English-title + Greek-subtitle pairing appears on
  three of five, not all.** `calendar.qml`'s "KALENDAE" answered by italic
  "ἡμερολόγιον" beneath it (the widget's own header names this "the
  AoidePanel 'AOIDE ⁄ ᾠδή' bilingual-inscription idiom"); `powermenu.qml`'s
  "ΕΞΟΔΟΣ" answered by "the closing song — πῶς τελευτᾷ ἡ ᾠδή;";
  `launcher.qml`'s English chapter titles answered by Greek whispers
  ("MOST SUMMONED" / "ἕξις · σελίς I"). `notifications.qml` stays
  English-only ("HERALD") — consistent with its own stated austerity, the
  Tuscan order carrying no ornament frieze either.
- **The `<noun>.<qualifier>` callout token isn't only for order-marked
  surfaces.** `greek-grammar.md §1`'s table assigns Greek letters to nine
  tokens (`battery.gauge` among them — `bar.qml`'s battery `BarPopout` is
  literally titled `"battery.gauge"`, confirming Ξ is live where the table
  says it is). But the token shape itself is the general popout-naming
  convention, not scoped to the nine: `bar.qml`'s tray `BarPopout` is titled
  `"tray.held"`, a token with no assigned order-mark at all — the Greek
  letters are a labeling layer over an already-general callout-id scheme,
  not a requirement every popout must satisfy to be named this way.

## 11. The glass — how a popout becomes a stele

The frosted-glass feel that makes a popout read as a framed stele rather than
a flat rectangle is compositor-side, not drawable in QML alone
(`modules/facets/compositor/default.nix`):

- **`layerrule = blur on` + `ignore_alpha 0.05`** per Wayland layer
  namespace (`aoide-dock`, `aoide-launcher`, `aoide-powermenu`) blurs
  whatever sits behind that surface and stops fully-transparent regions from
  rendering as a flat grey stripe.
- **`layerrule = blur_popups on, match:namespace aoide-bar`** extends the
  same blur to the bar's `xdg_popup` children specifically — this is what
  frosts `notifications.qml`'s and every `BarPopout`-hosted card without
  either widget touching a compositor option.
- **`plugin:hyprglass`** layers actual refraction/fresnel ("Liquid Glass")
  on top of the flat blur for the same namespace set, plus a global
  `manage_window_blur` toggle that also glasses translucent *windows* (the
  kitty terminal) since hyprglass has no per-window targeting in this
  version.
- **A widget can opt OUT of the shared blur, per-namespace, on its own
  layer surface.** `calendar.qml` moved to its own `SteleLayerPopout`
  (`WlrLayershell.namespace: "aoide-calendar"`) specifically so
  `layerrule = blur off, match:namespace aoide-calendar` could exempt it
  from `aoide-bar`'s `blur_popups` rule — its papyrus sheet cuts a
  transparent window over the day grid that has to show the desktop
  crisply, not frosted. This is the concrete proof the glass system is a
  per-namespace dial a widget can reach for, not a blanket effect baked into
  `BarPopout`/`StelePopout` chrome.

None of this lives in the widget file itself — a widget earns the frosted
"stele" read by which host anchor and namespace it's hosted under, not by
anything it paints. A widget author changing how "glassy" a popout feels is
editing `modules/facets/compositor/default.nix`'s layerrule list (a facet
change, out of a song widget's own scope) or choosing which host chrome
(`BarPopout`/`StelePopout`/`SteleLayerPopout`) to hang under — not adding a
blur effect in QML.
