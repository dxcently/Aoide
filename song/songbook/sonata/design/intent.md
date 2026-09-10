# sonata — Design Intent

**Song:** sonata (committed rice; the selected key on yomi-strix)
**Palette:** a Greek marble key — pale warm-marble ground, plum-charcoal ink,
deep Attic-gold chrome accent, terracotta urgent, a true laurel leaf-green
one-hot blaze; aegean blue steps back to a preview/information role
**Grammar:** `design/greek-grammar.md` — sonata's drawing vocabulary: faces,
glyph sets, type tiers, the opacity ladder, rule idioms, state treatments, the
state-glyph contract shared verbatim with `theme.rs`, and the role→hue palette
below. A deliberate divergence from the Pantheon wireframe grammar the retired
`default` song once drew (kept as historical reference at
`docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`)
**Cover:** none — the wallpaper note is `null`, so the stylix facet bakes a
deterministic **bright-marble field** from `palette.bg` (`#f2ebde`) — the
same null-wallpaper-to-solid mechanism the stylix facet applies to any song
with no cover note. Colours fit the theme, not a
photo; the wallpaper switcher handles photos live (see the Iteration Log)

---

## The design shelf — where to look

Per-song design memory lives here, not in the wiki. Four files, in reading
order for someone about to draw something:

| file | answers |
|---|---|
| `making-a-widget.md` | **what makes a widget read as belonging to this desktop** — the shared skeleton with its real numbers, how colour and opacity are spent, the two ways new widgets fail here, worked walkthroughs of the live ones, and how to verify on screen |
| `greek-grammar.md` | the vocabulary to draw with — faces, glyph sets, type tiers, the opacity ladder, rule idioms, state treatments, role→hue |
| `widget-structure.md` | the mechanism and the hard contracts — slot pipeline, injected props, root type by anchor kind, host chrome (free vs must-supply), geometry constants, motion tiers, file conventions |
| `hazards.md` | what has actually failed live — glyph rendering, layout loops, dynamic-loading limits, popup/service traps, verification traps |
| `intent.md` (this file) | the KEY itself — why this palette, region by region, with computed contrast and the dated iteration log |

Each widget's own file header is its individual design record; there is no
shared per-widget blueprint anywhere.

---

## Why this song exists

sonata is the light key currently performed on yomi-strix. Any host in the
fleet performs it with one line — `aoide.song = "sonata";` — and the whole
`aoide.livery` fan-out swaps with zero other edits.

## Host-agnostic by construction

This song sets ONLY `aoide.livery`. It names no host, enables no facet or
dendrite, touches no hardware or service, and does not key `stylix.polarity`
(the stylix facet pins polarity light — a song cannot flip it). The venue
(host) decides its instruments; sonata carries only the notes. That is exactly
what lets one score be performed on any host with its own specifics and its own
enabled facet/dendrite set (CONTRACTS.md §5).

## The key — the Greek register, region by region

The palette is authored as a self-standing **Greek register**, not read from a
cover image: a temple in daylight. It is a marble ground under dark ink, with
chrome drawn from a temple's own materials — aegean sea-blue, terracotta clay,
Attic gold, laurel green, bronze verdigris, Tyrian purple, deep bronze.

Because the desktop is a **marble frosted-glass** surface — "brightness is
opacity, not colour": kitty renders at `background_opacity` 0.86 and the bar
sheet at ~0.30 opacity **over** this ground — `base00` stays in the pale
marble-glass family, a warm sunlit stone.

The polarity is **light** (the stylix facet pins it): `base00` is the lightest
value and `base05`–`base07` are the dark inks.

