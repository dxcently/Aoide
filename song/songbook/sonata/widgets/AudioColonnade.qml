import QtQuick
import "../.."

// ── THE COLONNADE · a two-column temple porch (audio in + out) ────────────────
// The bar's audio control: BOTH channels of the machine's sound, drawn as a pair
// of literal fluted classical columns sharing one architrave and one stylobate
// — a temple FRONT, not two gauges. The INPUT (source / microphone) is the left
// column, the OUTPUT (sink / speakers) the right. Each column IS its meter: the
// shaft fills bottom-to-top with its level, capital + fluting + neck intact.
// Click a column to mute it (it falls to RUIN — see below), scroll to set level.
//
// khoa, 2026-07-31 — this is now a SELF-FRAMED MARBLE STELE, the same grammar
// ConductorGadget/TerminalsGadget/MetersGadget/PowerVitalsGadget/NotificationCard
// wear (greek-grammar.md §4; NotificationCard.qml documents the shared shell):
// opaque marble body, 2px paletteFg border, 1px inset signature keyline, a cast
// shadow, a carved-serif crown + name, a Canvas frieze, a box-drawing TUI frame
// top AND bottom (closing 𝄂 barline in gold), radius 0, colour only from livery.
// It no longer wears GadgetFrame — it is hosted bare (StelePopout) and draws all
// its own chrome, converging the outer panel on the rest of the pantheon family
// while keeping its OWN distinct order/hue/crown/motif:
//
//   · ORDER   — a DISTYLE PORCH of TWO orders, one per channel, so the columns are
//               told apart by SHAPE as well as hue:
//                 – MIC → IONIC, aegean (`holoBlue`): a volute (scroll) capital,
//                   echoing the aegean Ionic Terminals temple. The cool input.
//                 – VOL → DORIC, Attic gold (`paletteAccent`): a plain echinus
//                   capital, echoing the gold Doric Conductor. The warm output.
//   · SIGNATURE (chrome hue) — MUREX `livery.violet` (base0E) as of 2026-08-16,
//               by direction ("make the audio widget main color diff from the
//               calendar widget"); it was base09 amber, which the calendar also
//               wears. The reasoning that picked amber still stands and is what
//               narrowed the replacement: the chrome must not read as any of
//               this widget's own CHANNELS, so gold (output), aegean (input) and
//               teal (bt) were disqualified, laurel is the one-hot blaze, and
//               base0F rust is both notifications' and — measured — the same
//               orange-brown family as the calendar's clay (1.49:1), so it
//               would not have answered the ask. Murex is the only remaining
//               slot, and it is an upgrade rather than a survivor: 6.50:1
//               against this marble where amber managed 3.31:1, which lifts
//               the frame text and `[ pipewire ]` above the 4.5:1 AA bar they
//               were under; and amber sat 1.07:1 from the gold Doric fill it
//               was drawn beside, near-isoluminant in the same warm family.
//               Tyrian murex is the dedication dye — the temple's ribbon, not
//               any channel's. base0E is also PowerVitals' dock signature; the
//               share is cross-FAMILY and stands, flagged, because a signature
//               tells apart steles of the same family and the bar-popout family
//               is now unique (calendar clay, audio murex, notifications rust).
//               Two reads were checked on screen because their luminance
//               ratios are close and only hue separates them: the terracotta
//               hushed row (1.39:1) and the aegean recording gauge (1.14:1) —
//               both read cleanly against murex chrome (captured). The frieze
//               dropped 0.85 → 0.7 after looking: at the old alpha the bead
//               course out-read its own crown.
//   · CROWN   — ♫ a beamed pair of livery (two channels) in place of a clef, the
//               way Power's ϟ / Notification's ❧ stand in where no clef fits.
//   · FRIEZE  — a bead-and-reel (astragal) course: round beads spaced by reels,
//               a classical moulding none of the other temples use.
//   · MUTED = RUIN — a muted channel is a BROKEN COLUMN: the capital is gone and
//               the shaft is SNAPPED partway up along a jagged terracotta break,
//               with fallen rubble at its foot — a ruined-temple column, legible
//               as "silenced" from the silhouette alone. No slash, no ghost fill.
//
// khoa, 2026-08-15 — a THIRD BAY: BLUETOOTH. The porch goes distyle → TRISTYLE
// (width 268 → 320; colW 70 → 72, bayGap 30 → 26, so 3*72 + 2*26 = 268 still
// leaves a 15px inset the stylobate's widest step clears). The bar's audio
// cells stopped hover-opening this stele on the same date — it is now
// CLICK-latched, and hovering gets a separate small cue readout in bar.qml —
// so `hovered` is kept but no longer load-bearing for that host.
//
//   · ORDER   — the third channel takes the third classical order, CORINTHIAN
//               (concave abacus, flaring kalathos bell, two tiers of acanthus
//               tips, corner helices). Doric cushion / Ionic volutes /
//               Corinthian leaves: the three bays are told apart by capital
//               alone, colour removed — the same test the first two passed.
//   · PIER    — its shaft is WIDER than its neighbours (`pierW` 34 vs
//               `shaftW` 24) because it carries an inscription per drum. A
//               shaft is built of stacked drums; here the stack is exactly
//               TWO, jointed, and which drum is LIT is the mode. One box, one
//               joint — not two boxes (khoa's sketch is a single divided box).
//   · HUE     — `wireCyan` (base0C teal), NOT `holoBlue`: aegean is already
//               the MIC bay's role in this same stele, and a role does one job
//               (making-a-widget.md §2). The gold VOL bay sits between the two
//               cool bays, so they never abut. Its two reads — the lit drum
//               and the device name in the tally — serve ONE state, which is
//               the house's rule for spending a second hue.
//   · STATES  — power is the base state, profile the refinement:
//                 no adapter        → `avail` false: ghosted pier, tally "—"
//                 adapter powered off → RUIN, the same broken-column
//                                     silhouette a muted channel wears, tally
//                                     "OFF" in terracotta. Powered-down and
//                                     silenced are the same idea, so they get
//                                     the same shape.
//                 on, nothing bound → intact pier, both drums carved but
//                                     inert (ink 0.32), tally "ON"
//                 bound in A2DP     → upper drum fills teal, inscription
//                                     reversed out in ground colour
//                 bound in HSP      → lower drum fills, tally carries the
//                                     device's own name instead of "ON"
//               The tally never repeats what the drums say: drums = which
//               profile, tally = who (or, with nobody bound, the power word).
//   · MIXER   — `[ mixer ]` in the ledger, the house's own bracket-token
//               vocabulary rather than a button (this stele has no button
//               anywhere else and does not grow one). The host runs
//               pavucontrol; this file only signals.
//
// khoa, 2026-08-16 — EVERY BAY GETS ITS ROSTER. "Which channel is this, and
// let me change it" was the one question the porch could not answer: it drew
// how loud the output was and never which output. Two parts, and no new
// chrome family for either:
//
//   · THE PLINTH — a course of three drawn chips under the tallies, one per
//     bay, each naming its bay's current channel. It is the calendar's
//     `[ ἔτος ⌄ ]` control verbatim (hairline border, radius 0, a faint
//     signature fill on hover, a wedge pointing where the material will go),
//     tripled and set on the stylobate's own line, because that file already
//     settled that a CONTROL in this house has a pressable edge while a tag
//     is only ever letters. Read at rest, it answers the question without
//     being opened, which was half the ask.
//   · THE PICKER — pressing a chip swaps the porch's middle for that bay's
//     list. It does not drop DOWN out of the pillar: three 84px popups hung
//     under three bays would overlap each other, escape the stele, or be too
//     narrow to hold a device name, and this file has no floating-panel
//     vocabulary to borrow for them. Instead the bay OPENS — the columns fade
//     out and the list takes exactly the rect they occupied, captioned by the
//     stele's existing top frame (`♪ levels` becomes `♪ vol · sinks`). Three
//     structural consequences fall out for free: only one bay can be open
//     (`openBay` is a single int), the list can never overlap another bay
//     because it replaces all three, and it can never escape the stele
//     because it is anchored inside the porch. The architrave stays lit above
//     it with the open bay's inscription at full ink and the other two
//     stepped back, so the list never loses which pillar it came from.
//     Rows are `launcher.qml`'s manuscript row — margin mark, serif name,
//     mono gloss, hairline rule — with the CURRENT entry marked by that bay's
//     own hue, the same "filled in the bay's hue means this one is live"
//     language the lit drum and the rising fill already speak.
//
// The porch widened 320 → 352 (colW 72 → 84) for exactly one reason: a chip
// 72px wide elides a real sink name to about "SN6186 An…", and a control that
// cannot show its own answer is not worth the line it costs.
//
// The write is NATIVE, and that is a correction to what this file assumed in
// August: `quickshell-service-pipewire.qmltypes` marks `defaultAudioSink` and
// `defaultAudioSource` `isReadonly: true`, but declares alongside them
// `preferredDefaultAudioSink` and `preferredDefaultAudioSource` carrying
// `write: "setDefaultConfiguredAudioSink"` / `"setDefaultConfiguredAudioSource"`.
// So switching the default output or input needs no `pactl set-default-sink`
// and no shell-out at all. Only the bluez A2DP↔HSP profile still leaves QML,
// for the reason recorded above — that one has no native setter anywhere in
// either module. This file holds neither seam; it emits the intent and the
// bar performs it.
//
// NOT VERIFIED HERE: the connected / A2DP / HSP / device-list renders are
// driven from fabricated props through AudioColonnadePreview, never from the
// live service. yomi-strix now HAS a powered bluez adapter, but the running
// Quickshell was started before bluetooth came up and its `Bluetooth`
// singleton reports no default adapter, so the bar still feeds this file the
// no-adapter register and only that register has been seen coming off real
// bluez. The sink and source rosters ARE live.
//
// khoa, 2026-08-16 — THE PARTHENON. "A separate channel mixer you can open —
// expand the audio widget like the calendar into a bigger parthenon with all
// mixing channels and audio options", then: "additional tabs for playback,
// recording, output devices and input devices" (pavucontrol's four, plus a
// fifth for bt — see REGISTERS below). The morph itself is not this file's:
// it runs on the shared `MorphState`, the facet component the calendar's
// compact↔expanded scroll also runs on now.
//
//   · IT GROWS DOWN, AND THE PORCH STAYS. The expanded state is this same
//     stele's part 9 grown, not a second widget: one crown, one wordmark, one
//     [ pipewire ], one frieze, and the ten chrome parts above and below the
//     body untouched. The three pillars persist — the compact bay-swap existed
//     only because the porch's box was the sole room a closed stele had, and
//     the parthenon has a room, so the trade goes with the constraint that
//     forced it. Hiding the master meters during the one activity that moves
//     them would be backwards anyway. Width is fixed at 352 in both modes: a
//     centred xdg_popup is re-centred by the compositor on every WIDTH change,
//     and a height-only step never asks it to.
//   · REGISTERS — five drawn chips on a course under the plinth, the
//     calendar's `[ ἔτος ⌄ ]` control a second time. All five read at rest;
//     the stele's existing top frame renames itself to echo the sounding one
//     (`♪ levels` → `♪ playback`), which is the band that already renamed for
//     an open bay. A caption ALONE was rejected: it shows the current register
//     and hides the other four, and identity behind interaction is the failure
//     this desktop has already paid for once. 5 × 56 + 4 × 6 = 304, the porch's
//     own measure to the pixel.
//   · TWO ROW SPECIES, and no third. Streams have a level and a mute, so
//     playback/recording wear the cue's row (mark · name · gloss · gauge ·
//     figure; click silences, wheel adjusts). Devices are a CHOICE, so
//     outputs/inputs/bt wear the picker's row (mark · name · gloss · rule;
//     click selects, the current one marked in the channel's hue) — the very
//     same `DeviceRow` the compact bay picker now uses, so the two faces
//     cannot disagree about which channel is current. A row carrying both a
//     gauge and a selection mark would be a species with no precedent here.
//   · STREAMS ARE NEVER PILLARS. A pillar exists in this file only if its
//     capital tells it apart with the colour removed, and the classical
//     inventory holds five orders total; per-app streams are unbounded and
//     transient. They are occupants of the temple, not its architecture.
//   · AUDIO OPTIONS resolved to what was already here rather than a new
//     toolbar: the plinth chips (which channel each bay is on) persist into
//     the expanded face and now ROUTE to their register instead of fading the
//     porch, and `[ mixer ]` keeps its ledger [ token ] — the one door that
//     leaves the widget, and khoa asked for it explicitly. No gear, no
//     slider-with-thumb: a level is a fill and the wheel sets it, everywhere.
//   · THE LIVE FIGURES follow the mode. The naos control carries the channel
//     count (`3 channels ⌄`), so it is the control, the label and the figure at
//     once; and the closing frame stops repeating the tallies once the naos is
//     up, changing to the one number nothing else says — how many are hushed.
//
// House rules: every colour from `livery` roles (zero hex); radius 0; no
// QtQuick.Layouts (plain Item/Row/anchors — the documented sizing-loop hazard).
Item {
    id: root

    required property var livery

    // ── Live levels, fed from the bar's Pipewire seams ──────────────────────
    property int  outPct: 0
    property bool outMuted: false
    property bool outAvail: false
    property int  inPct: 0
    property bool inMuted: false
    property bool inAvail: false

    // ── The BLUETOOTH bay — power as the base state, profile once connected ─
    // btAvail  : an adapter exists at all (bluez up + hardware present)
    // btOn     : the adapter is powered
    // btConnected / btName : a device is connected, and who it is
    // btProfile: "a2dp" | "hsp" | "" (connected but profile not yet resolved)
    property bool btAvail: false
    property bool btOn: false
    property bool btConnected: false
    property string btName: ""
    property string btProfile: ""

    // ── Control intents, handled by the bar (which owns the sink/source) ────
    signal outToggle()
    signal outAdjust(int delta)
    signal inToggle()
    signal inAdjust(int delta)
    signal btToggle()                     // power the adapter on/off
    signal btPick(string profile)         // "a2dp" | "hsp"
    signal openMixer()                    // the [ mixer ] tag → pavucontrol

    // ── The rosters — one per bay, supplied by the bar ──────────────────────
    // Each entry is plain data, never a live service object:
    //   { key: string, name: string, gloss: string, current: bool }
    // `key` is opaque here — a PipeWire node id for the two audio bays, a
    // bluez address for the third — and travels back out untouched in the
    // pick signals. The bar owns every service handle; this file is handed
    // rows to draw and hands back which one was pressed, which is the same
    // seam the level controls already use.
    property var sinkRoster: []
    property var sourceRoster: []
    property var btRoster: []
    signal pickSink(string key)           // → Pipewire.preferredDefaultAudioSink
    signal pickSource(string key)         // → Pipewire.preferredDefaultAudioSource
    signal pickBt(string key)             // → BluetoothDevice connect/disconnect

    // ── THE CHANNELS — per-application PipeWire streams, the naos's payload ───
    // Same seam as the rosters: plain data in, an opaque key back out.
    //   { key: string, name: string, gloss: string, pct: int, muted: bool,
    //     out: bool }        out=true a playback stream, false a capture one
    // These are the nodes the three rosters deliberately EXCLUDE (`isStream`),
    // because a stream is an occupant, not a device — and, by the same
    // argument, not a pillar either (see the naos note in the header).
    property var streamRoster: []
    signal streamToggle(string key)       // click a row → mute that stream
    signal streamAdjust(string key, int delta)   // wheel a row → set its level

    // Which bay's roster is open: -1 none, 0 MIC, 1 VOL, 2 BT. A single int
    // is the whole no-overlap guarantee — two bays cannot be open at once
    // because there is nowhere to store that.
    property int openBay: -1
    // Popout-retention: true while the pointer is over any column. Kept for
    // hosts that hover-retain; the bar's colonnade is CLICK-latched as of
    // 2026-08-15 and no longer reads it.
    readonly property bool hovered: micCol.hovering || volCol.hovering || btCol.hovering

    // ── PORCH ↔ PARTHENON — the morph, on the shared MorphState ─────────────
    // The width NEVER changes: a centred xdg_popup is re-centred by the
    // compositor on every width change, one frame behind the resize
    // (hazards.md §4, re-captured live 2026-08-16 — the one transitional frame
    // per resize is a WIDTH artifact and is absent from a height-only step).
    // So the temple grows DOWNWARD: the porch stays exactly where it was and
    // the interior is revealed beneath it.
    //
    // THIS STELE DELIBERATELY WIRES NEITHER OF MorphState's STEP HOOKS, and
    // that is the opposite of what calendar.qml does with the same object.
    // Measured, both ways, on this popup (2026-08-16):
    //   · STEPPED (the calendar's discipline — surface jumps to the open size
    //     first, paint lerps inside it): the surface is larger than the stele
    //     for the whole 340ms, and on a FROSTED xdg_popup the uncovered area
    //     is not nothing — `blur_popups` frosts it, so a blank glass rectangle
    //     up to 148px tall hangs under the stele for the length of the morph,
    //     in both directions. Captured.
    //   · TRACKING (implicitHeight bound straight to the painted height):
    //     clean. The stele is opaque and fills its surface exactly, so there
    //     is never an uncovered region to frost; captured across a full
    //     collapse with the border, ledger and closing frame riding down
    //     intact frame by frame and no artifact at the trailing edge.
    // The calendar's surface is transparent and unblurred (its own layer
    // namespace, `blur off`), so ITS overhang is genuinely invisible and
    // stepping is free there. Same primitive, opposite window handling — which
    // is exactly why MorphState owns the state and the ORDER and not the
    // window.
    MorphState {
        id: mode
        // `openBay` is the COMPACT mechanism and only that: it exists because
        // the porch's own box was the only room the closed stele had. The
        // parthenon has a room, so expanding closes any open bay and the
        // pillars stay standing — a register never trades the façade away.
        // Expanding always lands on `playback`, so the naos never opens empty
        // of a reason.
        onExpandedChanged: { root.openBay = -1; root.register = 0 }
    }
    readonly property int naosH: mode.lerpInt(0, root.naosOpenH)

    // ── THE REGISTERS — which rank of pipes is sounding ─────────────────────
    // An organ's register selects which rank speaks; this stele's pipes are
    // literally columns, so the word costs nothing and renames a course that
    // was already going to be chips. Five of them, khoa 2026-08-16:
    //   0 playback  · per-app OUTPUT streams   (gold — the output channel)
    //   1 recording · per-app INPUT streams    (aegean — the input channel)
    //   2 outputs   · sinks                    (gold)
    //   3 inputs    · sources                  (aegean)
    //   4 bt        · paired bluetooth devices (teal)
    // The bt register is the fifth because pairing is not sink selection: a
    // connected headset does appear under `outputs` as a bluez sink, but
    // connecting it is a different verb, and with the pillars persisting the
    // bt roster would otherwise have no way in from the expanded face.
    //
    // TWO ROW SPECIES, not one. Registers 0–1 are STREAMS: they have a level
    // and a mute, so they wear the cue's row — mark · name · gloss · gauge ·
    // figure, click silences, wheel adjusts. Registers 2–4 are DEVICES: they
    // are a CHOICE, not a level, so they wear the picker's row — mark · name ·
    // gloss · rule, click selects, the current one marked in the channel's hue.
    // Both already existed in this file; a row carrying both a gauge and a
    // selection mark would be a third species with no precedent.
    property int register: 0
    readonly property var playbackRows: {
        var out = [], list = root.streamRoster || []
        for (var i = 0; i < list.length; i++)
            if (list[i] && list[i].out !== false) out.push(list[i])
        return out
    }
    readonly property var recordingRows: {
        var out = [], list = root.streamRoster || []
        for (var i = 0; i < list.length; i++)
            if (list[i] && list[i].out === false) out.push(list[i])
        return out
    }
    function registerRows(r) {
        if (r === 0) return root.playbackRows
        if (r === 1) return root.recordingRows
        if (r === 2) return root.sinkRoster
        if (r === 3) return root.sourceRoster
        if (r === 4) return root.btRoster
        return []
    }
    function registerIsStream(r) { return r === 0 || r === 1 }
    function registerWord(r) {
        if (r === 0) return "playback"
        if (r === 1) return "recording"
        if (r === 2) return "outputs"
        if (r === 3) return "inputs"
        if (r === 4) return "bt"
        return ""
    }
    // One hue per CHANNEL, carried across both of that channel's registers — the
    // same single job gold already does for the VOL fill, its plinth chip and
    // its picker's current row.
    function registerHue(r) {
        if (r === 1 || r === 3) return root.livery.holoBlue      // aegean — in
        if (r === 4) return root.livery.wireCyan                 // teal — bt
        return root.livery.paletteAccent                         // gold — out
    }
    // What the stele's own top frame says. One band captions the body: the
    // porch's word closed, the open bay's word while a bay is open, the
    // sounding register's word once the naos is up.
    function frameWord() {
        if (mode.expanded) return root.registerWord(root.register)
        return root.bayWord(root.openBay)
    }
    // The bay a plinth chip routes to once the naos is open — the same control
    // meaning ("show me this bay's choices") in the room that now has space
    // for the answer.
    function bayRegister(b) { return b === 1 ? 2 : (b === 0 ? 3 : 4) }

    width: 352
    implicitHeight: stele.height + 5      // +5 clears the cast shadow's overhang

    // type voices — shared across the pantheon
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // this stele's signature — MUREX (base0E). See the header's SIGNATURE note.
    readonly property color sig: livery.violet
    readonly property color ink: livery.paletteFg

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }
    function mix(a, b, t) {
        var ca = Qt.color(a), cb = Qt.color(b)
        return Qt.rgba(ca.r + (cb.r - ca.r) * t, ca.g + (cb.g - ca.g) * t,
                       ca.b + (cb.b - ca.b) * t, 1.0)
    }
    function stepTone(a) {                              // warm marble step tone
        var w = mix(livery.paletteFg, livery.paletteBg, 0.42)
        return Qt.rgba(w.r, w.g, w.b, a)
    }
    // one mood face reading the whole box (kana/punct proven safe by MoodFaces)
    // ONE mood face reading the whole box, extended rather than joined by a
    // second one when the naos opened: with the interior showing, the room's
    // own state outranks the two shafts, because that is what you are looking
    // at. Every glyph stays inside the kana/punct vocabulary MoodFaces proves
    // safe on this stack.
    function kaomojiFor() {
        var mutedCount = (outMuted ? 1 : 0) + (inMuted ? 1 : 0)
        if (mode.expanded && channelCount === 0) return "( ･ - ･ )"   // an empty room
        if (mode.expanded && hushedCount > 0) return "( >ω<)"       // some hushed
        if (mutedCount === 2) return "(-_- )"                 // both hushed
        if (mutedCount === 1) return "( ･_･)"                 // one silenced
        if (btConnected) return "♪( ˘ω˘ )"                    // bound, listening
        if (Math.max(outPct, inPct) >= 85) return "♪(´▽｀)"   // loud & lively
        return "( ･ω･)ﾉ"                                      // attentive
    }
    function shortName(s, n) {
        var t = "" + s
        if (t.length <= n) return t
        return t.substring(0, n - 1) + "…"
    }

    // ── The open bay, resolved four ways ────────────────────────────────────
    // One switch each rather than four parallel arrays, so adding a bay is a
    // line in each and nothing can fall out of step silently.
    function bayRoster(b) {
        if (b === 0) return root.sourceRoster
        if (b === 1) return root.sinkRoster
        if (b === 2) return root.btRoster
        return []
    }
    function bayHue(b) {
        if (b === 0) return root.livery.holoBlue        // aegean — the input
        if (b === 1) return root.livery.paletteAccent   // Attic gold — output
        if (b === 2) return root.livery.wireCyan        // teal — bluetooth
        return root.ink
    }
    // What the stele's own top frame says while this bay is open. The bay's
    // own word, plural, because the frame is captioning a LIST.
    function bayWord(b) {
        if (b === 0) return "in · sources"
        if (b === 1) return "out · sinks"
        if (b === 2) return "bt · devices"
        return "levels"
    }
    // The line a bay's chip carries at rest: the current entry's name, or the
    // honest absence. The bt bay counts instead of naming, because its tally
    // one line above already names the connected device and nothing in this
    // house reads twice.
    function bayChipText(b) {
        var list = root.bayRoster(b)
        if (b === 2) {
            if (!root.btAvail) return "no adapter"
            if (list.length === 0) return "none paired"
            return list.length + (list.length === 1 ? " device" : " devices")
        }
        for (var i = 0; i < list.length; i++)
            if (list[i].current) return root.shortName(list[i].name, 13)
        return list.length > 0 ? "none set" : "—"
    }
    function pickInBay(b, key) {
        if (b === 0) root.pickSource(key)
        else if (b === 1) root.pickSink(key)
        else if (b === 2) root.pickBt(key)
        root.openBay = -1
    }
    // The same three writes from the naos. It does NOT close the register
    // afterwards the way a bay closes: the compact picker is a detour off the
    // porch and wants to end, while a register is a place you are standing in
    // and expect to still be standing in after you pick.
    function pickInRegister(r, key) {
        if (r === 2) root.pickSink(key)
        else if (r === 3) root.pickSource(key)
        else if (r === 4) root.pickBt(key)
    }

    // colonnade geometry — one place, so architrave/shafts/tally/plinth stay
    // aligned. TRISTYLE as of 2026-08-15 (was distyle); widened 2026-08-16 so
    // a plinth chip can hold a real channel name: 3 * 84 + 2 * 26 = 304 inside
    // a 330px content width, leaving a 13px inset the stylobate's widest step
    // (inset - 9) still clears.
    readonly property int colW: 84
    readonly property int bayGap: 26
    readonly property int shaftW: 24
    readonly property int pierW: 34          // the bluetooth pier — wider, it
                                             // carries an inscription per drum
    readonly property real breakFrac: 0.52   // where a muted column snaps

    // ── naos geometry — the interior, revealed under the porch ──────────────
    // The porch band is untouched at 168; the naos is what grows beneath it.
    // Six slots is the cap: at 20px a seventh row costs another 20 of a popup
    // that already hangs 465 off a 36px bar, and a machine with more than six
    // things making noise has `[ mixer ]` for it. Past six the list scrolls.
    readonly property int porchH: 168
    readonly property int rowH: 20               // one manuscript row, both species
    readonly property int channelSlots: 6
    readonly property int regChipW: 56           // 5 * 56 + 4 * 6 = 304, the porch's
    readonly property int regGap: 6              // own measure to the pixel
    readonly property int naosOpenH: 4 + 1 + 4 + 15 + 4
                                    + root.channelSlots * root.rowH
    readonly property int channelCount: root.streamRoster ? root.streamRoster.length : 0
    readonly property int hushedCount: {
        var list = root.streamRoster || []
        var n = 0
        for (var i = 0; i < list.length; i++)
            if (list[i] && list[i].muted === true) n++
        return n
    }

    // ══ ONE COLUMN — capital + fluted shaft, or a broken ruin when muted ══════
    component Colonna : Item {
        id: col
        property string order: "doric"       // "doric" | "ionic" | "corinthian"
        property int pct: 0
        property bool muted: false
        property bool avail: true
        property color fillHue: root.livery.paletteAccent
        property alias hovering: hoverMa.containsMouse
        property real shaftWidth: root.shaftW
        signal toggle()
        signal adjust(int delta)

        // ── DRUM MODE — the shaft is built of two inscribed drums instead of
        //    being a rising meter. Used by the bluetooth pier: the drums are
        //    the A2DP ↔ headset switch. `drumsLive` false leaves them carved
        //    but inert (adapter on, nothing connected), which is how the pier
        //    still reads as a pier with no device to switch.
        property bool drums: false
        property var drumLabels: []          // top-to-bottom
        property int activeDrum: -1
        property bool drumsLive: false
        signal drumPicked(int index)

        readonly property bool broken: muted && avail
        readonly property real frac: avail ? Math.max(0, Math.min(1, pct / 100)) : 0
        readonly property color liveHue: !avail ? root.withA(root.ink, 0.22) : fillHue

        // The full-bay catcher is declared FIRST so the drums' own MouseAreas
        // (declared later, inside the shaft) win the hit test — the house rule
        // from notifications.qml's click-anywhere-dismiss.
        MouseArea {
            id: hoverMa
            anchors.fill: parent
            hoverEnabled: true
            enabled: col.avail
            cursorShape: Qt.PointingHandCursor
            onClicked: col.toggle()
            onWheel: function(wheel) {
                col.adjust(wheel.angleDelta.y > 0 ? 2 : -2)
                wheel.accepted = true
            }
        }

        // ── the capital — order-specific; GONE when the column is broken ────
        Canvas {
            id: capital
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: col.width
            height: 15
            visible: !col.broken
            readonly property color stroke: col.avail ? root.withA(root.ink, 0.85)
                                                      : root.withA(root.ink, 0.4)
            readonly property string ord: col.order
            readonly property real sw: col.shaftWidth
            onStrokeChanged: requestPaint()
            onOrdChanged: requestPaint()
            onSwChanged: requestPaint()
            onVisibleChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset(); ctx.clearRect(0, 0, width, height)
                if (!visible) return
                ctx.strokeStyle = stroke
                var cx = width / 2
                var shaftHalf = col.shaftWidth / 2
                if (ord === "corinthian") {
                    // Corinthian — a CONCAVE abacus over a flaring kalathos
                    // bell, two rows of acanthus tips rising off the neck, and
                    // a small helix scroll tucked under each abacus corner.
                    // The ornate order for the ornate channel; told apart from
                    // Doric's plain cushion and Ionic's two big volutes by the
                    // leaf course alone, colour removed.
                    var kH = shaftHalf + 11
                    ctx.lineWidth = 1.2
                    ctx.beginPath()                                  // abacus slab,
                    ctx.moveTo(cx - kH, 1)                           // concave front
                    ctx.lineTo(cx + kH, 1)
                    ctx.moveTo(cx - kH, 3.2)
                    ctx.quadraticCurveTo(cx, 5.2, cx + kH, 3.2)
                    ctx.stroke()
                    ctx.beginPath()                                  // the kalathos —
                    ctx.moveTo(cx - shaftHalf, height - 1)           // a flaring bell
                    ctx.bezierCurveTo(cx - shaftHalf - 1, height - 6,
                                      cx - kH + 1, 8, cx - kH + 1, 4.4)
                    ctx.moveTo(cx + shaftHalf, height - 1)
                    ctx.bezierCurveTo(cx + shaftHalf + 1, height - 6,
                                      cx + kH - 1, 8, cx + kH - 1, 4.4)
                    ctx.stroke()
                    ctx.lineWidth = 1.0
                    ctx.beginPath()                                  // corner helices,
                    ctx.arc(cx - kH + 3.2, 6.6, 1.9, 0, 2 * Math.PI) // tucked under
                    ctx.moveTo(cx + kH - 1.3, 6.6)                   // the abacus
                    ctx.arc(cx + kH - 3.2, 6.6, 1.9, 0, 2 * Math.PI)
                    ctx.stroke()
                    ctx.beginPath()                                  // acanthus, two
                    for (var a = -1; a <= 1; a++) {                   // tiers of tips
                        var lx = cx + a * (shaftHalf * 0.62)
                        ctx.moveTo(lx - 2.4, height - 1)
                        ctx.quadraticCurveTo(lx, height - 9, lx + 2.4, height - 1)
                    }
                    for (var b = -1; b <= 1; b += 2) {
                        var ux = cx + b * (shaftHalf * 0.31)
                        ctx.moveTo(ux - 2.2, height - 5)
                        ctx.quadraticCurveTo(ux, height - 11.5, ux + 2.2, height - 5)
                    }
                    ctx.stroke()
                } else if (ord === "ionic") {
                    // Ionic — an abacus over two volute scrolls joined by an ovolo
                    ctx.lineWidth = 1.8
                    var abacusHalf = shaftHalf + 9
                    ctx.beginPath()
                    ctx.moveTo(cx - abacusHalf, 1.5); ctx.lineTo(cx + abacusHalf, 1.5)
                    ctx.stroke()
                    var eyeY = height * 0.52
                    function volute(ex, dir) {
                        ctx.beginPath()
                        var turns = 1.7, steps = 46, r0 = 6.2
                        for (var i = 0; i <= steps; i++) {
                            var t = i / steps
                            var ang = dir * (t * turns * 2 * Math.PI) - Math.PI / 2
                            var r = r0 * (1 - 0.66 * t)
                            var x = ex + r * Math.cos(ang), y = eyeY + r * Math.sin(ang)
                            if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y)
                        }
                        ctx.stroke()
                    }
                    volute(cx - shaftHalf - 3,  1)
                    volute(cx + shaftHalf + 3, -1)
                    ctx.lineWidth = 1.4
                    ctx.beginPath()
                    ctx.moveTo(cx - shaftHalf, 3.5)
                    ctx.quadraticCurveTo(cx, height - 1, cx + shaftHalf, 3.5)
                    ctx.stroke()
                } else {
                    // Doric — a THIN abacus slab over a SHALLOW echinus cushion
                    // (wider than tall) + a neck ring. No lampshade.
                    ctx.lineWidth = 1.3
                    var aH = shaftHalf + 8
                    ctx.beginPath()
                    ctx.moveTo(cx - aH, 2);  ctx.lineTo(cx + aH, 2)          // abacus top
                    ctx.moveTo(cx - aH + 1, 5); ctx.lineTo(cx + aH - 1, 5)   // abacus bottom (thin slab)
                    ctx.stroke()
                    ctx.beginPath()                                          // echinus cushion
                    ctx.moveTo(cx - aH + 1, 5)
                    ctx.quadraticCurveTo(cx - shaftHalf - 3, height - 2, cx - shaftHalf, height - 1)
                    ctx.moveTo(cx + aH - 1, 5)
                    ctx.quadraticCurveTo(cx + shaftHalf + 3, height - 2, cx + shaftHalf, height - 1)
                    ctx.moveTo(cx - shaftHalf, height - 1); ctx.lineTo(cx + shaftHalf, height - 1)  // neck ring
                    ctx.stroke()
                }
            }
        }

        // ── THE SHAFT — the fluted meter; a broken stump + rubble when muted ─
        Item {
            id: shaftBox
            anchors.top: parent.top
            anchors.topMargin: col.broken ? 0 : capital.height + 1
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            width: col.shaftWidth
            readonly property real neck: 6
            readonly property real innerH: height - 2 - neck

            // ── INTACT shaft (unmuted): track · rising fill · flutes · neck ──
            Item {
                anchors.fill: parent
                visible: !col.broken && !col.drums

                Rectangle {                                   // unlit stone track
                    anchors.fill: parent; radius: 0
                    color: root.withA(root.ink, 0.09)
                    border.width: 1
                    border.color: root.withA(root.ink, col.avail ? 0.5 : 0.25)
                }
                Rectangle {                                   // rising FILL — the level
                    id: fill
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.bottom: parent.bottom; anchors.margins: 1
                    height: shaftBox.innerH * col.frac
                    radius: 0
                    color: root.withA(col.liveHue, 0.9)
                    Behavior on height { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
                    Rectangle {                               // meniscus — crisp cap line
                        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                        height: 1; visible: col.frac > 0.01
                        color: root.withA(root.ink, 0.55)
                    }
                }
                Canvas {                                      // fluting + trachelion groove
                    anchors.fill: parent
                    readonly property color groove: root.withA(root.ink, col.avail ? 0.35 : 0.2)
                    onGrooveChanged: requestPaint()
                    onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset(); ctx.clearRect(0, 0, width, height)
                        ctx.strokeStyle = groove; ctx.lineWidth = 1
                        for (var i = 1; i <= 3; i++) {
                            var x = Math.round(width * i / 4) + 0.5
                            ctx.beginPath(); ctx.moveTo(x, 2); ctx.lineTo(x, height - 2); ctx.stroke()
                        }
                        var ny = Math.round(shaftBox.neck) + 0.5
                        ctx.beginPath(); ctx.moveTo(1.5, ny); ctx.lineTo(width - 1.5, ny); ctx.stroke()
                    }
                }
            }

            // ── DRUM shaft: two inscribed drums instead of a rising meter ───
            //    A real shaft is built of stacked drums; here the stack is
            //    exactly two, and which one is LIT is the mode. The lit drum
            //    fills in the column's hue with its inscription reversed out
            //    in ground colour — the same "one fill, one hue" move the
            //    calendar spends on today's cell — so the mode reads from the
            //    silhouette with the colour removed.
            Item {
                id: drumStack
                anchors.fill: parent
                visible: col.drums && !col.broken

                Rectangle {                             // the pier's own stone —
                    anchors.fill: parent                // ONE box, jointed, not
                    radius: 0                           // two stacked boxes
                    color: root.withA(root.ink, 0.09)
                    border.width: 1
                    border.color: root.withA(root.ink, col.avail ? 0.5 : 0.25)
                }
                Repeater {
                    model: col.drumLabels
                    delegate: Item {
                        id: drum
                        required property int index
                        required property var modelData
                        width: drumStack.width
                        height: drumStack.height / 2
                        y: index * (drumStack.height / 2)
                        readonly property bool lit: col.drumsLive && col.activeDrum === index

                        Rectangle {                     // the lit face
                            anchors.fill: parent
                            anchors.margins: 1
                            radius: 0
                            color: drum.lit ? root.withA(col.liveHue, 0.9) : "transparent"
                            Behavior on color { ColorAnimation { duration: 150 } }
                        }
                        Text {                          // the inscription
                            anchors.centerIn: parent
                            text: "" + drum.modelData
                            font.family: root.faceMono
                            font.pixelSize: 9
                            font.letterSpacing: 1
                            color: drum.lit ? root.livery.paletteBg
                                            : root.withA(root.ink,
                                                  !col.avail ? 0.25
                                                : (col.drumsLive ? 0.8 : 0.32))
                        }
                        MouseArea {                     // wins over hoverMa
                            anchors.fill: parent
                            enabled: col.drumsLive
                            cursorShape: Qt.PointingHandCursor
                            onClicked: col.drumPicked(drum.index)
                        }
                    }
                }
                Rectangle {                             // the drum joint
                    anchors.left: parent.left; anchors.right: parent.right
                    y: Math.round(parent.height / 2)
                    height: 1
                    color: root.withA(root.ink, col.avail ? 0.5 : 0.25)
                }
            }

            // ── BROKEN shaft (muted): a snapped stump + jagged break + rubble ─
            Canvas {
                id: ruin
                anchors.fill: parent
                visible: col.broken
                readonly property color stone: root.withA(root.ink, 0.16)
                readonly property color edge:  root.withA(root.ink, 0.5)
                // the fracture — dark STONE, not a red wound (khoa: no red on the
                // broken part of the column; the ruin reads by silhouette).
                readonly property color wound: root.withA(root.ink, 0.72)
                onVisibleChanged: requestPaint()
                onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset(); ctx.clearRect(0, 0, width, height)
                    if (!visible) return
                    var topY = height * (1 - root.breakFrac)       // the stump's break height
                    // a LOW irregular crest atop a solid stub (not a tall zigzag):
                    // short teeth of ±2px so it reads as a fracture, not a bolt.
                    var jag = [topY + 1, topY - 2, topY,     topY - 1,
                               topY + 1, topY - 2, topY + 1, topY - 1, topY]
                    // the stump body (dim stone), capped by the jagged break
                    ctx.beginPath()
                    ctx.moveTo(1, height - 1)
                    ctx.lineTo(1, jag[0])
                    for (var i = 0; i < jag.length; i++)
                        ctx.lineTo(1 + (width - 2) * (i / (jag.length - 1)), jag[i])
                    ctx.lineTo(width - 1, height - 1)
                    ctx.closePath()
                    ctx.fillStyle = stone; ctx.fill()
                    // side edges of the stump
                    ctx.strokeStyle = edge; ctx.lineWidth = 1
                    ctx.beginPath()
                    ctx.moveTo(1.5, height - 1); ctx.lineTo(1.5, jag[0])
                    ctx.moveTo(width - 1.5, height - 1); ctx.lineTo(width - 1.5, jag[jag.length - 1])
                    ctx.stroke()
                    // fluting on the stump (below the break)
                    ctx.strokeStyle = root.withA(root.ink, 0.32)
                    for (var f = 1; f <= 3; f++) {
                        var fx = Math.round(width * f / 4) + 0.5
                        var fyTop = jag[Math.min(jag.length - 1, Math.round((jag.length - 1) * f / 4))] + 3
                        ctx.beginPath(); ctx.moveTo(fx, fyTop); ctx.lineTo(fx, height - 2); ctx.stroke()
                    }
                    // the fracture — the jagged break line, in dark STONE (not red)
                    ctx.strokeStyle = wound; ctx.lineWidth = 1.6
                    ctx.beginPath()
                    for (var j = 0; j < jag.length; j++) {
                        var jx = 1 + (width - 2) * (j / (jag.length - 1))
                        if (j === 0) ctx.moveTo(jx, jag[j]); else ctx.lineTo(jx, jag[j])
                    }
                    ctx.stroke()
                    // (the fallen debris is drawn by the wider `rubble` field
                    //  below, so the pieces can scatter across the whole foot.)
                }
            }
        }

        // ── SCATTERED RUBBLE — the toppled capital + chips strewn across the
        //    whole foot of a broken column (spans the bay, not just the shaft,
        //    so the pieces scatter AROUND the base, tumbled at varied angles) ─
        Canvas {
            id: rubble
            anchors.bottom: parent.bottom
            anchors.bottomMargin: -1
            anchors.horizontalCenter: parent.horizontalCenter
            width: col.width
            height: 24
            visible: col.broken
            onVisibleChanged: requestPaint()
            onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset(); ctx.clearRect(0, 0, width, height)
                if (!visible) return
                var fill = root.withA(root.ink, 0.2)
                var edge = root.withA(root.ink, 0.62)
                var cx = width / 2
                function chip(x, y, w, h, rot) {
                    ctx.save(); ctx.translate(x, y); ctx.rotate(rot)
                    ctx.fillStyle = fill; ctx.fillRect(-w / 2, -h / 2, w, h)
                    ctx.strokeStyle = edge; ctx.lineWidth = 1; ctx.strokeRect(-w / 2, -h / 2, w, h)
                    ctx.restore()
                }
                // the fallen capital — a wide abacus slab tumbled off to one side,
                // its order still legible (a volute scroll on its end for Ionic)
                ctx.save()
                ctx.translate(cx - 21, height * 0.52); ctx.rotate(-0.52)
                ctx.fillStyle = fill; ctx.fillRect(-9, -5, 18, 10)
                ctx.strokeStyle = edge; ctx.lineWidth = 1; ctx.strokeRect(-9, -5, 18, 10)
                if (col.order === "ionic") {
                    ctx.beginPath(); ctx.arc(-9, 0, 3.0, 0, 2 * Math.PI); ctx.stroke()
                    ctx.beginPath(); ctx.arc(-9, 0, 1.2, 0, 2 * Math.PI); ctx.stroke()
                }
                ctx.restore()
                // drum fragments + chips, scattered across the foot at varied angles
                chip(cx + 20, height * 0.60, 10, 6,  0.50)
                chip(cx + 30, height * 0.40,  6, 5, -0.35)
                chip(cx +  4, height * 0.72,  7, 5,  0.18)
                chip(cx + 11, height * 0.46,  5, 4, -0.62)
                chip(cx - 30, height * 0.66,  6, 5,  0.40)
                chip(cx -  8, height * 0.78,  5, 4, -0.15)
            }
        }

    }

    // ══ ONE DEVICE — a choosable channel, as a manuscript row ════════════════
    // launcher.qml's row: margin mark · serif name · mono gloss · hairline
    // rule, with the CURRENT entry marked in the channel's own hue — the same
    // "filled in the bay's hue is the live one" language the lit drum and the
    // rising fill already speak. Shared verbatim by the compact bay picker and
    // the naos's three device registers, so `current` can only ever come from
    // one place and the two faces cannot disagree about it.
    component DeviceRow : Item {
        id: entry
        required property var modelData
        property color hue: root.ink
        signal picked(string key)

        readonly property bool current: entry.modelData
                                        && entry.modelData.current === true
        readonly property bool hot: entryMa.containsMouse

        Text {                                   // the margin mark
            id: entryMark
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            width: 13
            horizontalAlignment: Text.AlignHCenter
            text: entry.current ? "♪" : "·"
            font.family: root.faceMusic
            font.pixelSize: entry.current ? 13 : 11
            color: entry.current ? entry.hue : root.withA(root.ink, 0.3)
        }
        Text {                                   // who
            id: entryName
            anchors.left: entryMark.right; anchors.leftMargin: 5
            anchors.right: entryGloss.left; anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            elide: Text.ElideRight
            text: entry.modelData ? ("" + entry.modelData.name) : ""
            font.family: root.faceSerif
            font.pixelSize: 11
            font.weight: entry.current ? Font.DemiBold : Font.Normal
            color: (entry.current || entry.hot) ? root.ink
                                                : root.withA(root.ink, 0.85)
        }
        Text {                                   // how
            id: entryGloss
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            text: entry.modelData ? ("" + entry.modelData.gloss) : ""
            font.family: root.faceMono
            font.pixelSize: 8
            font.letterSpacing: 1
            color: root.withA(root.ink, 0.45)
        }
        Rectangle {                              // the rule
            anchors.left: parent.left; anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: entry.current ? 2 : 1
            color: entry.current ? entry.hue
                 : root.withA(entry.hot ? entry.hue : root.ink,
                              entry.hot ? 0.6 : 0.13)
            Behavior on color { ColorAnimation { duration: 150 } }
        }
        MouseArea {
            id: entryMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: entry.picked(entry.modelData ? ("" + entry.modelData.key) : "")
        }
    }

    // ══ ONE CHANNEL — a per-application stream, as a manuscript row ════════════
    // launcher.qml's row, which the picker already borrowed, with the cue's
    // hairline gauge spliced in between the gloss and the figure. Declared at
    // the ROOT's level and not inside another component — Quickshell's engine
    // rejects a nested inline component outright ("Nested inline components are
    // not supported", hit live in bar.qml).
    //
    // The margin mark is the STATE, not a selection: `♪` while the channel
    // sounds, `𝄽` — the rest — when it is hushed, which is the grammar's own
    // muted treatment (a rest glyph in place of the value) moved to the margin
    // so the figure can keep saying how loud it WOULD be. No laurel anywhere:
    // a mixer has no one selected row, and inventing one to spend the blaze on
    // would be backwards.
    component ChannelRow : Item {
        id: channel
        required property var modelData
        readonly property string vkey:  channel.modelData ? ("" + channel.modelData.key) : ""
        readonly property string vname: channel.modelData ? ("" + channel.modelData.name) : ""
        readonly property string vgloss: channel.modelData ? ("" + channel.modelData.gloss) : ""
        readonly property int  vpct:   channel.modelData ? (channel.modelData.pct | 0) : 0
        readonly property bool vmuted: channel.modelData && channel.modelData.muted === true
        readonly property bool vout:   !channel.modelData || channel.modelData.out !== false
        // Each row wears its own bay's hue, so a capture stream reads as
        // belonging to the aegean MIC pillar and a playback one to the gold
        // VOL pillar without a word being spent on which it is.
        readonly property color hue: channel.vout ? root.livery.paletteAccent
                                                : root.livery.holoBlue
        readonly property bool hot: channelMa.containsMouse

        Text {                                   // the margin mark — the state
            id: vMark
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            width: 13
            horizontalAlignment: Text.AlignHCenter
            text: channel.vmuted ? "𝄽" : "♪"
            font.family: root.faceMusic
            font.pixelSize: channel.vmuted ? 12 : 13
            color: channel.vmuted ? root.livery.paletteUrgent
                                : root.withA(channel.hue, channel.hot ? 1.0 : 0.8)
        }
        Text {                                   // who
            id: vName
            anchors.left: vMark.right; anchors.leftMargin: 5
            anchors.right: vGloss.left; anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            elide: Text.ElideRight
            text: channel.vname
            font.family: root.faceSerif; font.pixelSize: 11
            color: root.withA(root.ink, channel.hot ? 1.0 : 0.85)
        }
        Text {                                   // what it is playing
            id: vGloss
            anchors.right: vGauge.left; anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            width: 72
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
            text: channel.vgloss
            font.family: root.faceMono; font.pixelSize: 8
            font.letterSpacing: 1
            color: root.withA(root.ink, 0.45)
        }
        Item {                                   // the gauge — the cue's own
            id: vGauge                           // hairline track, no box
            anchors.right: vValue.left; anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            width: 46; height: 5
            Rectangle {
                anchors.fill: parent; radius: 0
                color: root.withA(root.ink, 0.09)
                border.width: 1
                border.color: root.withA(root.ink, 0.35)
            }
            Rectangle {
                anchors.left: parent.left; anchors.top: parent.top
                anchors.bottom: parent.bottom; anchors.margins: 1
                width: (parent.width - 2) * Math.max(0, Math.min(1, channel.vpct / 100))
                radius: 0
                color: channel.vmuted ? root.withA(root.ink, 0.28)
                                    : root.withA(channel.hue, 0.9)
                Behavior on width { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
                Behavior on color { ColorAnimation { duration: 150 } }
            }
        }
        Text {                                   // the figure
            id: vValue
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            width: 32
            horizontalAlignment: Text.AlignRight
            text: channel.vpct + "%"
            font.family: root.faceMono; font.pixelSize: 10
            color: channel.vmuted ? root.livery.paletteUrgent
                                : root.withA(root.ink, channel.hot ? 1.0 : 0.8)
        }
        Rectangle {                              // the rule — hover promotes it
            anchors.left: parent.left; anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: root.withA(channel.hot ? channel.hue : root.ink,
                              channel.hot ? 0.6 : 0.13)
            Behavior on color { ColorAnimation { duration: 150 } }
        }
        MouseArea {
            id: channelMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.streamToggle(channel.vkey)
            onWheel: function(wheel) {
                root.streamAdjust(channel.vkey, wheel.angleDelta.y > 0 ? 2 : -2)
                wheel.accepted = true
            }
        }
    }

    // cast shadow — shared pantheon idiom ───────────────────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: root.withA(root.ink, 0.22)
    }

    // ══ THE STELE — opaque marble body, border, inset keyline ════════════════
    Rectangle {
        id: stele
        width: parent.width
        anchors.top: parent.top
        radius: 0
        color: root.livery.paletteBg
        border.color: root.ink
        border.width: 2
        height: content.implicitHeight + 20

        Rectangle {                                       // inset amber keyline
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.sig; border.width: 1
        }

        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 11
            spacing: 4

            // ── ENTABLATURE: crown + carved name + order tag ─────────────────
            Item {
                width: parent.width; height: 24
                Text {
                    id: crown
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -2
                    text: "♫"; font.family: root.faceMusic; font.pixelSize: 22
                    color: root.sig
                }
                Text {
                    anchors.left: crown.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "AUDIO"
                    font.family: root.faceSerif; font.pixelSize: 15
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: root.ink
                }
                Text {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "[ pipewire ]"
                    font.family: root.faceMono; font.pixelSize: 10
                    color: root.withA(root.sig, 0.9)
                }
            }

            // ── bead-and-reel (astragal) frieze, in amber ───────────────────
            Canvas {
                id: frieze
                width: parent.width; height: 10
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset(); ctx.clearRect(0, 0, width, height)
                    // 0.85 → 0.7, one rung down the ladder, and only after
                    // looking: murex carries roughly twice amber's contrast on
                    // this marble, and at the old alpha the bead course was the
                    // loudest thing on the stele — it out-read its own crown.
                    ctx.strokeStyle = root.withA(root.sig, 0.7)
                    ctx.fillStyle = root.withA(root.sig, 0.7)
                    ctx.lineWidth = 1
                    var cy = height / 2, period = 13
                    for (var x = 3; x < width - 3; x += period) {
                        ctx.beginPath(); ctx.arc(x, cy, 2.4, 0, 2 * Math.PI); ctx.fill()   // round bead
                        // the reel — a spool/lozenge between beads (double-cone), so
                        // the course reads as carved bead-AND-reel, not a dotted rule
                        var rx = x + period / 2
                        ctx.beginPath()
                        ctx.moveTo(rx - 3.5, cy); ctx.lineTo(rx, cy - 2.2)
                        ctx.lineTo(rx + 3.5, cy); ctx.lineTo(rx, cy + 2.2)
                        ctx.closePath(); ctx.fill()
                    }
                }
            }

            // ── box-drawing top frame ───────────────────────────────────────
            Item {
                width: parent.width; height: 15
                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    // The frame label IS the picker's caption — an open bay
                    // renames this band rather than growing a second one.
                    text: "┌─┤ ♪ " + root.frameWord() + " ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                // THE NAOS CONTROL — the one thing on this stele that changes
                // its size, so it gets the pressable edge a control has in this
                // house and a tag never does (calendar.qml's `[ ἔτος ⌄ ]`
                // argument, and the same drawn chip the plinth already wears).
                // It carries the live channel count as well as the verb, so it is
                // ALSO this stele's per-app figure and the word "channels"
                // appears exactly once on the surface — the naos captions its
                // rows by their SOURCE (`♪ apps`), the way a notification card
                // captions itself with the program that sent it.
                Rectangle {
                    id: naosChip
                    anchors.right: tfR.left; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    width: 72; height: 15
                    radius: 0
                    color: naosMa.containsMouse ? root.withA(root.sig, 0.16)
                         : (mode.expanded ? root.withA(root.sig, 0.10) : "transparent")
                    border.width: 1
                    border.color: root.withA(root.sig,
                                             naosMa.containsMouse ? 0.9
                                           : (mode.expanded ? 0.75 : 0.45))
                    Behavior on color { ColorAnimation { duration: 150 } }
                    Behavior on border.color { ColorAnimation { duration: 150 } }
                    Text {
                        anchors.centerIn: parent
                        // ⌄ / ⌃ are calendar.qml's own pair, already rendering
                        // live at this face — a proven glyph reused.
                        text: root.channelCount + " channels " + (mode.expanded ? "⌃" : "⌄")
                        font.family: root.faceMono; font.pixelSize: 9
                        color: root.sig
                        opacity: naosMa.containsMouse ? 1.0 : 0.85
                    }
                    MouseArea {
                        id: naosMa
                        anchors.fill: parent; anchors.margins: -3
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mode.toggle()
                    }
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: naosChip.left
                    anchors.leftMargin: 2; anchors.rightMargin: 5
                    anchors.verticalCenter: parent.verticalCenter; anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }

            // ══ THE TEMPLE — the porch, and under it the naos it opens onto ══
            // ONE band in the content Column, not two, deliberately: a second
            // band whose height lerps from 0 would pop the Column's 4px spacing
            // in at the first frame of the morph, and nothing in this house
            // jumps. So the porch keeps its fixed 168 measured from the TOP of
            // this band, and the band's own height is what grows.
            Item {
                id: porch
                width: parent.width
                height: root.porchH + root.naosH

                // shared architrave beam carrying the MIC / VOL inscriptions
                Item {
                    id: architrave
                    anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right
                    height: 22
                    readonly property real inset: (parent.width - (root.colW * 3 + root.bayGap * 2)) / 2
                    Rectangle {                               // cornice (top rule)
                        anchors.top: parent.top
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: architrave.inset; anchors.rightMargin: architrave.inset
                        height: 1; color: root.withA(root.ink, 0.3)
                    }
                    // The inscriptions stay lit while a picker is open below
                    // them — that is what keeps the list attached to a
                    // PILLAR. The open bay holds full ink and the other two
                    // step back one rung of the ladder, which is the whole
                    // "which one did this come from" signal: no arrow, no
                    // tether, one opacity step (making-a-widget.md §3).
                    Row {
                        anchors.top: parent.top; anchors.topMargin: 3
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: root.bayGap
                        component Inscription : Text {
                            property int bay: -1
                            property bool avail: true
                            readonly property bool dimmed: root.openBay >= 0
                                                           && root.openBay !== bay
                            readonly property bool open: root.openBay === bay
                            width: root.colW; horizontalAlignment: Text.AlignHCenter
                            font.family: root.faceSerif; font.pixelSize: 12
                            font.weight: Font.DemiBold; font.letterSpacing: 3
                            // OPEN OUTRANKS UNAVAILABLE (2026-08-16). It used
                            // to be the other way round, and the bay that most
                            // needs the roster is exactly the unavailable one:
                            // with no default source set on this rig, opening
                            // MIC lit NO pillar — 0.4 against the other two's
                            // 0.35 is not a distinction — under a frame that
                            // claimed "in · sources". The band's whole job is
                            // to say which pillar the list came from.
                            color: open ? root.ink
                                 : (!avail ? root.withA(root.ink, 0.4)
                                 : (dimmed ? root.withA(root.ink, 0.35) : root.ink))
                            Behavior on color { ColorAnimation { duration: 150 } }
                        }
                        Inscription { text: "MIC"; bay: 0; avail: root.inAvail }
                        Inscription { text: "VOL"; bay: 1; avail: root.outAvail }
                        Inscription { text: "BT";  bay: 2; avail: root.btAvail }
                    }
                    Rectangle {                               // architrave (the carried beam)
                        anchors.bottom: parent.bottom
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: architrave.inset; anchors.rightMargin: architrave.inset
                        height: 1.5; color: root.withA(root.ink, 0.5)
                    }
                }

                // ── THE PORCH PROPER — what an open bay trades away ──────────
                // Columns, stylobate and tallies ride one Item so the picker
                // can take their exact rect. Cross-faded rather than switched:
                // `visible` is driven off the opacity so a faded-out porch
                // stops taking clicks, and the columns' own MouseAreas go with
                // it (nothing behind the picker is reachable while it is up).
                // ── THE FLOOR STAYS (2026-08-16) — the stepped stylobate is
                // hoisted OUT of porchBody so it survives an open bay. It used
                // to fade with the columns, and the picker then floated in a
                // floorless room between the beam above and the chips below;
                // a temple's floor does not leave with its columns. Position is
                // a plain binding rather than an anchor because its reference
                // (tallyRow) is now a grandchild, not a sibling — the geometry
                // is unchanged to the pixel.
                Item {
                    id: stylo
                    y: porchBody.y + tallyRow.y - height - 3
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 8
                    readonly property real inset: (parent.width - (root.colW * 3 + root.bayGap * 2)) / 2
                    Rectangle {
                        anchors.top: parent.top
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: stylo.inset - 2; anchors.rightMargin: stylo.inset - 2
                        height: 4; color: root.stepTone(0.85)
                    }
                    Rectangle {
                        anchors.top: parent.top; anchors.topMargin: 4
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: stylo.inset - 9; anchors.rightMargin: stylo.inset - 9
                        height: 3; color: root.stepTone(0.6)
                    }
                }

                Item {
                    id: porchBody
                    anchors.top: architrave.bottom
                    anchors.bottom: plinthRow.top
                    anchors.left: parent.left; anchors.right: parent.right
                    opacity: root.openBay >= 0 ? 0 : 1
                    visible: opacity > 0.01
                    Behavior on opacity { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }

                // the three columns, between architrave and stylobate
                Row {
                    anchors.top: parent.top
                    anchors.horizontalCenter: parent.horizontalCenter
                    // stylo is now this Row's UNCLE, not its sibling, so the
                    // shafts take its top as a height rather than an anchor.
                    height: Math.max(0, stylo.y - porchBody.y)
                    spacing: root.bayGap
                    Colonna {
                        id: micCol
                        width: root.colW; height: parent.height
                        order: "ionic"
                        pct: root.inPct; muted: root.inMuted; avail: root.inAvail
                        fillHue: root.livery.holoBlue          // aegean — cool input
                        onToggle: root.inToggle()
                        onAdjust: function(d) { root.inAdjust(d) }
                    }
                    Colonna {
                        id: volCol
                        width: root.colW; height: parent.height
                        order: "doric"
                        pct: root.outPct; muted: root.outMuted; avail: root.outAvail
                        fillHue: root.livery.paletteAccent     // Attic gold — warm output
                        onToggle: root.outToggle()
                        onAdjust: function(d) { root.outAdjust(d) }
                    }
                    // The BLUETOOTH pier — the third channel. Power is the base
                    // state (dark adapter = a RUINED column, the same broken
                    // silhouette a muted channel wears), and a connected device
                    // lights one of the two drums: the A2DP ↔ headset switch.
                    Colonna {
                        id: btCol
                        width: root.colW; height: parent.height
                        order: "corinthian"
                        shaftWidth: root.pierW
                        avail: root.btAvail
                        muted: root.btAvail && !root.btOn      // powered down = ruin
                        fillHue: root.livery.wireCyan           // teal — the third cool channel
                        drums: true
                        drumLabels: ["A2DP", "HSP"]
                        drumsLive: root.btConnected
                        activeDrum: root.btProfile === "a2dp" ? 0
                                  : (root.btProfile === "hsp" ? 1 : -1)
                        onToggle: root.btToggle()
                        onDrumPicked: function(i) { root.btPick(i === 0 ? "a2dp" : "hsp") }
                    }
                }

                // the tally row — level percent / MUTED / absent, under each column
                Row {
                    id: tallyRow
                    anchors.bottom: parent.bottom
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: root.bayGap
                    component Tally : Text {
                        property bool muted: false
                        property bool avail: true
                        property int pct: 0
                        width: root.colW; horizontalAlignment: Text.AlignHCenter
                        text: !avail ? "—" : (muted ? "MUTED" : pct + "%")
                        font.family: muted ? root.faceSerif : root.faceMono
                        font.pixelSize: muted ? 11 : 13
                        font.letterSpacing: muted ? 3 : 0
                        font.weight: muted ? Font.DemiBold : Font.Normal
                        color: muted ? root.livery.paletteUrgent
                                     : (avail ? root.ink : root.withA(root.ink, 0.4))
                    }
                    Tally { pct: root.inPct;  muted: root.inMuted;  avail: root.inAvail }
                    Tally { pct: root.outPct; muted: root.outMuted; avail: root.outAvail }
                    // The bluetooth tally reads WHO, not how loud — the drums
                    // above already carry the profile, so this never repeats
                    // it. With no device bound it falls back to the power word.
                    Text {
                        width: root.colW; horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        text: !root.btAvail ? "—"
                            : (!root.btOn ? "OFF"
                            : (root.btConnected ? root.shortName(root.btName, 10) : "ON"))
                        font.family: root.btConnected ? root.faceMono : root.faceSerif
                        font.pixelSize: root.btConnected ? 10 : 11
                        font.letterSpacing: root.btConnected ? 0 : 3
                        font.weight: root.btConnected ? Font.Normal : Font.DemiBold
                        color: !root.btAvail ? root.withA(root.ink, 0.4)
                             : (!root.btOn ? root.livery.paletteUrgent
                             : (root.btConnected ? root.livery.wireCyan : root.ink))
                    }
                }
                }   // ── end porchBody ────────────────────────────────────────

                // ══ THE PICKER — the open bay's roster, in the porch's rect ══
                // Anchored to the same band `porchBody` occupies, so it can
                // neither overlap another bay (there is no other bay while it
                // is up) nor escape the stele. Rows are launcher.qml's
                // manuscript row: a margin mark, a serif name, a mono gloss
                // and a hairline rule, with the CURRENT entry marked in the
                // bay's own hue — the same "filled in the bay's hue is the
                // live one" language the lit drum and the rising fill speak.
                Item {
                    id: picker
                    anchors.top: architrave.bottom; anchors.topMargin: 2
                    anchors.bottom: plinthRow.top; anchors.bottomMargin: 2
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    opacity: root.openBay >= 0 ? 1 : 0
                    visible: opacity > 0.01
                    Behavior on opacity { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }

                    readonly property color hue: root.bayHue(root.openBay)
                    readonly property var rows: root.bayRoster(root.openBay)
                    readonly property int slots: Math.max(0, Math.floor((height - 4) / root.rowH))

                    // BLANK MANUSCRIPT RULING under the unfilled slots — the
                    // launcher's sparse-state idiom at the ladder's bottom rung
                    // (0.13, greek-grammar.md §7). Before it, a one-entry
                    // roster was a single row floating in ~118px of bare marble
                    // and read as breakage rather than as room to spare. Drawn
                    // BEHIND the list, and the list is top-anchored now instead
                    // of centred, so rows hang from the beam that captions them.
                    Column {
                        anchors.top: parent.top; anchors.topMargin: 2
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: rosterView.width
                        Repeater {
                            model: picker.slots
                            delegate: Item {
                                width: rosterView.width; height: root.rowH
                                Rectangle {
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    height: 1
                                    color: root.withA(root.ink, 0.13)
                                }
                            }
                        }
                    }

                    // A ListView and not a Column: a machine can carry more
                    // sinks than this band has room for, and a clipped Column
                    // would simply hide them. Fixed height, fixed delegate
                    // height, no footer — the contentHeight loop recorded in
                    // hazards.md needs a footer or a self-referential size to
                    // bite, and this has neither.
                    ListView {
                        id: rosterView
                        // Inset to the architrave's own inset, so the rows'
                        // rules run the same width as the cornice above and
                        // the stylobate below rather than edge to edge — the
                        // list sits INSIDE the porch's measure, not across it.
                        readonly property real inset:
                            (parent.width - (root.colW * 3 + root.bayGap * 2)) / 2
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * inset
                        // Height from the ROW COUNT, never from contentHeight:
                        // sizing a view off its own contentHeight is the loop
                        // recorded in hazards.md §2. TOP-anchored (was centred):
                        // the rows hang from the architrave that captions them,
                        // and the ruled slots below carry the rest of the band.
                        height: Math.min(picker.rows.length * root.rowH,
                                         picker.slots * root.rowH)
                        y: 2
                        clip: true
                        model: picker.rows
                        boundsBehavior: Flickable.StopAtBounds
                        delegate: DeviceRow {
                            width: rosterView.width
                            height: root.rowH
                            hue: picker.hue
                            onPicked: function(k) { root.pickInBay(root.openBay, k) }
                        }
                    }

                    // The honest empty, now in the house's own register: the
                    // ITALIC line is the Greek answer-line and nothing else
                    // (greek-grammar.md §6), so the italic keeps `οὐδέν` and
                    // the English that has to be legible drops to a mono gloss
                    // beneath it — the bilingual inscription idiom (§10),
                    // not a second font decision.
                    Column {
                        anchors.centerIn: parent
                        spacing: 3
                        visible: picker.rows.length === 0
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "οὐδέν"
                            font.family: root.faceSerif; font.pixelSize: 12
                            font.italic: true
                            color: root.withA(root.ink, 0.5)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: root.openBay === 2 && !root.btAvail
                                  ? "no adapter" : "nothing to choose"
                            font.family: root.faceMono; font.pixelSize: 8
                            font.letterSpacing: 1
                            color: root.withA(root.ink, 0.35)
                        }
                    }
                }

                // ══ THE PLINTH — one chip per bay, naming its channel ════════
                // calendar.qml's `[ ἔτος ⌄ ]` control, tripled: a DRAWN chip
                // (hairline edge, radius 0, faint fill on hover) because that
                // file settled that a control in this house has a pressable
                // edge while a tag is only ever letters. At rest the edge is
                // ink and only the wedge carries the bay's hue; the OPEN chip
                // takes the hue outright, which is the one thing on this
                // course wearing colour at a time.
                Row {
                    id: plinthRow
                    // Measured from the TOP of the band, not its bottom: this
                    // band grows downward now and the plinth belongs to the
                    // porch, not to whatever is revealed beneath it.
                    y: root.porchH - height
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: root.bayGap
                    component Chip : Rectangle {
                        id: chip
                        property int bay: -1
                        // A chip is live when its bay has a ROSTER, not when
                        // the bay has an active channel — the case that forced
                        // this apart was a machine with a capture device and
                        // no default source set: the MIC column ghosts out
                        // correctly, and gating the chip on the same flag
                        // disabled the one control that could have set one.
                        // The bt chip is always live, because "no adapter" is
                        // an answer and you find it by opening the thing.
                        readonly property bool avail: chip.bay === 2
                                                      || root.bayRoster(chip.bay).length > 0
                        // Open means "this bay's choices are showing", which is
                        // the picker while the stele is closed and the matching
                        // REGISTER once the naos is up — same control, same
                        // meaning, the room it points into is what changed.
                        readonly property bool open: mode.expanded
                            ? root.register === root.bayRegister(chip.bay)
                            : root.openBay === chip.bay
                        readonly property color hue: root.bayHue(chip.bay)
                        width: root.colW
                        height: 15
                        radius: 0
                        color: !chip.avail ? "transparent"
                             : chip.open ? root.withA(chip.hue, 0.16)
                             : (chipMa.containsMouse ? root.withA(chip.hue, 0.10)
                                                     : "transparent")
                        border.width: 1
                        border.color: !chip.avail ? root.withA(root.ink, 0.18)
                                    : chip.open ? chip.hue
                                    : root.withA(root.ink,
                                                 chipMa.containsMouse ? 0.6 : 0.32)
                        Behavior on color { ColorAnimation { duration: 150 } }
                        Behavior on border.color { ColorAnimation { duration: 150 } }

                        Text {
                            id: chipWedge
                            anchors.right: parent.right; anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            // ⌄ / ⌃ are calendar.qml's own pair, already
                            // rendering live on this desktop at this face —
                            // a proven glyph reused, not a new one adopted.
                            text: chip.open ? "⌃" : "⌄"
                            font.family: root.faceMono; font.pixelSize: 9
                            color: chip.avail ? root.withA(chip.hue, chip.open ? 1.0 : 0.8)
                                              : root.withA(root.ink, 0.25)
                        }
                        Text {
                            anchors.left: parent.left; anchors.leftMargin: 5
                            anchors.right: chipWedge.left; anchors.rightMargin: 3
                            anchors.verticalCenter: parent.verticalCenter
                            elide: Text.ElideRight
                            text: root.bayChipText(chip.bay)
                            font.family: root.faceMono; font.pixelSize: 8
                            color: !chip.avail ? root.withA(root.ink, 0.3)
                                 : (chip.open || chipMa.containsMouse)
                                   ? root.ink : root.withA(root.ink, 0.72)
                        }
                        MouseArea {
                            id: chipMa
                            anchors.fill: parent
                            hoverEnabled: true
                            enabled: chip.avail
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (mode.expanded)
                                    root.register = root.bayRegister(chip.bay)
                                else
                                    root.openBay = chip.open ? -1 : chip.bay
                            }
                        }
                    }
                    Chip { bay: 0 }
                    Chip { bay: 1 }
                    Chip { bay: 2 }          // the bt chip stays live with no
                                             // adapter: it is how you find out
                }

                // ══ THE NAOS — the interior, under the porch's own floor ═════
                // Revealed downward by the naos chip in the top frame. It is
                // NOT a second widget and it does not repeat one part of the
                // stele: no crown, no wordmark, no second frieze — the ten
                // chrome parts above and below it are untouched and the porch
                // itself stays exactly where it was. What grows is part 9.
                //
                // Nothing in here becomes a pillar. A pillar exists in this
                // file only if its capital tells it apart with the colour
                // removed, and the classical inventory holds five orders in
                // total — streams are unbounded, transient occupants of the
                // temple, not its architecture, and a device list is a choice.
                // So both species are manuscript ROWS, the vocabulary the
                // picker already borrowed from launcher.qml.
                Item {
                    id: naos
                    y: root.porchH
                    anchors.left: parent.left; anchors.right: parent.right
                    height: root.naosH
                    clip: true                       // the paint is revealed
                    // Late fade, calendar.qml's own two-layer curve: the room
                    // is dark until it is most of the way open, so the rows
                    // never read as sliding out from under the plinth.
                    opacity: Math.max(0, Math.min(1, mode.frac * 2.2 - 0.6))

                    readonly property real inset:
                        (parent.width - (root.colW * 3 + root.bayGap * 2)) / 2
                    readonly property var rows: root.registerRows(root.register)
                    readonly property bool streams: root.registerIsStream(root.register)
                    readonly property color hue: root.registerHue(root.register)
                    readonly property int listY: 4 + 1 + 4 + 15 + 4

                    // the divider — the house's default 1px rule (§8 idiom 1),
                    // the ledger's own edge. No label: the register course
                    // right below names every register at rest and the stele's
                    // top frame echoes the sounding one, so a third naming
                    // would be the third reading of the same word.
                    Rectangle {
                        y: 4
                        anchors.left: parent.left; anchors.right: parent.right
                        height: 1; color: root.withA(root.sig, 0.3)
                    }

                    // ── THE REGISTER COURSE — five drawn chips ───────────────
                    // The plinth's own control verbatim, one course down. All
                    // five read at rest, which is the whole reason this is a
                    // course of chips and not the top frame's caption alone: a
                    // caption shows the current register and hides the other
                    // four, and identity behind interaction is the failure this
                    // house has already paid for once. 5 * 56 + 4 * 6 = 304 —
                    // the porch's own measure, so the course lines up with the
                    // plinth above it and the rows below it.
                    Row {
                        id: registerRow
                        y: 4 + 1 + 4
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: root.regGap
                        component Reg : Rectangle {
                            id: reg
                            property int index: -1
                            readonly property bool open: root.register === reg.index
                            readonly property color hue: root.registerHue(reg.index)
                            readonly property int count: root.registerRows(reg.index).length
                            width: root.regChipW
                            height: 15
                            radius: 0
                            color: reg.open ? root.withA(reg.hue, 0.16)
                                 : (regMa.containsMouse ? root.withA(reg.hue, 0.10)
                                                        : "transparent")
                            border.width: 1
                            border.color: reg.open ? reg.hue
                                        : root.withA(root.ink,
                                                     regMa.containsMouse ? 0.6 : 0.32)
                            Behavior on color { ColorAnimation { duration: 150 } }
                            Behavior on border.color { ColorAnimation { duration: 150 } }
                            Text {
                                anchors.centerIn: parent
                                text: root.registerWord(reg.index)
                                font.family: root.faceMono; font.pixelSize: 8
                                color: (reg.open || regMa.containsMouse)
                                       ? root.ink : root.withA(root.ink, 0.72)
                            }
                            MouseArea {
                                id: regMa
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.register = reg.index
                            }
                        }
                        Reg { index: 0 }
                        Reg { index: 1 }
                        Reg { index: 2 }
                        Reg { index: 3 }
                        Reg { index: 4 }
                    }

                    // blank manuscript ruling for every slot, filled or not —
                    // the same sparse-state rung the picker now stands on, so
                    // an empty register reads as a ruled ledger awaiting
                    // entries rather than as a void.
                    Column {
                        y: naos.listY
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * naos.inset
                        Repeater {
                            model: root.channelSlots
                            delegate: Item {
                                width: parent.width; height: root.rowH
                                Rectangle {
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    height: 1
                                    color: root.withA(root.ink, 0.13)
                                }
                            }
                        }
                    }

                    // ── the two row species, cross-faded on the register ─────
                    // Two views rather than one polymorphic delegate: the row
                    // shapes genuinely differ on their right-hand side, and a
                    // Loader per row to switch between them would cost more
                    // than a second view that is simply not visible. 150ms
                    // OutCubic, the same crossfade the bay open/close uses —
                    // registers have no material continuity to dramatize.
                    //
                    // Height off the ROW COUNT, never contentHeight
                    // (hazards.md §2), no footer, no scrollbar: past six slots
                    // it wheel-scrolls inside its own clip, the way every
                    // other list in this house pages rather than growing
                    // chrome.
                    ListView {
                        id: channelView
                        y: naos.listY
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * naos.inset
                        height: Math.min(naos.rows.length, root.channelSlots) * root.rowH
                        clip: true
                        model: naos.streams ? naos.rows : []
                        opacity: naos.streams ? 1 : 0
                        visible: opacity > 0.01
                        Behavior on opacity { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
                        boundsBehavior: Flickable.StopAtBounds
                        delegate: ChannelRow {
                            width: channelView.width
                            height: root.rowH
                        }
                    }
                    ListView {
                        id: deviceView
                        y: naos.listY
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * naos.inset
                        height: Math.min(naos.rows.length, root.channelSlots) * root.rowH
                        clip: true
                        model: naos.streams ? [] : naos.rows
                        opacity: naos.streams ? 0 : 1
                        visible: opacity > 0.01
                        Behavior on opacity { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
                        boundsBehavior: Flickable.StopAtBounds
                        delegate: DeviceRow {
                            width: deviceView.width
                            height: root.rowH
                            hue: naos.hue
                            onPicked: function(k) { root.pickInRegister(root.register, k) }
                        }
                    }

                    // the honest empty — the Greek answer-line, glossed. A
                    // mixer with nothing playing says so in the house's own
                    // register rather than opening onto blank marble.
                    Column {
                        x: naos.inset
                        y: naos.listY + (root.channelSlots * root.rowH) / 2 - 16
                        width: parent.width - 2 * naos.inset
                        spacing: 3
                        visible: naos.rows.length === 0
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: naos.streams ? "σιγή" : "οὐδέν"
                            font.family: root.faceSerif; font.pixelSize: 12
                            font.italic: true
                            color: root.withA(root.ink, 0.5)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: root.register === 0 ? "nothing is playing"
                                : root.register === 1 ? "nothing is recording"
                                : root.register === 4 && !root.btAvail ? "no adapter"
                                : "nothing to choose"
                            font.family: root.faceMono; font.pixelSize: 8
                            font.letterSpacing: 1
                            color: root.withA(root.ink, 0.35)
                        }
                    }
                }
            }

            // ── ledger line: kaomoji reads the box · amber detail ───────────
            Item {
                width: parent.width; height: 16
                Rectangle {
                    anchors.top: parent.top
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 1; color: root.withA(root.sig, 0.3)
                }
                Text {
                    anchors.left: parent.left; anchors.bottom: parent.bottom
                    text: root.kaomojiFor()
                    font.pixelSize: 11
                    color: root.withA(root.sig, 0.9)
                }
                Text {
                    id: hint
                    anchors.right: mixerTag.left; anchors.rightMargin: 10
                    anchors.bottom: parent.bottom
                    // The verbs are the same on a shaft and on a channel row,
                    // which is why the naos needed no gesture vocabulary of
                    // its own — this one line still names both.
                    text: "scroll · set   click · mute"
                    font.family: root.faceMono; font.pixelSize: 9
                    color: root.withA(root.ink, 0.5)
                }
                // The mixer door — the one control in this stele that leaves
                // it. A [ token ] in the house's own bracket vocabulary, not a
                // button: the stele has no button anywhere else and does not
                // grow one now.
                Text {
                    id: mixerTag
                    anchors.right: parent.right; anchors.bottom: parent.bottom
                    text: "[ mixer ]"
                    font.family: root.faceMono; font.pixelSize: 10
                    font.underline: mixerMa.containsMouse
                    color: mixerMa.containsMouse ? root.sig : root.withA(root.sig, 0.85)
                    MouseArea {
                        id: mixerMa
                        anchors.fill: parent
                        anchors.margins: -3
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.openMixer()
                    }
                }
            }

            // ── box-drawing bottom frame — closing 𝄂 barline, gold ink ──────
            Item {
                width: parent.width; height: 18
                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    // The closing figure follows what the surface is showing.
                    // Compact it is the two levels; open, that would be a
                    // third reading of the tallies AND of the naos chip, so
                    // it changes to the one number nothing else on the stele
                    // carries — how many channels are hushed.
                    text: mode.expanded
                          ? ("└─┤ " + root.hushedCount + " of " + root.channelCount
                             + " hushed ├")
                          : ("└─┤ out " + (root.outAvail ? (root.outMuted ? "×" : root.outPct) : "—")
                             + " · in " + (root.inAvail ? (root.inMuted ? "×" : root.inPct) : "—") + " ├")
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.ink, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"; font.family: root.faceMusic; font.pixelSize: 16
                    color: root.livery.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter; anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }
        }
    }
}
