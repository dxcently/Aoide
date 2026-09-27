import QtQuick
import Quickshell
import Quickshell.Io

// ── THE LEDGER · a claude.ai usage stele ──────────────────────────────────────
// A live view of how hard the account is leaning on claude.ai — the plan's
// rolling 5-hour window, the weekly caps (overall + per-model), any extra-usage
// credits — over a LOCAL this-machine token/cost estimate. Reads the file the
// `aoide usage` poller writes (state/usage.json, schema §0); the gadget only
// exists while that file does, so it is effectively toggled on by the nix
// option `aoide.usage.enable` (which instantiates the poller and its timer).
// Fifth of the pantheon; it wears the shared marble-stele grammar but stands
// in its OWN order and hue:
//
//   · ORDER      — a LEDGER stele, re-skinned to read as CLAUDE. Its signature is
//                  base09 — in the light sonata scheme a CLAY / kiln-fired orange
//                  (#c06a35), the palette's warmest, most Claude-coral note: the
//                  spark, the inset keyline, the box-drawing frame and the frieze
//                  all run clay, where the Conductor ran gold, the Meters teal, the
//                  Power murex and the Terminals aegean — plainly its own building.
//                  Its frieze is a BEAD-AND-REEL astragal (beads alternating with
//                  paired reels) — a counting-off ornament, fitting a meter of
//                  consumption; distinct from the Conductor's baton-and-downbeat,
//                  the Meters' triglyph, the Power's egg-and-dart.
//   · MUSIC      — the CLAUDE SPARK ❋ (U+274B, a rayed sunburst — the Anthropic
//                  mark) crowns it, in place of the old segno. A gold 𝄂 closes the
//                  score.
//   · TERMINAL   — box-drawing frames the panel; each cap is the dock's shared
//                  [▓▓░░] shade-glyph gauge (LiveryState.ctxBar), tinted clay
//                  and swung to terracotta past 85% (ctxColor) like every other
//                  meter in the house.
//
// GRACEFUL DEGRADE + LIVE-READY: today the poller's `live` block is a bare
// { ok:false, error } stub — so the live section renders a single quiet, dim
// "unavailable" note (NOT a red alarm), never empty/zero gauges, while the LOCAL
// tile (real data) always renders. Every live sub-field is existence-guarded, so
// the day `live.ok` flips true and fiveHour/sevenDay/… arrive, the plan/weekly/
// credit gauges light up with NO further change. Height is content-driven, so the
// degraded panel is compact, not a big empty marble box. All colour flows from
// `livery`; hard corners (radius 0) everywhere.
Item {
    id: gadget

    required property var livery            // palette roles
    property string usagePath: (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/usage.json"

    // Historically injected by the dock (AoidePanel.qml, retired — the dock
    // is now sonata's own `widgets/usage.qml`, a WidgetSlot anchor). Nothing
    // instantiates this component today. null on a bridge-less host — the
    // manual-refresh click null-guards on it and silently no-ops, exactly like
    // Meters/Power which declare no bridge at all. OUTBOUND-only (hazards §5):
    // the only thing ever sent is the `refreshusage` command, via refreshUsage().
    property var bridge: null

    implicitWidth: 360   // fallback only; the dock stack sets width = root.gadgetW (360)
    // content-driven height: the frame hugs whatever the sections need, so the
    // degraded (live-stub) state is a short panel, not an empty box.
    implicitHeight: body.implicitHeight + 26

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif:  "Noto Serif"               // carved marble
    readonly property string faceMono:   "JetBrainsMono Nerd Font"  // the terminal
    readonly property string faceMusic:  "Noto Music"               // notation
    readonly property string faceSymbol: "Noto Sans Symbols 2"      // the Claude spark ❋

    // this temple's signature accent — base09, its own among the pantheon. In the
    // LIGHT sonata scheme base09 resolves to #c06a35, a "clay / kiln-fired orange":
    // already the palette's warmest, most Claude-coral note that does NOT collide
    // with urgent (base08 #b0472f terracotta, which ctxColor swings to past 85%).
    // So the Claude re-skin stays palette-driven on base09 — no off-palette coral.
    // (A literal Claude clay ≈ #d97757 is noted for the vision-check, not applied.)
    readonly property color signature: livery.base09

    readonly property int cells: 14
    readonly property real urgentAt: 85

    property real nowMs: Date.now()

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── PARSED USAGE ────────────────────────────────────────────────────────────
    // The whole document, or {} when the file is missing/empty/garbage. hasData
    // gates the ENTIRE gadget's visibility (historically read by AoidePanel;
    // sonata's `widgets/usage.qml` reads its own `implicitHeight` instead)
    // so a host without the
    // poller enabled shows nothing at all.
    property var usage: ({})
    // TICK HOOK — `usage` is reassigned to a FRESH object (JSON.parse always
    // allocates a new one) at the end of every parseUsage() call, and
    // parseUsage() only ever runs from usageFile.onLoaded (itself fired by
    // FileView's watchChanges → onFileChanged → reload()) or
    // Component.onCompleted's initial read. So onUsageChanged fires exactly
    // once per aoide poller write (or parse attempt) — never on an idle
    // frame — which makes it the one signal that means "aoide just pushed
    // fresh data", as opposed to e.g. the 30s nowMs countdown timer below,
    // which fires on a clock regardless of whether the file changed at all.
    // clef (the ❋ spark, declared further down) listens here for its tick.
    onUsageChanged: {
        clef.tick()                              // arrival heartbeat (fires once per poller write)
        // A landed write satisfies a pending manual refresh: clear the cooldown
        // early so the spark is clickable again the instant fresh data shows —
        // "until the next onUsageChanged or refreshCooldownMs, whichever first".
        if (gadget.refreshPending) {
            gadget.refreshPending = false
            refreshCooldown.stop()
        }
        clef.fetchDone = true
        clef.maybeStop()
    }
    readonly property bool hasData: !!(usage && (usage.local || usage.live))

    // ── MANUAL REFRESH — click the ❋ spark to demand a fresh `aoide usage` ────
    // The spark is already the data heartbeat (it ticks on every poller write,
    // above); clicking the heartbeat asks for a beat. requestRefresh sends the
    // OUTBOUND-only `refreshusage` command (bridge.refreshUsage → shellbridge →
    // `aoide usage` → state/usage.json write → the FileView watch → onUsageChanged
    // → clef.tick()), so SUCCESS feedback is the SAME arrival spin a timed poll
    // draws — no second success animation. clef.acknowledge() covers only the gap
    // between click and that write with a quiet press-dip, so the click never
    // feels dead. refreshPending is a cooldown: nothing downstream dedupes the
    // command, so without it a mash would stack redundant ~15s `aoide usage` runs on
    // the daemon — it swallows repeat clicks until the next real write
    // (onUsageChanged) or refreshCooldownMs, whichever comes first.
    readonly property int refreshCooldownMs: 10000
    property bool refreshPending: false
    // Hover discoverability cue — set by either click-to-refresh MouseArea
    // (the ❋ spark and the live-section overlay, both below). Text-only, no
    // animation: the spinner is reserved for real work (refreshPending).
    property bool hoverRefresh: false

    // The real Claude Code status-word list — extracted verbatim (strings on
    // the installed claude-code binary) rather than invented, per khoa: "just
    // copy the code over". One word is drawn per click and held for that
    // whole spin (see requestRefresh/spinWord below), same as the real CLI
    // picks one word per turn rather than cycling through several.
    readonly property var spinWords: [
        "Accomplishing", "Actioning", "Actualizing", "Architecting", "Baking",
        "Beaming", "Beboppin'", "Befuddling", "Billowing", "Blanching",
        "Bloviating", "Boogieing", "Boondoggling", "Booping", "Bootstrapping",
        "Brewing", "Bunning", "Burrowing", "Calculating", "Canoodling",
        "Caramelizing", "Cascading", "Catapulting", "Cerebrating",
        "Channeling", "Channelling", "Choreographing", "Churning", "Clauding",
        "Coalescing", "Cogitating", "Combobulating", "Composing", "Computing",
        "Concocting", "Considering", "Contemplating", "Cooking", "Crafting",
        "Creating", "Crunching", "Crystallizing", "Cultivating", "Deciphering",
        "Deliberating", "Determining", "Dilly-dallying", "Discombobulating",
        "Doing", "Doodling", "Drizzling", "Ebbing", "Effecting", "Elucidating",
        "Embellishing", "Enchanting", "Envisioning", "Fermenting",
        "Fiddle-faddling", "Finagling", "Flambéing", "Flibbertigibbeting",
        "Flowing", "Flummoxing", "Fluttering", "Forging", "Forming",
        "Frolicking", "Frosting", "Gallivanting", "Galloping", "Garnishing",
        "Generating", "Gesticulating", "Germinating", "Gitifying", "Grooving",
        "Gusting", "Harmonizing", "Hashing", "Hatching", "Herding", "Honking",
        "Hullaballooing", "Hyperspacing", "Ideating", "Imagining",
        "Improvising", "Incubating", "Inferring", "Infusing", "Ionizing",
        "Jitterbugging", "Julienning", "Kneading", "Leavening", "Levitating",
        "Lollygagging", "Manifesting", "Marinating", "Meandering",
        "Metamorphosing", "Misting", "Moonwalking", "Moseying", "Mulling",
        "Mustering", "Musing", "Nebulizing", "Nesting", "Newspapering",
        "Noodling", "Nucleating", "Orbiting", "Orchestrating", "Osmosing",
        "Perambulating", "Percolating", "Perusing", "Philosophising",
        "Photosynthesizing", "Pollinating", "Pondering", "Pontificating",
        "Pouncing", "Precipitating", "Prestidigitating", "Processing",
        "Proofing", "Propagating", "Puttering", "Puzzling", "Quantumizing",
        "Razzle-dazzling", "Razzmatazzing", "Recombobulating", "Reticulating",
        "Roosting", "Ruminating", "Sautéing", "Scampering", "Schlepping",
        "Scurrying", "Seasoning", "Shenaniganing", "Shimmying", "Simmering",
        "Skedaddling", "Sketching", "Slithering", "Smooshing", "Sock-hopping",
        "Spelunking", "Spinning", "Sprouting", "Stewing", "Sublimating",
        "Swirling", "Swooping", "Symbioting", "Synthesizing", "Tempering",
        "Thinking", "Thundering", "Tinkering", "Tomfoolering",
        "Topsy-turvying", "Transfiguring", "Transmuting", "Twisting",
        "Undulating", "Unfurling", "Unravelling", "Vibing", "Waddling",
        "Wandering", "Warping", "Whatchamacalliting", "Whirlpooling",
        "Whirring", "Whisking", "Wibbling", "Working", "Wrangling", "Zesting",
        "Zigzagging"
    ]
    property string spinWord: ""
    function requestRefresh() {
        if (gadget.refreshPending) return          // in cooldown → ignore the click entirely
        // No bridge → the quiet press-dip is the ONLY feedback we can give (nothing
        // to send, silent degrade). WITH a bridge we deliberately SKIP the dip: the
        // spin below starts on this very click and IS the feedback, and any scale
        // motion here would make the spin read differently from Conductor's hookTag
        // (which never scales) — khoa: the two spinners must look identical mid-spin.
        if (!gadget.bridge) { clef.acknowledge(); return }
        gadget.refreshPending = true                // engage cooldown (a real request is going out)
        refreshCooldown.restart()
        gadget.bridge.refreshUsage()
        // One word for the whole spin, drawn now and held (not rerolled
        // every frame) — matches the real CLI picking one word per turn.
        gadget.spinWord = gadget.spinWords[Math.floor(Math.random() * gadget.spinWords.length)]
        // Start the loop fresh — reset even though activeSpin should
        // already be false here (the cooldown guard above blocks re-entry
        // while a previous cycle is in flight), so a stray leftover frame
        // can never be mistaken for an instant "already looped".
        clef.activeSpin = true
        clef.fetchDone = false
        clef.loopedOnce = false
        spinFrame.frame = -1
    }
    // Fallback release: if NO write ever lands (daemon down, the command unknown to
    // an un-rebuilt daemon, or the fetch itself failing), clear the cooldown
    // after refreshCooldownMs so the spark never stays stuck un-clickable.
    Timer {
        id: refreshCooldown
        interval: gadget.refreshCooldownMs
        onTriggered: {
            gadget.refreshPending = false
            clef.fetchDone = true
            clef.maybeStop()
        }
    }

    // ── STALE-DATA note — fetchedAt wired to exactly this, nothing else ─────────
    // fetchedAt is the WHOLE document's write stamp (local + live share one poll,
    // one timestamp), so staleness is checked independent of liveOk — a poller
    // that's gone quiet is stale whether or not its last live fetch happened to
    // succeed. Threshold is 3x the poller's own cadence (aoide.usage.interval,
    // default "300s" — modules/nucleus/options.nix): missing three ticks in a row
    // is plainly "not refreshing", not a single missed beat. Mirrored as a literal
    // constant rather than read live off the livery file — flag: if that Nix default
    // ever gets retuned, this drifts with it (a one-line livery file field would fix
    // that properly; not worth it for a single dim caveat).
    readonly property real pollCadenceMs: 300 * 1000
    readonly property real staleAfterMs: pollCadenceMs * 3
    readonly property real fetchedAtMs: {
        var v = (usage && usage.fetchedAt) ? Date.parse(usage.fetchedAt) : NaN
        return isNaN(v) ? -1 : v
    }
    readonly property bool dataStale:
        fetchedAtMs > 0 && (gadget.nowMs - fetchedAtMs) > staleAfterMs
    // "aug 2" — lowercase, matching the gadget's own lowercase note voice
    // (localNote / "live usage unavailable"); year only when it isn't this one.
    function staleDateLabel(ms) {
        var months = ["jan","feb","mar","apr","may","jun","jul","aug","sep","oct","nov","dec"]
        var d = new Date(ms), now = new Date(gadget.nowMs)
        var s = months[d.getMonth()] + " " + d.getDate()
        return d.getFullYear() === now.getFullYear() ? s : s + " " + d.getFullYear()
    }

    // the live block, and its ok flag — every field below is guarded off THIS,
    // so the section is inert (a note only) until the day `ok` turns true.
    readonly property var live: (usage && usage.live) ? usage.live : ({})
    readonly property bool liveOk: live && live.ok === true
    readonly property string liveError: (live && live.error) ? live.error : "live fetch not enabled"

    // guarded live sub-blocks — null unless liveOk AND the field is actually there
    readonly property var fiveHour:  (liveOk && live.fiveHour)       ? live.fiveHour       : null
    readonly property var sevenDay:  (liveOk && live.sevenDay)       ? live.sevenDay       : null
    readonly property var opusWk:    (liveOk && live.sevenDayOpus)   ? live.sevenDayOpus   : null
    readonly property var sonnetWk:  (liveOk && live.sevenDaySonnet) ? live.sevenDaySonnet : null
    readonly property var extra:     (liveOk && live.extraUsage)     ? live.extraUsage     : null
    readonly property bool creditsOn: !!(extra && extra.isEnabled === true)

    // the local estimate (real data now) — null when the block is absent
    readonly property var local:     (usage && usage.local)         ? usage.local         : null
    readonly property var localToday: (local && local.today)        ? local.today         : null
    readonly property var localWeek:  (local && local.week)         ? local.week          : null
    readonly property string localNote: (local && local.note) ? local.note : "local estimate, this machine only"

    function utilOf(block) { return (block && block.utilization !== undefined) ? block.utilization : 0 }
    function money(v) { return (v === undefined || v === null) ? "—" : "$" + Number(v).toFixed(2) }

    // ── FILE: the usage document, hot-reloaded on change ────────────────────────
    function parseUsage() {
        try {
            var t = usageFile.text()
            if (!t || t.trim().length === 0) { gadget.usage = ({}); return }
            gadget.usage = JSON.parse(t)
        } catch (e) {
            gadget.usage = ({})
        }
    }
    FileView {
        id: usageFile
        path: gadget.usagePath
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: gadget.parseUsage()
        onFileChanged: reload()
    }
    // ticks the reset countdowns without a re-read
    Timer { interval: 30000; running: gadget.liveOk; repeat: true; onTriggered: gadget.nowMs = Date.now() }
    Component.onCompleted: parseUsage()

    // cast shadow — lifts the stele off any marble wallpaper ─────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: gadget.withA(livery.paletteFg, 0.22)
    }

    // the stele ──────────────────────────────────────────────────────────────────
    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: livery.paletteBg
        border.color: livery.paletteFg
        border.width: 2

        Rectangle {                                   // inset keyline — CLAY (Claude)
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: gadget.signature; border.width: 1
        }

        // ── reusable utilization cap — the shared [▓▓░░] gauge ───────────────────
        component UsageMeter : Item {
            id: um
            property string label: ""
            property var block: null            // { utilization, resetsAt }
            property int barCells: gadget.cells
            property bool compact: false
            readonly property bool present: !!(block && block.utilization !== undefined)
            readonly property real util: gadget.utilOf(block)
            readonly property bool urgent: util >= gadget.urgentAt
            width: parent ? parent.width : 0
            visible: present
            // Row height copies MetersGadget's stacked Gauge geometry (its
            // label-over-bar row is h46: label line, ~5px air, an 18px bar
            // box, 2px bottom margin). At the old 34/26 the two line boxes
            // OVERLAPPED — serif label ≈19px + mono bar ≈18.5px + 2 margin
            // ≈ 39.5 > 34 — and the ▓░ shade glyphs ink their full cell, so
            // the bar's top rows sat above the label's baseline: the
            // "crowded" PLAN/WEEKLY read. 44/36 restores Meters' air.
            height: present ? (compact ? 36 : 44) : 0

            Text {                                    // section label (serif)
                id: umName
                anchors.left: parent.left
                anchors.top: parent.top
                text: um.label
                font.family: gadget.faceSerif
                font.pixelSize: um.compact ? 12 : 14
                font.weight: Font.Medium; font.letterSpacing: 2
                color: livery.paletteFg
            }
            Text {                                    // util %, tallied in gold / terracotta
                id: umPct
                anchors.right: parent.right
                anchors.baseline: umName.baseline
                text: Math.round(um.util) + "%"
                font.family: gadget.faceMono; font.pixelSize: um.compact ? 12 : 15
                color: um.urgent ? livery.paletteUrgent : livery.paletteAccent
            }
            Text {                                    // reset countdown (small, dim, italic)
                anchors.left: umName.right; anchors.leftMargin: 8
                anchors.right: umPct.left; anchors.rightMargin: 6   // bounded — never bleeds under the %
                anchors.baseline: umName.baseline
                text: gadget.livery.usageResetIn(um.block ? um.block.resetsAt : "", gadget.nowMs)
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignLeft
                font.family: gadget.faceSerif; font.italic: true
                font.pixelSize: 10
                color: gadget.withA(livery.paletteFg, 0.5)
            }
            Text {                                    // the shared shade-glyph gauge
                anchors.left: parent.left; anchors.right: parent.right
                anchors.bottom: parent.bottom; anchors.bottomMargin: um.compact ? 0 : 2
                text: gadget.livery.ctxBar(um.util, um.barCells)
                font.family: gadget.faceMono; font.pixelSize: um.compact ? 12 : 14
                fontSizeMode: Text.HorizontalFit
                horizontalAlignment: Text.AlignLeft
                color: gadget.livery.ctxColor(um.util, gadget.signature)
            }
        }

        // ── CONTENT — a top-down score; the frame hugs it (content-driven height) ─
        Column {
            id: body
            anchors.left: parent.left; anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 13
            spacing: 8

            // ── ENTABLATURE: Claude spark · CLAUDE · [ claude.ai ] ───────────────
            Item {
                id: head
                width: parent.width
                height: 38

                Rectangle {                           // deeper marble band
                    anchors.fill: parent; anchors.bottomMargin: 5
                    color: gadget.withA(livery.paletteFg, 0.05)
                }
                Text {                                // the Claude spark — a rayed
                    id: clef                          // sunburst, crowning the stele
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    // Fixed width + centered glyph — spinGlyphs vary in
                    // natural width at this size ("⋅" vs "✳" etc.), and
                    // CLAUDE (anchored to clef.right, below) would jitter
                    // sideways each frame without a stable box to anchor
                    // against (khoa: "keep it exactly in the same position").
                    width: 32
                    horizontalAlignment: Text.AlignHCenter
                    // Static "❋" (U+274B) at rest; cycles the same spinner
                    // glyphs/timing as Conductor's hookTag (ConductorGadget.qml)
                    // — click-triggered only (khoa: "only play while claude
                    // is working", hover never starts it). activeSpin is
                    // purely imperative, never bound (hazards §3): set true
                    // by gadget.requestRefresh() the moment a real request
                    // goes out, cleared by maybeStop() only once BOTH the
                    // fetch has concluded (fetchDone, set from
                    // onUsageChanged / refreshCooldown) AND a full lap has
                    // played (loopedOnce, set from spinFrame) — a fast
                    // response still gets to finish the loop it started
                    // (khoa: "should play 1 whole loop first").
                    readonly property var spinGlyphs: ["·", "✻", "✽", "✶", "✳", "✢"]
                    property bool activeSpin: false
                    property bool fetchDone: false
                    property bool loopedOnce: false
                    function maybeStop() {
                        if (clef.activeSpin && clef.fetchDone && clef.loopedOnce)
                            clef.activeSpin = false
                    }
                    text: clef.activeSpin ? (clef.spinGlyphs[spinFrame.frame] || "❋") : "❋"
                    // Per-frame font, mirroring hookTag's own faceSymbol/faceMono
                    // ternary. The dot frame (spinGlyphs[0], "·" U+00B7 MIDDLE DOT)
                    // is NOT in "Noto Sans Symbols 2" — the font's charset lacks it
                    // and its .notdef for it is blank, so that one frame renders as
                    // an invisible gap (or a Qt fallback gamble): glaring in THIS
                    // 26px spark, imperceptible in hookTag's 10px. The five stars
                    // ✻✽✶✳✢ and the resting ❋ ARE in NSS2, so they keep faceSymbol;
                    // only the dot swaps to faceMono, which HAS a well-centred U+00B7
                    // (measured: its ink centre lands inside the stars' own vertical
                    // band at this size — no Y-offset needed, only the right font).
                    // Glyph array + hold-first/last timing stay identical to hookTag
                    // (COPY exactly); only the per-frame font is corrected here.
                    font.family: text === "·" ? gadget.faceMono : gadget.faceSymbol
                    font.pixelSize: 26   // U+274B — the Claude mark
                    color: gadget.signature
                    transformOrigin: Item.Center
                    scale: 1.0
                    // A spin means "no scale motion", exactly like hookTag: the instant
                    // a spin begins, kill any tick/ack pulse still in flight from a
                    // just-prior poll and pin scale at rest for the whole lap. Paired
                    // with tick()'s activeSpin guard (no NEW pulse starts mid-spin) and
                    // requestRefresh dropping the ack-dip on the spin path, this
                    // guarantees scale stays 1.0 start-to-finish of every spin.
                    // opacity is pinned the same way: the idle shimmer below stops on
                    // activeSpin (its `running` flips false) but a stopped animation
                    // LEAVES the property at its mid-flight value — so without this
                    // write every spin played at a random frozen 0.72..1.0, visibly
                    // dimmer than hookTag (which has no element-opacity motion, ever)
                    // and different spin to spin; at 0.72 the faint "·" frame all but
                    // vanished, reading as the dot's font override being broken.
                    onActiveSpinChanged: if (activeSpin) { tickPulse.stop(); ackPulse.stop(); scale = 1.0; opacity = 1.0 }

                    Timer {
                        id: spinFrame
                        interval: baseIntervalMs
                        repeat: true
                        triggeredOnStart: true
                        running: clef.activeSpin
                        property int frame: -1
                        // Matches Conductor's hookSpin exactly (khoa: "it
                        // should use the conductors claude animation") —
                        // hold-first/last timing, same glyph order.
                        readonly property int baseIntervalMs: 170
                        readonly property int holdIntervalMs: 300
                        onTriggered: {
                            // wrapped: true exactly when THIS tick advances
                            // past the last glyph back to 0 — a genuine lap,
                            // not the very first tick (frame starts at -1,
                            // never at length-1, so it can't be mistaken
                            // for one).
                            var wrapped = frame === clef.spinGlyphs.length - 1
                            frame = (frame + 1) % clef.spinGlyphs.length
                            if (wrapped) { clef.loopedOnce = true; clef.maybeStop() }
                            interval = (frame === 0 || frame === clef.spinGlyphs.length - 1)
                                       ? holdIntervalMs : baseIntervalMs
                        }
                    }

                    // TICK — a scale pulse, fired once per aoide poller write
                    // (see gadget.onUsageChanged above): the spark's MOTION is
                    // a live-update heartbeat, distinct from the slow idle
                    // shimmer below which just keeps it from reading as inert
                    // between pushes. No rotation — the refreshPending glyph-
                    // cycle above is the "working" language now; this is only
                    // the arrival beat.
                    function tick() {
                        // While a manual-refresh spin is in flight the spin ITSELF is
                        // the arrival feedback; a scale pop on top would break the match
                        // with Conductor's hookTag (which has no scale motion), so the
                        // heartbeat pulse is suppressed until the spin ends. onUsageChanged
                        // still sets fetchDone + maybeStop() AROUND this call, so the state
                        // machine that ends the spin is untouched. Normal (timed-poll)
                        // heartbeats — when not spinning — still pulse exactly as before.
                        if (clef.activeSpin) return
                        ackPulse.stop()   // data arrived: the tick owns `scale` now (hazards §3)
                        tickPulse.restart()
                    }
                    // ACKNOWLEDGE — a quick, quiet press-dip fired on a manual
                    // refresh CLICK (gadget.requestRefresh), bridging the gap
                    // until the daemon's write lands and the louder tick() takes
                    // over. Deliberately smaller and shorter than tick's upward
                    // pulse: a dip to 0.9 and back, no spin — a click ack, not a
                    // heartbeat. Stops tickPulse first so only ONE animation ever
                    // writes `scale` at a time (hazards §3: two on one property fight).
                    function acknowledge() {
                        tickPulse.stop()
                        ackPulse.restart()
                    }
                    SequentialAnimation {
                        id: tickPulse
                        NumberAnimation { target: clef; property: "scale"; to: 1.12; duration: 260; easing.type: Easing.OutCubic }
                        NumberAnimation { target: clef; property: "scale"; to: 1.0;  duration: 340; easing.type: Easing.InCubic }
                    }
                    SequentialAnimation {
                        id: ackPulse
                        NumberAnimation { target: clef; property: "scale"; to: 0.9; duration: 110; easing.type: Easing.OutCubic }
                        NumberAnimation { target: clef; property: "scale"; to: 1.0; duration: 200; easing.type: Easing.OutBack }
                    }

                    // idle — a slow, subtle opacity shimmer so the spark
                    // never sits fully static between pushes; deliberately
                    // much quieter than the tick, so the tick still reads
                    // as the distinct "just updated" event. Paused during
                    // activeSpin (khoa: match Conductor's hookTag exactly —
                    // that one only ever swaps text, no opacity motion, so
                    // the two should look identical mid-spin). Pausing alone
                    // is HALF the match: stopping freezes opacity mid-value,
                    // so onActiveSpinChanged above also pins it back to 1.0
                    // — hookTag spins at constant full opacity, so must this.
                    // On spin end `running` flips true again and the loop
                    // eases from 1.0 back into its cycle — no snap.
                    SequentialAnimation on opacity {
                        running: !clef.activeSpin
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.72; duration: 1800; easing.type: Easing.InOutSine }
                        NumberAnimation { to: 1.0;  duration: 1800; easing.type: Easing.InOutSine }
                    }
                }
                Text {
                    anchors.left: clef.right; anchors.leftMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: "CLAUDE"
                    font.family: gadget.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: livery.paletteFg
                }
                Text {                                // the source — swaps to the
                                                        // hover cue or the spin word,
                                                        // same reserved right-anchored
                                                        // slot (activeSpin wins over
                                                        // hoverRefresh: the mouse is
                                                        // usually still over the spark
                                                        // right as a spin starts)
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: clef.activeSpin ? ("[ " + gadget.spinWord + "… ]")
                          : (gadget.hoverRefresh ? "[ click to refresh ]" : "[ claude.ai ]")
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }

                // ── the ❋ spark IS the refresh button ────────────────────────
                // No new chrome/glyph (nothing to font-verify, hazards §1): the
                // heartbeat glyph doubles as the affordance, house grammar (the
                // bar's clef→powermenu, mode-cell→toggleRiceMode click precedents).
                // A ~32px hit area over the 26px glyph; the glyph itself is
                // untouched. Last child of `head` so it's top-most exactly here.
                MouseArea {
                    anchors.centerIn: clef
                    width: 32; height: 32
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onEntered: gadget.hoverRefresh = true
                    onExited: gadget.hoverRefresh = false
                    onClicked: gadget.requestRefresh()
                }
            }

            // ── bead-and-reel astragal frieze, in clay ──────────────────────────
            Canvas {
                id: frieze
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = gadget.signature
                    ctx.lineWidth = 1.4
                    var u = height / 5
                    var top = u * 0.5, bot = height - u * 0.5, mid = (top + bot) / 2
                    ctx.beginPath()                    // the two rails
                    ctx.moveTo(0, top); ctx.lineTo(width, top)
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot)
                    ctx.stroke()
                    // bead radius: span/2 − 3, NOT − 1.5 — at −1.5 the bead ink
                    // (centre ± r ± half the 1.4 stroke) ended 0.1px from the
                    // rails' inner ink edge: sub-pixel, so antialiasing welded
                    // beads to rails and the band smeared into the ┌─ frame
                    // line below (khoa's "overlapping rows"). An astragal's
                    // beads FLOAT between the fillets — unlike Meters'
                    // triglyphs, whose verticals rightly span rail to rail.
                    // −3 leaves 1.6px of true air each side; reels inset 2.5
                    // (was 1, which OVERLAPPED rail ink by 0.4px) for 1.8px.
                    var period = height * 1.1, r = (bot - top) / 2 - 3
                    for (var x = 0; x < width; x += period) {
                        var bx = x + period * 0.3       // the bead (a circle)
                        ctx.beginPath()
                        ctx.arc(bx, mid, r, 0, 2 * Math.PI)
                        ctx.stroke()
                        var rx = x + period * 0.75      // the reel (paired short bars)
                        ctx.beginPath()
                        ctx.moveTo(rx - 1.5, top + 2.5); ctx.lineTo(rx - 1.5, bot - 2.5)
                        ctx.moveTo(rx + 1.5, top + 2.5); ctx.lineTo(rx + 1.5, bot - 2.5)
                        ctx.stroke()
                    }
                }
            }

            // ── box-drawing top frame: ┌─┤ ♪ usage ├────┐ ────────────────────────
            Item {
                id: topFrame
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ usage ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }

            // ── LIVE SECTION — gauges when ok, a quiet note otherwise ────────────
            // Wrapped in a plain Item (not just a MouseArea sibling) because
            // Column positions its own children's y directly — a MouseArea
            // child with anchors.fill fights that. The wrapper takes
            // liveSection's old slot in the outer body Column (khoa:
            // clicking the live usage section should refresh too, not just
            // the ❋ spark).
            Item {
                id: liveSectionWrap
                width: parent.width
                height: liveSection.height
            Column {
                id: liveSection
                width: parent.width
                spacing: 8

                // ok: the plan / weekly / per-model caps + credits (each self-hides
                // when its block is absent, so partial live payloads degrade too)
                UsageMeter { label: "PLAN · 5-HOUR"; block: gadget.fiveHour }
                UsageMeter { label: "WEEKLY";        block: gadget.sevenDay }
                UsageMeter { label: "OPUS · WK";     block: gadget.opusWk;   barCells: 10; compact: true }
                UsageMeter { label: "SONNET · WK";   block: gadget.sonnetWk; barCells: 10; compact: true }

                Item {                                // extra-usage credits, when enabled
                    width: parent.width; height: gadget.creditsOn ? 16 : 0
                    visible: gadget.creditsOn
                    Text {
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: "credits"
                        font.family: gadget.faceSerif; font.pixelSize: 12
                        font.letterSpacing: 2; color: livery.paletteFg
                    }
                    Text {
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: gadget.extra
                              ? (gadget.money(gadget.extra.usedCredits)
                                 + " / " + gadget.money(gadget.extra.monthlyLimit))
                              : ""
                        font.family: gadget.faceMono; font.pixelSize: 11
                        color: gadget.withA(gadget.signature, 0.95)
                    }
                }

                // NOT ok (today's reality): one dim, quiet note — NOT a red alarm,
                // and NEVER an empty/zero gauge. Lights out entirely the day live
                // turns on (the gauges above take over with no other change).
                Item {
                    width: parent.width
                    visible: !gadget.liveOk
                    height: visible ? 40 : 0
                    Column {
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3
                        Text {
                            text: "live usage unavailable"
                            font.family: gadget.faceSerif; font.italic: true
                            font.pixelSize: 12
                            color: gadget.withA(livery.paletteFg, 0.5)
                        }
                        Text {
                            width: parent.width
                            text: "— " + gadget.liveError + " · enable the live fetch"
                            elide: Text.ElideRight
                            font.family: gadget.faceMono; font.pixelSize: 10
                            color: gadget.withA(livery.paletteFg, 0.38)
                        }
                    }
                }
            }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onEntered: gadget.hoverRefresh = true
                    onExited: gadget.hoverRefresh = false
                    onClicked: gadget.requestRefresh()
                }
            }

            // ── hairline rule between the live headline and the local ledger ─────
            Rectangle {
                width: parent.width; height: 1
                color: gadget.withA(gadget.signature, 0.30)
            }

            // ── LOCAL TILE — this-machine estimate (real data now) ───────────────
            Column {
                id: localTile
                width: parent.width
                spacing: 4

                // a ledger row — label · tokens · ~$cost
                component LocalRow : Item {
                    property string tag: ""
                    property var span: null
                    width: parent ? parent.width : 0
                    height: 18
                    Text {
                        id: lrName
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: tag
                        font.family: gadget.faceSerif; font.pixelSize: 13
                        font.weight: Font.Medium; font.letterSpacing: 2
                        color: livery.paletteFg
                    }
                    Text {                            // ~$cost, tallied in gold
                        id: lrCost
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: span ? ("~" + gadget.money(span.costUsd)) : "—"
                        font.family: gadget.faceMono; font.pixelSize: 12
                        color: livery.paletteAccent
                    }
                    Text {                            // token count, compacted (686M …)
                        anchors.right: lrCost.left; anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: span ? (gadget.livery.ctxCompact(span.tokens) + " tok") : ""
                        font.family: gadget.faceMono; font.pixelSize: 11
                        color: gadget.withA(gadget.signature, 0.9)
                    }
                }

                LocalRow { tag: "TODAY";     span: gadget.localToday }
                LocalRow { tag: "THIS WEEK"; span: gadget.localWeek }

                Text {                                // the honest caveat — small + dim
                    width: parent.width
                    text: gadget.localNote
                    elide: Text.ElideRight
                    font.family: gadget.faceSerif; font.italic: true
                    font.pixelSize: 10
                    color: gadget.withA(livery.paletteFg, 0.45)
                }
            }

            // ── box-drawing bottom frame: └─┤ summary ├── 𝄂 ┘ ───────────────────
            Item {
                id: footer
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + (gadget.liveOk && gadget.fiveHour
                                    ? ("5h " + Math.round(gadget.utilOf(gadget.fiveHour)) + "%"
                                       + (gadget.sevenDay ? " · wk " + Math.round(gadget.utilOf(gadget.sevenDay)) + "%" : ""))
                                    : "local est · this machine") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(livery.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {                                // final barline — gold ink (shared)
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: gadget.faceMusic; font.pixelSize: 18
                    color: livery.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }
        }
    }
}