| Region of the Greek register | Slot(s) | Colour |
|---|---|---|
| pale sunlit marble (temple stone in daylight) | `base00` / `bg` | `#f2ebde` |
| marble in soft shade | `base01` | `#e8dfcc` |
| aged / weathered marble | `base02` | `#dacdb2` |
| weathered-stone grey-tan | `base03` | `#a99a82` |
| stone-slate midtone | `base04` | `#6d6250` |
| plum-charcoal ink | `base05` / `fg` | `#2f2a33` |
| obsidian-plum (deeper ink) | `base06` | `#211d26` |
| near-black ink | `base07` | `#14111a` |
| terracotta (fired clay) | `base08` / `urgent` | `#b0472f` |
| kiln-fired clay orange | `base09` | `#c06a35` |
| deep Attic gold (chrome accent) | `base0A` / `accent` | `#a07414` |
| true laurel leaf-green (one-hot blaze) | `base0B` / `hot` | `#4e8b45` |
| bronze verdigris | `base0C` | `#3f867e` |
| aegean deep-blue (preview / info role) | `base0D` | `#345f81` |
| Tyrian / murex purple | `base0E` | `#6f4373` |
| deep bronze / clay | `base0F` | `#8a5a34` |

The deep Attic **gold** is the chrome **accent** (`palette.accent` = `base0A`);
the terracotta is **urgent**; the laurel leaf-green is the single one-hot
**trace** colour (`palette.hot`, `base0B`) — the one blaze the grammar reserves
for the live/traced element, kept 71° off the gold hue so the two never muddy.
The **aegean blue** (`base0D`) steps back from chrome to a **preview /
information** role — the workspace preview ring, links, cool syntax ("the sea
between the columns"), never active-state chrome. The roles carry over from the
previous key by NAME (`wireCyan`=base0C, `holoBlue`=base0D, `violet`=base0E,
`glitchPink`=base08), each re-hued to its Greek equivalent: bronze-verdigris,
aegean, Tyrian/murex purple, terracotta.

## Window frames

Active border is `base0A` Attic gold (`#a07414`, = `palette.accent`) — the
focused window reads as the "live" one, matching the bar's chrome accent. The
inactive border steps back to `base0C` bronze-verdigris (`#3f867e`), a cool
blue-teal that recedes without vanishing into the ground.

## Component tier

`bar` and `notif` are all-null (palette everywhere) — the key does the work,
no hidden overrides to undo before a transposition. Only `window` carries the
song-owned border notes above.

## Contrast (light polarity, computed — not eyeballed)

WCAG relative-luminance ratios against `base00` (`#f2ebde`):

| Pair | Ratio | Requirement | Result |
|---|---|---|---|
| `base05` on `base00` | 11.81:1 | ≥ 4.5:1 (AA body text) | PASS |
| `base04` on `base00` | 5.04:1 | ≥ 3:1 (dim fg) | PASS |
| accent gold `base0A` on `base00` | 3.54:1 | ≥ 3:1 (fill / line, not body) | PASS |
| urgent `base08` on `base00` | 4.68:1 | ≥ 3:1 | PASS |
| hot laurel `base0B` on `base00` | 3.47:1 | ≥ 3:1 (2px border + halo) | PASS |

Every required bar clears. The chrome **accent is now deep Attic gold**
(`base0A`, 3.54:1) — it replaces the aegean blue as `palette.accent` and unifies
with the retired `base0A` highlight (`#b98a2e`, which under-contrasted at
2.63:1). Gold at 3.54:1 is a **fill / line colour, never running body text**
(< 4.5 AA) — the same standing rule as the verdigris structural cap. The one-hot
laurel blaze (`base0B` `#4e8b45`, 3.47:1) replaces the previous olive `#6b8b33`:
Fable found the olive only 41° off the new gold, muddying the one-hot; true
leaf-green sits 71° off gold and clears ≥ 3:1 for its 2px traced border + halo.
The aegean blue (`base0D`, 5.71:1) and bronze-verdigris (`base0C`, 3.60:1) are
now the preview/info and structural chrome tiers respectively — chrome, never
bare running text.

## Current surface elements (as performed)

How the key reads on yomi-strix — sonata's instantiation of its house grammar
(`design/greek-grammar.md`). The glass alphas are facet/dendrite constants, not
livery notes; they are tuned against THIS key and recorded here as its design
memory. The colour values below are the re-keyed Greek notes; the drawn shapes
are each songbook widget's (`bar`/`calendar`/`launcher`/`notifications`/
`powermenu`.qml) own concern now — each documents its own chrome in its own
file header rather than a shared cross-cutting blueprint:

