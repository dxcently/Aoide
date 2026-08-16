# Hazards — what has actually failed on this rig

**Song:** sonata

Each entry below was found the hard way on yomi-strix — most of them with a
screenshot or a journalctl line — and is recorded in the comments of the file
that hit it. None of it is theoretical; none of it is a general QML tutorial.
Read §1 before you type a glyph you have not already seen on this desktop.

---

## 1. Fonts and glyphs

**The rule, sharpened twice: ANY non-ASCII glyph needs live verification on
this font stack — at the exact face AND weight you use it.** Not just rare
Supplementary-Plane symbols. Three separate failures established this:

| glyph | where | what happened |
|---|---|---|
| `𝄐` / `𝄑` U+1D110/U+1D111 (fermata pair) | `bar.qml` tray toggle, `font.family: "monospace"` + `font.bold: true` | rendered **fully invisible**. Root-caused with pixel-diff screenshots, not guessed: forcing `visible: true` unconditionally changed zero pixels; removing `font.bold` made the glyph appear immediately. The bold face on this stack has no glyph for the pair and Quickshell drew nothing at all |
| `𝄂` U+1D102 (final barline) | `bar.qml` rice-mode cell, `font.family: "monospace"` | rendered as a bare `\|` fallback |
| `‖` U+2016 (double vertical line) | same cell | rendered as a stray `/` — **and this is common General Punctuation, not a rare SMP symbol**, which is what sharpened the lesson past "rare glyphs are risky" |

The rice-mode cell settled on plain ASCII `"||"`, which "sidesteps the whole
fallback chain".

**A correlation worth knowing, with the causation NOT established:** all three
recorded failures are in `bar.qml`, which declares `font.family: "monospace"`
(the generic fontconfig alias) on every cell. The same `𝄂` code point is
declared with `"Noto Music"` in `notifications.qml`, `calendar.qml` and
`powermenu.qml`'s heading, and with `"JetBrainsMono Nerd Font"` in
`powermenu.qml`'s bottom frame, and none of those has been reported failing.
Operationally: **declare one of the three named faces
(`greek-grammar.md` §1), and verify anyway.**

Other glyph findings:

- **`font.bold` is a rendering hazard, not just a weight.** Where a glyph is
  rare, prefer regular weight and get contrast from size or colour.
- **Moon-phase emoji** are absent from all three declared faces (`fc-list`
  verified). Bare code points fall back to Noto Color Emoji, which is
  fixed-colour and deaf to `color:`; appending `U+FE0E` resolves them to
  DejaVu Sans monochrome, which recolours from `notes.*` and stays legible at
  9px. `calendar.qml` runs the colour version by explicit owner override and
  documents the monochrome set as its rollback.
- **Kaomoji charset.** Stay inside the kana and punctuation vocabulary
  `MoodFaces.qml` already proves safe (`ω ｏ ノ ｀ ´ ˘ ･`). An earlier pick
  used Thai and Hangul and rendered as tofu-adjacent junk live.
- **Vertical guillemets.** The true CJK forms `︿ ﹀ ︽ ︾` arrive full-width
  and sans-weight via a Noto Sans CJK fallback, overwhelming a serif line;
  `˄ ˅` are modifier letters, superscript-floating and too small to hit.
  `calendar.qml` rotated its existing `‹ › « »` 90° instead — **prefer
  rotating a proven glyph over adopting an unproven one.**
- **Tall glyphs clip a short surface.** The treble clef at 20px clipped the
  36px bar strip; the fix is `font.pixelSize: 16` +
  `fontSizeMode: Text.VerticalFit` + `height: root.stripHeight`, not an apron
  (an apron reopens the wallpaper-bleed scar).

---

## 2. Layout and sizing

- **`QtQuick.Layouts` deadlocks this shell.** A v1 notification card hit a
  sizing-negotiation loop — `RangeError: Maximum call stack size exceeded`,
  live in journalctl — the moment wrapped body text rendered: nested `Layout`
  items under an ancestor whose `implicitHeight` derives from the layout's own
  `implicitHeight`. No widget uses Layouts; use `Column`/`Row`/`Item` with
  explicit widths and anchors.