| Surface | Element | Value |
|---|---|---|
| bar sheet (`AoideBar.qml`) | `paletteBg` alpha | 1.0 — OPAQUE, no glass, no gloss gradient (khoa: flat solid strip) |
| bar popouts (`BarPopout.qml`) | glass alpha | 0.72 (the popout reads solid against the thinned strip) |
| kitty terminal (kitty dendrite) | `background_opacity` | 0.86 (compositor fades unfocused windows to 0.80) |
| launcher (`AoideLauncher.qml`) | glass alpha (over busy windows) | 0.72 |
| dock + gadget frames (`AoideAgentWidgets`/`GadgetFrame`) | glass alpha | 0.72 |
| workspace strip | resting notes | solid plum-charcoal ink (`paletteFg` `#2f2a33`) |
| workspace strip | active note | swells 15→19px, fills `accent` gold `#a07414` on a soft accent glow |
| workspace strip | urgent | pulses `glitchPink` (`base08` terracotta `#b0472f`) |
| window frames | active / inactive | `#a07414` Attic gold hairline / `#3f867e` bronze-verdigris teal (2px, rounding 0) |
| the one blaze | ✎N live-sessions cell + DAG/TERMINALS trace | `hot` laurel leaf-green `#4e8b45` |
| Conductor's Codex agent book | turning leaf and settled spread | printed page geometry in a fixed 24×18 box; both faces carry fine text strokes, and the loop meets on the same printed spread; Terminals uses its common state lamp and nameplate |
| wallpaper | baked field | deterministic bright-marble solid from `palette.bg` `#f2ebde` (wallpaper note null) |

Every text element sits on a glass backing (bar sheet, frames, chips) — no bare
text on the wallpaper, so no outline treatment is in use.

## Session identity semantics in sonata widgets

Conductor and Terminals show the published title as their main heading, falling
back to the harness when absent. Harness and reported model share a secondary
line; missing models remain absent. Conductor puts its recovery handle on that
same line and suppresses a redundant harness when it already serves as the
heading without a model. Narrow cards retain two secondary lines; each continues to carry identity,
not a dedicated action bar. Conductor reserves a fixed animation lane left of
the heading in every state; Terminals adds no identity animation. A separate line carries the
petname and the real session ID's final four characters. These
are screenshot hints, not unique routing keys. Missing titles or petnames stay
absent; the widgets never mint substitutes.

Every card, including children and terminal rows, keeps recovery copy and details
inside the context menu. Hover never changes the identity labels’ available
width. Details retain the full session ID, published title, harness, model,
state, petname, working directory, prompt, native harness ID, PID/window, and
effective project name. Copy uses
Qt’s text clipboard without a shell command. Host is shown only when published;
missing host data remains unavailable. Session IDs remain unchanged for focus,
tracing, permissions, and row matching. Prompt content is displayed only from
the explicit prompt field, never inferred from the title.

Right-clicking a card opens the shared square `SessionMenu.qml` sheet; ordinary
left-click keeps focusing the session. The menu carries recovery copy/details,
the undying mark, project assignment, project creation and directory editing,
and process termination. Undying means retention for manual resurrection, not
automatic restart. Project roots accumulate through a native single-directory
picker or absolute path entry, with each root removable before saving. Existing
project names stay fixed while their directory roots are edited. Explicit
session project assignment wins over longest-directory-prefix grouping.

Actions cross `bridge.sessionAction`; the menu retains pending, success, or
failure text from the response. Termination reports a request, not confirmed
exit. Shared app process refusals stay visible. The menu adds no persistent row
to the cards and scrolls within the temple when its editor exceeds the viewport.

The kill action's label names its scope before the click rather than always
reading "Kill process": a subagent record shows a disabled "Kill (subagent)"
with a hint pointing at its executor session; a record that already owns a
dedicated process reads "Kill process"; a record hosted by another terminal
(the conductor roster's immediate parent, passed to the menu as `host`, and
not yet ended) names that terminal, "Kill terminal <name>"; anything else reads "Kill…" and defers
to aoide's own hosting-terminal resolution on click.

## Iteration Log

- 2026-07-26: initial commit as the replay fixture; palette-only, no cover.
- 2026-07-29: re-keyed from `song/covers/sonata.webp` (the dusk pianist). The
  earlier indigo-nocturne intent and the interim warm-parchment (Alma-Tadema
  *Unconscious Rivals*) key are both retired; the light key now reads from the
  cover it actually ships with, region by region, contrast computed above.
- 2026-07-29: cover swapped to `song/covers/yuki-sonata.png` — the same
  artwork as `sonata.webp` (identical scene and dimensions, PNG master), so
  the region-keyed palette carries over unchanged.
- 2026-07-29: bar sheet and popout glass lowered 0.62/0.60 → 0.45/0.45
  (khoa: a more transparent strip; the hyprglass blur carries legibility).
  Current surface elements recorded above; rice design memory now lives
  per-song (this file). [2026-08-14: the house grammar this once pointed at
  (the default rice's `design/pantheon.md`) is retired along with `default`
  itself — historical reference now at
  `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`.]
- 2026-07-29: **RE-KEYED to the Greek register** (khoa-approved). The light
  dusk key drawn from `yuki-sonata.png` (pale peach-cream ground `#f4e9e2`,
  plum-ink `#3b2f3a`, dusk slate-blue accent `#5a6f9c`, crimson-ember urgent
  `#b34a52`, horizon-green hot `#5f8a7a`) is **RETIRED** — kept here in the log,
  not deleted. The new key is a Greek marble register: marble ground `#f2ebde`,
  plum-charcoal ink `#2f2a33`, aegean accent `#345f81`, terracotta urgent
  `#b0472f`, Attic-gold highlight `#b98a2e`, laurel-green hot `#6b8b33`; all
  16 base16 slots re-derived region-by-region above, contrast recomputed and
  clearing every bar. Sonata also gains its OWN house grammar,
  `design/greek-grammar.md` — a typographic-Greek design language (columns,
  meanders, pediments; text-and-symbols only) diverging from the default song's
  Pantheon wireframe grammar, the blueprint for the widget reskin. The cover
  still points at `yuki-sonata.png` (covers/ is out of this re-key's scope); a
  Greek cover is a follow-up. The musical state-glyph contract (`♪ 𝄐 𝄽 𝄂 ·`,
  from `pkgs/aoide/src/conductor/theme.rs`) is untouched — the Greek forms frame it,
  never replace it. [2026-08-14: greek-grammar.md's motif vocabulary (columns,
  meander, entablature/pediment/stylobate) and its §4 widget-by-widget
  blueprint were cut outright in a retcon pass — khoa's call, not a further
  drift. The grammar's surviving scope is the order-marks (§1), the
  state-glyph contract (§2), and the role→hue palette (§3); each songbook
  widget's own file header is its design record now, not a shared blueprint.]
- 2026-07-29: Fable review GO-WITH-TWEAKS. Applied at palette level: (a) the
  one-hot laurel `hot`/`base0B` brightened `#5f7a37` → `#6b8b33` so the green
  blaze out-reads the teal verdigris structural role on warm marble by hue, not
  opacity alone (new ratio 3.30:1 on `#f2ebde`, clears ≥ 3:1 for a 2px border +
  halo); (b) Tyrian `base0E` `#7d4a6b` → `#6f4373`, toward true murex purple (it
  read mauve-rosewood); (c) `greek-grammar.md` §5 gains the verdigris-cap
  **invariant** — structural `wireCyan`/verdigris chrome renders at ≤ 0.5 alpha
  at all times, so the structural role can never steal the one-hot blaze.
- 2026-07-30: **gold-primary re-key + marble default** (Fable-advised, exact
  values). Chrome **accent → deep Attic gold `#a07414`** (3.54:1), replacing the
  aegean blue `#345f81` as `palette.accent`; **`base0A` unified to `#a07414`**
  too (the old `#b98a2e` gold under-contrasted at 2.63:1 — retired). **hot /
  `base0B` → true laurel leaf-green `#4e8b45`** (3.47:1), replacing the interim
  olive `#6b8b33` (only 41° off the new gold; leaf-green is 71° off). **Aegean
  `base0D` keeps its value and `holoBlue` role but its JOB narrows to
  preview/information** (workspace preview ring, links) — no longer active-state
  chrome. **Cover retired: `wallpaper` → null**, so the stylix facet bakes a
  deterministic bright-marble solid from `palette.bg` `#f2ebde` (the default
  song's mechanism); the `covers/yuki-sonata.png` reference is dropped (khoa:
  colours fit the theme, not a photo; the wallpaper switcher handles photos
  live). Facet-tuning memory recorded above: bar sheet glass 0.45 → 0.30, bar
  popouts → 0.72, unfocused kitty 0.90 → 0.80 (these live in the compositor/
  quickshell facets — other workers' domain — noted here only as design memory,
  not edited by this song).
- 2026-07-30: **bar opaque + window borders swapped** (khoa: theme consistency
  pass). `AoideBar.qml`'s manuscript-strip sheet is no longer glass: fill is now
  `paletteBg` at alpha 1.0 (was 0.30) and the Aero-style white gloss gradient
  overlay on the sheet is REMOVED entirely — the bar reads as a flat opaque
  strip, no blur-through, no sheen. Everything else (36px `stripHeight`, staff
  lines, barlines, playhead ink, clef/rests/note-heads, workspaces, clock,
  interaction) is unchanged. Window border notes swapped: `window.border`
  (active) is now Attic **gold** `#a07414` (= `palette.accent`, was the
  bronze-verdigris `#3f867e`); `window.borderInactive` is now bronze-verdigris
  **teal** `#3f867e` (was marble-shade `#e8dfcc`) — the focused window now
  matches the bar's gold chrome, the unfocused window recedes to a cool teal
  instead of near-vanishing into the marble ground. Edited in both
  `livery.json` and `rice.nix` (kept in sync). The compositor facet
  (`modules/facets/compositor/default.nix:42-44,72-73`) already derives
  `col.active_border`/`col.inactive_border` from `aoide.livery.window.border`/
  `.borderInactive` (falling back to `palette.accent`/`palette.bg` when a song
  leaves them null) — no compositor-side change was needed, the pipeline just
  picked up the new song-owned notes.
- 2026-08-15: **`design/` retconned around the belonging problem.** Three
  separate design passes over ONE bar popout were rejected on sight the same
  day (an invented votive-tablet metaphor with a 30-line defence; a stripped
  bare-icon `Flow`; an icon+name row list), and the popout body was torn out
  of `bar.qml` rather than kept. Diagnosis: the docs transmitted RULES
  (`notes.*`-only, `radius: 0`, banner style) but not what makes a widget read
  as family. New `design/making-a-widget.md` states the finding — belonging is
  carried by a shared eleven-part build order at fixed sizes, not by a
  metaphor; the metaphor only renames the parts — with the colour-spend
  census, the two named failure modes, worked walkthroughs of
  `notifications.qml`/`powermenu.qml`/`calendar.qml`/`AudioColonnade.qml`, a
  bench check, and the on-screen verification loop. New `design/hazards.md`
  collects the live-confirmed traps (the invisible-fermata `font.bold` scar,
  the two further non-ASCII glyph failures, the Layouts sizing loop, the
  popup-resize remap, the dynamic-loading type-resolution limit, the
  notification close-path rules). `greek-grammar.md` retconned from
  house-grammar prose into the concrete vocabulary (faces, glyph sets, type
  tiers, opacity ladder, rule idioms, state treatments), keeping the
  order-marks, the state-glyph contract and the role palette; the doc-history
  paragraphs about what earlier passes cut were dropped, and the verdigris
  ≤0.5 structural cap is now stated once, plainly, as the live invariant it
  is. `widget-structure.md` refocused on mechanism + hard contracts and gained
  a free-vs-must-supply table for the five host anchors and a motion-tier
  table. No `.qml` touched.
- 2026-09-07: **Codex terminal identity motion prepared.** Conductor and
  Terminals share a 24×18 illuminated book beside a main Codex agent's name.
  A leaf turns while working (1.1s turn, 450ms rest), with a faint gold edge
  following the lifted page. Awaiting/sudo holds the book open and still;
  idle/stopped/done closes its cover. The same box stays reserved in every
  state. The owner's book direction replaced the initial terminal-prompt mark
  and the intermediate constellation. Motion follows the existing live state,
  and stops/resets when its box is hidden, clipped away, or outside the owning
  window. A geometry binding intersects ancestor clips and the window bounds,
  observing positions, sizes and the dock's Translate so folder collapse,
  scrolling and the shut dock pause the loop without a polling timer. Transform
  discovery waits for component completion: dynamic creation fires parent
  changes before the outer root id is available.
  Agent identity and kind come from the session
  record; Terminals preserves the published kind in its render projection so
  a shell or subagent cannot inherit the badge. There is no reduced-motion
  preference in the current widget seam. The book and gilding use native geometry
  rather than a small-glyph font fallback; independent review and live
  observation remain the acceptance gates. Terminals' four rounded box-drawing
  corners were squared to `┌ ┐ └ ┘` at the owner's request, matching the other
  temples' frame vocabulary while retaining the Ionic capital and frieze.
- 2026-09-07: **Codex book and edged Terminals frame verified live.**
  Independent review passed dynamic creation/recreation, the six live states,
  and 23 visibility/exposure cases. The staged dock shows both books and the
  square frame; consecutive compositor captures show the lifted page moving.
  Runtime widget sources were older committed copies than the rendered song,
  so they were backed up and reconciled to the live baseline before staging.
  Only Conductor and Terminals changed in the rendered tree; the registry,
  livery, wallpaper and other widget bodies were preserved. The first reload's
  internal sync successfully reloaded the shell, but its redundant second IPC
  request raced loading and reported failure inside an overall `ok` outcome.
  A settled repeat succeeded and deduped the take. Learnings: staging needs
  source-drift detection and a pre-sync recovery snapshot; reload needs one
  acknowledged request and an outcome that reflects its result.
- 2026-09-07: **Codex's page loop carries its text through the turn.**
  Both faces of the moving leaf use the settled pages' outline and muted
  printed strokes. A shared gutter is drawn once, and the moving overlay's
  weight tends smoothly to zero at lift-off and landing, avoiding doubled
  translucent outlines. The landed page and loop reset show the same printed
  spread; repeated leaves keep the same decorative text so no pattern resets.
  At native 24×18 output, captured phases `0`, `.001`, `.999`, `1`, and the
  reset to `0` are pixel-identical in both widgets (0/432 changed pixels).
  Front/back mid-turn captures retain text and differ from those endpoints;
  dynamic widget creation/recreation and the existing state poses still pass.
  Independent review passed, and the staged dock was inspected across repeated
  turns. Only the two reviewed widget bodies changed; the settled reload
  succeeded and deduped the take.
- 2026-09-07: **The Codex book belongs to Conductor alone.** At the owner's
  request, Terminals removes the book, its motion/exposure helpers and its
  reserved nameplate space. The common terminal state lamp and kaomoji remain,
  as do the square `┌ ┐ └ ┘` frame corners. Conductor's book is unchanged.
  Independent review and the live dock confirm the book appears only in
  Conductor; Terminals keeps its name, state and square frame.

- 2026-09-10: Session nameplates retain harness and title beside petname/ID suffix; expandable, copyable recovery details include child rows. No routing identity changed.

- 2026-09-10: Main headings use session titles, with harness/model and recovery identity on separate lines; Conductor animations move left of the title. Prompt content stays independent.

- 2026-09-10: Terminals recovery controls share the petname line on hover; closed rows recover the dedicated action-row height.

- 2026-09-10: Conductor combines harness/model and recovery identity into one secondary line, reveals controls on hover, and omits duplicate harness-only labels.

- 2026-09-10: Session cards gain shared right-click actions, multi-directory project editing, and visible bridge outcomes.