- **Binding loops from self-referential sizing.** `calendar.qml`'s memento row
  hardcodes `width: 3 * 124 + 2 * 8` with the comment "not parent.width
  (binding loop: the Column sizes from us)".
- **A `ListView` footer counts toward `contentHeight`**, so anything sized off
  `contentHeight` loops. Sparse-state fillers are declared as SIBLINGS of the
  `ListView`, positioned at `lv.contentHeight + 10`.
- **A rotated `Text`'s layout box does not rotate with it.** Rotated glyphs
  ride fixed-size wrapper `Item`s.
- **Decoration that tracks an element must reuse the element's exact offset
  expression.** `bar.qml`'s active-workspace highlight was anchored to plain
  `verticalCenter` while every note glyph is displaced by `pitchOffset(ws)`,
  so the circle only lined up for whichever id resolved the offset to 0 —
  confirmed live as a mark sitting a full staff-space above its glyph. The fix
  is the same expression on both, not a fudge constant.
- **`WidgetSlot` sizes itself off the loaded child**, which is backwards for a
  full-width surface. `bar.qml` binds `width: parent ? parent.width :
  implicitWidth` — under `Component.createObject(root, props)` its `parent` IS
  the `WidgetSlot`, which the facet anchors to fill the window.
- **Toggling `visible` inside a centred layout makes the layout jump.**
  `powermenu.qml`'s laurel `♪` tick is anchored OUTSIDE the centring and
  cross-fades on `opacity` "so the name never jumps when the laurel arrives".

---

## 3. QML engine and dynamic loading

- **Same-directory custom types do not resolve reliably** under Quickshell's
  dynamic `Component.createObject(url)` loading — the mechanism both slot
  anchors use. Confirmed live: the implicit same-directory rule failed with
  "WorkspaceRow is not a type", and a top-level file-scoped `component` failed
  with "Syntax error". Nest helper components inside the root item.
- **`required property` is resolved at OBJECT CREATION.** A declarative
  `Loader` can only assign properties in `onLoaded`, which is too late —
  creation throws first, so the item never becomes non-null and `onLoaded`
  never fires. This is why `WidgetSlot`/`SurfaceSlot` use
  `Component.createObject(parent, initialProperties)` and manage lifecycle by
  hand.
- **`QtObject` has no default property.** Children must be declared as named
  properties: `property FileView noteFile: FileView { … }` in `LiveryState`,
  `property Connections _manifestWatch: Connections { … }` in `SurfaceSlot`.
- **Transitive reactivity is not guaranteed for a non-visual root.**
  `SurfaceSlot` found that its `resolvedSong`/`resolvedSource` chain did not
  reliably re-trigger on a late `stagingEngine.manifest` load the way
  `WidgetSlot`'s Item-rooted version does, and forces the dependency with an
  explicit `Connections` rather than depending on it.
- **A property that is both bound and written breaks its binding on first
  write.** `launcher.qml`'s `TextInput` binds `text: root.query` and also does
  `onTextChanged: root.query = text`; resetting `root.query` directly would
  leave stale text on screen, so the reset goes through `searchInput.text`.
- **Stop in-flight animations before a hard state reset**, or the running
  `NumberAnimation` keeps writing the property on later frames and fights the
  assignment (`flipOut.stop(); flipIn.stop()` before `turn.angle = 0`).
- **A parse error renders NOTHING and does not crash the shell.**
  `WidgetSlot` logs `[aoide/widgetslot] failed to load slot <name> - <error>`
  and moves on. An empty surface is a journalctl question first.
- **`destroy()`ing a loaded widget nulls its `root` id and re-fires every
  binding that captured it.** `launcher.qml` was dumping ~66-79
  `TypeError: Cannot read property 'notes' of null` lines per hot reload
  (1984 in six hours) — all of them its own 65 `root.notes.*` bindings
  re-evaluating at once, in one burst, on the log line right after the
  window's `Component.onDestruction`. **Read that message precisely: the
  null is `root`, not `root.notes`.** In `root.withA(root.notes.x, a)` QV4
  evaluates the call ARGUMENTS before it looks up the callee, so a null
  `root` throws on `.notes` and never mentions `.withA` — the reported name
  is the argument, not the receiver.
  The cause was upstream: `SurfaceSlot` re-entered `_rebuild()` **four times
  per reload** (`extraProps` → `onCompleted` → `resolvedSource` →
  `manifestChanged`), and the fourth found the surface already built,
  destroyed it, and built an identical one. The fix is idempotence at the
  anchor (`_builtSource`/`_sameExtras`, rebuild only on a real change), not
  null guards in the widget. `powermenu.qml` had the same double-build and
  stayed silent only because it is fully static — no Repeater/ListView
  delegates on the way down — so "it doesn't warn" never proved "it doesn't
  churn".
- **Compare `extraProps` by VALUE, never by identity.** `shell.qml` declares
  it as an object literal (`({ clipboard: clipboard, ledger: ledger })`), so
  every re-evaluation hands over a new JS object wrapping the same
  instances; an identity check calls that a change and rebuilds forever.
- **A slot anchor's teardown is loud; the engine's is quiet.** The previous
  generation's widget is destroyed on every reload too and never warns —
  Quickshell drops that whole generation at once. Only an explicit
  `item.destroy()` against a LIVE engine re-evaluates bindings. That, not
  `PanelWindow`-vs-`Item` and not delegate count, is why some widgets warn
  and others don't.

---

## 4. Quickshell services and popups

**Notifications** (`notifications.qml`, `AoideNotifications.qml`, grounded
against the compiled `quickshell-service-notifications.qmltypes`):

- The server exposes `trackedNotifications`, **not** `.notifications`, and a
  notification only enters that model once something sets `tracked = true`.
- **Never write `tracked = false` from a card, on any path.** `close(reason)`
  sets the close-reason before `deleteNotification` runs, which already
  removes it from the model. On the sender-initiated `CloseNotification` D-Bus
  path, `deleteNotification` is called directly WITHOUT `close()`, so a
  `tracked = false` write re-enters `setTracked(false)` → `close(Dismissed)` →
  a second, nested delete, emitting a second `NotificationClosed` signal with
  the WRONG reason before the correct one goes out — protocol-visible
  misbehaviour to any spec-compliant client.
- **`NotificationAction.invoke()` already closes** the notification for any
  non-resident sender. Calling `dismiss()` as well logged "Cannot close
  destroyed notification" on every action click.
- **Bind a `Repeater` to the `ObjectModel` directly, never to a
  `.values.slice()` copy.** A plain JS array has no identity to diff against,
  so every arrival or departure of ANY notification tears down and recreates
  EVERY delegate — restarting every other card's auto-dismiss `Timer` and
  urgency pulse.
- Spec senders (kitty always, for OSC-9/99 forwarding) attach an implicit
  `"default"` action. Rendering it produces an empty outlined button; filter
  on `modelData.identifier === "default"`.

**System tray:** `SystemTray.onItemRegistered`/`onItemUnregistered` are plain
METHODS in the qmltypes, **not signals** — a `Connections` handler on them
compiles and silently never fires. Watch the model instead:
`Connections { target: SystemTray.items; function onObjectInsertedPost(...)`.
(A plain `.values.length` binding was separately console-probed on the running
desktop and DOES stay reactive; the hand-maintained count in `bar.qml` is the
more robust idiom, not a fix for a broken binding.)

**Popup windows:**

- **Anything drawn below the bar inside the bar `Item` is clipped by the layer
  surface and never renders** (the v1 popouts' silent failure). A popout must
  be a real `PopupWindow` — its own surface, free to extend past the strip.
- **Hyprland re-maps an `xdg_popup` on the first resize of a visible popup**
  and plays its popup animation over the remap (~0.25s vanish/fade, measured,
  anchor-mode-independent). `calendar.qml` sequences its window resize around
  the morph so the flicker lands on a static image.
- **A centred `xdg_popup` is re-centred by the compositor on every width
  change, one frame behind the resize** — a visible twitch mid-morph
  (live-verified). Pin an edge (`anchorEdges`/`anchorGravity`) for anything
  that resizes while open.
- **An `xdg_popup` carries no namespace of its own**, so it cannot opt out of
  the bar layer's `blur_popups` layerrule. The only escape is its own layer
  surface (`SteleLayerPopout`).
- **Qt's `Window.visible` attached property never flips under Quickshell's
  proxy windows** (verified). The real open/close edge is
  `QsWindow.window.visible`.
- **`grabToImage` can fail** — it returns false; degrade to the instant path
  rather than leaving the widget mid-transition.
- A popup that can be opened with nothing to show should gate on the count
  and force itself closed when the count hits zero ("no stuck-open empty
  popout").

**Files:** `FileView` with `watchChanges: true` + `onFileChanged: reload()`;
parse inside `try/catch` and HOLD the last good value on garbage rather than
resetting to zero.

---

## 5. Data and command seams

- **QML never shells out.** Reading kernel state means reading a file
  (`bar.qml` parses `/proc/net/route` through a `FileView` — the
  files-not-processes rule), and side effects go through `bridge.sendCommand`
  or a Quickshell-native service call (`DesktopEntry.execute()`,
  `Hyprland` workspace `activate()`).
- **`ShellBridge` is OUTBOUND-only, and it only understands verbs the daemon
  parses.** `{ cmd: "powermenu" }` went nowhere — shellbridge's parser never
  knew that verb — so the bar's clef calls `.toggle()` on the injected
  powermenu instance directly instead. Inbound triggers are Hyprland
  `GlobalShortcut`s registered in-process (`aoide:launcher`,
  `aoide:powermenu`, `aoide:clipboard`); no new socket, no new CLI verb.
- **Pipewire's `.audio` sub-object never binds unless the node is tracked.**
  `PwObjectTracker` must carry BOTH `defaultAudioSink` and
  `defaultAudioSource`.

---

## 6. Verification traps

- **`aoide rice stage` refuses while the rice mode is `declarative`** (the
  default) with `reason: "declarative-mode-locked"`. Run
  `aoide rice mode stage [<song>]` first; `aoide rice mode declarative` locks
  it back.
- **A re-stage that changes no widget body deliberately skips the Quickshell
  reload** ("no widget bodies changed; nothing to reload"). If you need a
  reload regardless, run `aoide quickshell reload`.
- **A brand-new slot file is not discovered by a reload** — `manifest.json` is
  read once, at startup. Restart `aoide-quickshell.service`.
- **The CLI verb is `quickshell reload`, not `shell reload`** — renamed
  because `shell` collided with `--agent shell`.
- **Quickshell's file watcher only scans the TOP LEVEL of `run/qml/`, never
  `run/qml/songs/`.** So editing a song widget can never trigger a reload on
  its own — `aoide rice stage <song>`'s IPC call is the *only* thing that
  makes the edit render. There is no watcher fallback behind it.
- **`quickshell ipc call` exits 0 even when the call never reached a
  handler**, printing `Not ready to accept queries yet.` to **stdout** (not
  stderr) for an unknown target or function. Measured 2026-08-15: real call
  → exit 0, silent; unknown fn/target → exit 0, that message; bad `-p` path →
  exit 255. `aoide quickshell reload` now judges success on *silence*, since
  `reload()` is declared `: void` and a void call that lands prints nothing —
  but if you invoke `quickshell ipc call` by hand, **the exit code will lie to
  you**. Read its output.
- **`journalctl --user -u aoide-quickshell.service` can fall far behind** —
  observed ~35 minutes stale mid-session before catching up. A clean journal
  is not proof of a clean reload. `quickshell log --pid <pid> -t N` is the
  reliable channel and survives restarts.
- **Do not declare a visual change done without looking at it.** `grim` the
  screen, crop with `magick`, read the PNG back, judge it yourself first.
