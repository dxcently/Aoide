// WidgetPreview.qml — the widget design canvas: load ONE slot body at any
// size, on any viewport, against a fixture roster, with every action inert.
//
//   lyra preview [<widget>] …            builds the isolated root and launches this
//   env -u QS_STAGE AOIDE_ROOT=<root> AOIDE_STATE_DIR=<root>/state \
//     AOIDE_STAGE_DIR=<root>/state/stage qs -p <root>/run/qml/WidgetPreview.qml
//
// The name ends in `Preview.qml` ON PURPOSE: `rice mode stage` reaps every
// `qs -p *Preview.qml` harness by that suffix, so the canvas is torn down by
// the same convention that reaps the other seven harnesses.
//
// ── Why a canvas and not another fixed-size *Preview.qml ────────────────────
// ConductorPreview/TerminalsPreview/DockPreview each pin ONE widget into ONE
// 360x520 PanelWindow with a stub palette. That answers "does it still
// render"; it cannot answer "what does it look like at 240 on a portrait
// screen, anchored bottom-right, against sixteen sessions". This file is the
// same loading mechanism (Qt.createComponent + createObject — WidgetSlot's,
// because a plain `Loader { source }` cannot satisfy a `required property`)
// with the size, anchor, viewport, zoom, background and fixture set turned
// into controls, and the whole thing in a FloatingWindow — a real Hyprland
// client (class org.quickshell), so `lyra screen shot --window <addr>`
// captures it and no layer surface is created.
//
// ── Camera and toolbar ─────────────────────────────────────────────────────
// `viewport` sits at (camX, camY) inside a clipping frame at `effScale`; the
// widget and its annotation overlay are both its children, so ONE transform
// carries them and a pointer position in the overlay is already widget-local
// — a note written after a pan+zoom lands where it was clicked. Middle-drag
// pans; Space (held, outside a text field or a Tab-focused control) makes
// left-drag pan too and shows the hand, masking — never changing — the
// annotation tool in use; Ctrl+wheel
// zooms about the pointer; plain wheel reaches the widget. The toolbar's
// glyphs are the facet's `lyra icon resolve` output (`modules/facets/
// quickshell/icons/`), copied by `lyra preview` to `run/qml/icons/` beside
// this file and tinted from the chrome — no network, no icon font.
//
// ── Isolation: one env var, no socket ──────────────────────────────────────
// Every read a widget performs is already rooted at $AOIDE_ROOT — LiveryState
// (song/stage/livery.json), conductor.qml (state/stage/ via its own stageDir),
// terminals.qml (`stagePath`), SessionMenu (AOIDE_STATE_DIR), StagingEngine
// (run/qml/songs/manifest.json) — so pointing AOIDE_ROOT at a fixture root
// isolates every read with nothing else to configure. `QS_STAGE` must be
// UNSET or it overrides conductor's stage dir back out of the root.
// Writes are isolated by construction: this file never instantiates
// ShellBridge and opens no socket. Every action a widget fires lands in the
// stub bridge below, which only appends to the on-screen log. Nothing here
// can send, kill, focus or recheck a real session. The ONE process this
// canvas ever starts is `lyra preview set` (fixture/livery, below) — file
// copies into the preview root, which is the shell's job, not QML's.
//
// ── Empirical answers this file is built on (Quickshell 0.3.1 / Qt 6.11, probed) ─
//  · FileView write API: `setText(string)`. `atomicWrites` defaults TRUE and
//    `blockWrites` defaults FALSE; the write is acknowledged by the `saved`
//    signal, and a `reload()` afterwards reads back exactly what was written.
//    (`writeAdapter()` is the OTHER write path — it serialises a JsonAdapter
//    child instead of raw text; not used here because the control document
//    must preserve keys this canvas does not know about.)
//  · createObject with an initial property the component does NOT declare:
//    Qt warns once per unknown key ("… does not have a property called X")
//    and STILL CREATES the object, with every declared key applied. So there
//    is no slot→extras table here — one universal prop set is passed to every
//    widget, and the cost of a wrong guess is a log line, not a null item.
//    So loading conductor.qml prints six "does not have a property called
//    clipboard/dock/ledger/powermenu/stagePath/stagingEngine" warnings and
//    loading terminals five: that is the universal set being offered and
//    declined, EXPECTED NOISE, not an error. The error pane is the place a
//    real failure shows.
//  · An INLINE component ("component RailButton: Rectangle { … }") CAN read
//    the enclosing root's ids and properties — probed, contrary to what the
//    inline-component scope warning suggests. That is what lets every rail
//    control read the livery-derived chrome tokens directly instead of having
//    them re-passed at thirty call sites.
//  · Quickshell 0.3.1 does NOT reload when a file loaded through
//    `Qt.createComponent` changes on disk — not by default, and not with
//    `Quickshell.watchFiles = true` either (both probed: its watcher covers
//    the config's STATIC import graph, and a dynamically resolved slot body
//    is not in it). Auto-reload is therefore explicit here: FileViews over
//    preview.json's `watch` list, debounced into `requestReload()`: each
//    watched checkout file is copied over its run/qml copy (`stage`), then
//    `Quickshell.reload(false)`.
//  · An object held in a PROPERTY ("property FileView controlFile: FileView
//    { … }") completes AFTER the root that declares it — the opposite of a
//    default-list child. So preview.json is still unread when the root's
//    Component.onCompleted runs, and the first widget build cannot happen
//    there: see the `built` latch.
//  · Qt caches a Component PER URL. "Recreate" therefore re-runs
//    createObject against the CACHED component — cheap, and correct for a
//    property/fixture change. An EDITED widget BODY needs "Reload"
//    (`requestReload()`), which refreshes the copies and rebuilds from disk.
//
// Song-blind (slots.md's paint test): this file names no aesthetic
// decision of its own. It has no palette — it MIXES one out of whatever
// LiveryState is holding (see "Chrome, mixed out of the livery" below), so
// the instrument panel is dressed by the song being previewed rather than
// by a taste this file asserts. Reading `livery.paletteFg` is picking up an
// API; deciding what a gauge looks like is not, and nothing here does that.
import QtQuick
import QtQuick.Effects
import QtQml.Models
import QtQuick.Dialogs
import Quickshell
import Quickshell.Io

ShellRoot {
    id: canvas

    // ── The preview root ───────────────────────────────────────────────────
    readonly property string aoideRoot:
        Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")
    readonly property string controlPath: canvas.aoideRoot + "/preview.json"

    // ── Control state — every field of preview.json v0 ─────────────────────
    property string widgetPath: "songs/sonata/conductor.qml"
    property string songName: "sonata"
    property var widgetWidth: "auto"      // number (logical px) or "auto"
    property var widgetHeight: "auto"
    property bool widgetAspectLock: false
    property int viewportWidth: 1920
    property int viewportHeight: 1080
    property string viewportPreset: "16:9"
    property bool viewportAspectLock: true
    property string anchorSpot: "tl"      // tl t tr l c r bl b br
    property int margin: 0
    property var zoomSetting: "fit"       // "fit" or a number > 0

    // ── Camera — a view, never persisted ───────────────────────────────────
    property real camX: 0
    property real camY: 0
    property bool handHeld: false       // Space held (or the hand button): left-drag pans
    property bool camDragging: false
    property string background: "livery"  // livery | checker
    property string fixture: ""
    property string fixturesDir: ""
    property var fixtures: []
    // `liverySource` is the SPEC last applied, not a resolved file: `live`, a
    // song name, a livery.json path, or a base16 scheme file. `lyra preview
    // set --livery <spec>` resolves every form into ROOT/song/stage/livery.json
    // and LiveryState hot-reloads it, so a palette swap needs no reload here.
    property string liverySource: ""
    property var liveries: []
    // Auto-reload: the absolute paths whose edit should rebuild the config
    // (`lyra preview` writes the loaded widget plus its song's widgets dir),
    // and the toggle that gates it.
    property var watchPaths: []
    // src -> dst: the checkout file each watched path is, and the root's own
    // run/qml COPY of it that the canvas actually loads. Refreshed before
    // every reload so an edit in the checkout reaches the copy; nothing else
    // ever writes under run/qml.
    property var stageMap: ({})
    property bool autoReload: true

    // The control document as last read, so keys this version does not know
    // survive a write (forward compatibility with `lyra preview set`).
    property var controlDoc: ({})
    property bool applying: false         // suppress write-back while applying
    property string lastWritten: ""       // our own text, to ignore the echo
    property string lastApplied: ""       // the last document actually applied

    // ── Error pane, three independent slots ────────────────────────────────
    property string errWidget: ""
    property string errControl: ""
    property string errFiles: ""
    readonly property string errorText: {
        var parts = []
        if (canvas.errWidget !== "") parts.push("widget: " + canvas.errWidget)
        if (canvas.errControl !== "") parts.push("control: " + canvas.errControl)
        if (canvas.errFiles !== "") parts.push("files: " + canvas.errFiles)
        return parts.join("\n")
    }

    // ── Action log ─────────────────────────────────────────────────────────
    property var actionLog: []
    function log(line) {
        var next = canvas.actionLog.slice()
        var t = new Date()
        next.push(Qt.formatTime(t, "hh:mm:ss") + "  " + line)
        while (next.length > 50) next.shift()
        canvas.actionLog = next
    }
    function tail(s, n) {
        var t = ("" + s).replace(/\s+$/, "")
        if (t.length === 0) return ""
        if (t.length <= (n || 160)) return t
        return "…" + t.slice(t.length - (n || 160))
    }

    // ── Injected props ─────────────────────────────────────────────────────
    // REAL LiveryState and StagingEngine (both read under AOIDE_ROOT, so both
    // land in the preview root); STUB bridge and shared.
    property LiveryState liveryState: LiveryState {}
    property StagingEngine stagingEngineReal: StagingEngine {}

    property QtObject sharedStub: QtObject {
        property string tracedSessionId: ""
        property string hoveredSessionId: ""
        property int hoveredWorkspace: -1
    }

    // The stub bridge — every method the songbook widgets call today, each
    // only logging. Confirmed against `grep -rn 'bridge\.' song/songbook/*/
    // widgets/`: sendCommand, focusSession, recheckSessions, toggleRiceMode,
    // refreshUsage, sessionAction. `focusWindow` is on ShellBridge but no
    // committed widget calls it yet; it is stubbed anyway so a widget that
    // starts calling it on this canvas gets a log line, not a crash.
    property QtObject bridgeStub: QtObject {
        function focusSession(sessionId) {
            canvas.log("bridge.focusSession " + JSON.stringify({ sessionId: sessionId }))
        }
        function focusWindow(address) {
            canvas.log("bridge.focusWindow " + JSON.stringify({ address: address }))
        }
        function recheckSessions() { canvas.log("bridge.recheckSessions {}") }
        function toggleRiceMode() { canvas.log("bridge.toggleRiceMode {}") }
        function refreshUsage() { canvas.log("bridge.refreshUsage {}") }
        function sendCommand(obj) {
            canvas.log("bridge.sendCommand " + JSON.stringify(obj))
        }
        // Same signature and callback shape as ShellBridge.sessionAction —
        // cb exactly once, ASYNCHRONOUSLY (Qt.callLater), so SessionMenu's
        // pending state actually renders instead of resolving inside the
        // click handler and the menu never hangs waiting for a daemon.
        function sessionAction(sessionId, action, fields, callback) {
            canvas.log("bridge.sessionAction " + JSON.stringify({
                sessionId: "" + sessionId, action: "" + action, fields: fields || ({})
            }))
            if (!callback) return
            Qt.callLater(function () {
                callback({ ok: true, stub: true, action: "" + action,
                           sessionId: "" + sessionId,
                           message: "preview canvas stub — nothing was sent" })
            })
        }
    }

    // ── The control file ───────────────────────────────────────────────────
    property FileView controlFile: FileView {
        id: controlFile
        path: canvas.controlPath
        watchChanges: true
        printErrors: false
        // Blocking, so the first apply happens in one pass at startup rather
        // than a frame after the widget is already on screen.
        blockLoading: true
        onFileChanged: controlFile.reload()
        onLoaded: canvas.applyControl(controlFile.text())
        onLoadFailed: function (err) {
            canvas.errControl = "preview.json unreadable (" + err + ") at " + canvas.controlPath
        }
        onSaved: canvas.log("preview.json written")
        onSaveFailed: function (err) { canvas.errControl = "preview.json write failed (" + err + ")" }
    }

    function applyControl(text) {
        if (!text || text.trim().length === 0) return
        if (text === canvas.lastWritten) return    // our own write, echoed back
        if (text === canvas.lastApplied) return    // already applied verbatim
        canvas.lastApplied = text
        var doc
        try {
            doc = JSON.parse(text)
        } catch (e) {
            canvas.errControl = "parse error — " + e + " (keeping the last good state)"
            return
        }
        if (!doc || typeof doc !== "object" || doc instanceof Array) {
            canvas.errControl = "preview.json is not a JSON object (keeping the last good state)"
            return
        }
        canvas.applying = true
        canvas.controlDoc = doc
        if (doc.widget !== undefined) canvas.widgetPath = "" + doc.widget
        if (doc.song !== undefined) canvas.songName = "" + doc.song
        if (doc.widgetWidth !== undefined) canvas.widgetWidth = canvas.readAxis(doc.widgetWidth)
        if (doc.widgetHeight !== undefined) canvas.widgetHeight = canvas.readAxis(doc.widgetHeight)
        if (doc.widgetAspectLock !== undefined) canvas.widgetAspectLock = !!doc.widgetAspectLock
        if (doc.viewportWidth !== undefined) canvas.viewportWidth = Math.max(1, Math.round(Number(doc.viewportWidth)))
        if (doc.viewportHeight !== undefined) canvas.viewportHeight = Math.max(1, Math.round(Number(doc.viewportHeight)))
        if (doc.viewportPreset !== undefined) canvas.viewportPreset = "" + doc.viewportPreset
        if (doc.viewportAspectLock !== undefined) canvas.viewportAspectLock = !!doc.viewportAspectLock
        if (doc.anchor !== undefined) canvas.anchorSpot = "" + doc.anchor
        if (doc.margin !== undefined) canvas.margin = Math.max(0, Math.round(Number(doc.margin)))
        if (doc.zoom !== undefined) canvas.zoomSetting = (doc.zoom === "fit") ? "fit" : Math.max(0.01, Number(doc.zoom))
        if (doc.background !== undefined) canvas.background = "" + doc.background
        if (doc.fixture !== undefined) canvas.fixture = "" + doc.fixture
        if (doc.fixturesDir !== undefined) canvas.fixturesDir = "" + doc.fixturesDir
        if (doc.fixtures !== undefined && doc.fixtures instanceof Array) canvas.fixtures = doc.fixtures
        // `livery` is the v0 spelling; `liverySource` supersedes it (one name
        // per thing) and is what gets written back.
        if (doc.liverySource !== undefined) canvas.liverySource = "" + doc.liverySource
        else if (doc.livery !== undefined) canvas.liverySource = "" + doc.livery
        if (doc.liveries !== undefined && doc.liveries instanceof Array) canvas.liveries = doc.liveries
        if (doc.watch !== undefined && doc.watch instanceof Array) canvas.watchPaths = doc.watch
        if (doc.stage !== undefined && doc.stage !== null && typeof doc.stage === "object") canvas.stageMap = doc.stage
        if (doc.autoReload !== undefined) canvas.autoReload = !!doc.autoReload
        canvas.applying = false
        canvas.errControl = ""
        canvas.syncFields()
        canvas.applySizing()
        canvas.ensureBuilt()
    }

    function readAxis(v) {
        if (v === "auto" || v === null || v === undefined) return "auto"
        var n = Number(v)
        return (isNaN(n) || n <= 0) ? "auto" : Math.round(n)
    }

    Timer {
        id: writeDebounce
        interval: 150
        repeat: false
        onTriggered: canvas.writeControl()
    }
    function scheduleWrite() { if (!canvas.applying) writeDebounce.restart() }

    function writeControl() {
        var doc = {}
        var keys = Object.keys(canvas.controlDoc || ({}))    // unknown keys survive
        for (var i = 0; i < keys.length; i++) doc[keys[i]] = canvas.controlDoc[keys[i]]
        doc.schemaVersion = "0"
        doc.widget = canvas.widgetPath
        doc.song = canvas.songName
        doc.widgetWidth = canvas.widgetWidth
        doc.widgetHeight = canvas.widgetHeight
        doc.widgetAspectLock = canvas.widgetAspectLock
        doc.viewportWidth = canvas.viewportWidth
        doc.viewportHeight = canvas.viewportHeight
        doc.viewportPreset = canvas.viewportPreset
        doc.viewportAspectLock = canvas.viewportAspectLock
        doc.anchor = canvas.anchorSpot
        doc.margin = canvas.margin
        doc.zoom = canvas.zoomSetting
        doc.background = canvas.background
        doc.fixture = canvas.fixture
        doc.fixturesDir = canvas.fixturesDir
        doc.fixtures = canvas.fixtures
        delete doc.livery
        doc.liverySource = canvas.liverySource
        doc.liveries = canvas.liveries
        doc.watch = canvas.watchPaths
        doc.stage = canvas.stageMap
        doc.autoReload = canvas.autoReload
        var text = JSON.stringify(doc, null, 2)
        canvas.controlDoc = doc
        canvas.lastWritten = text
        controlFile.setText(text)
    }

    onWidgetPathChanged: { canvas.scheduleWrite(); if (canvas.built) canvas.rebuildWidget() }
    onSongNameChanged: canvas.scheduleWrite()
    onWidgetWidthChanged: { canvas.scheduleWrite(); canvas.applySizing() }
    onWidgetHeightChanged: { canvas.scheduleWrite(); canvas.applySizing() }
    onWidgetAspectLockChanged: canvas.scheduleWrite()
    onViewportWidthChanged: canvas.scheduleWrite()
    onViewportHeightChanged: canvas.scheduleWrite()
    onViewportPresetChanged: canvas.scheduleWrite()
    onViewportAspectLockChanged: canvas.scheduleWrite()
    onAnchorSpotChanged: canvas.scheduleWrite()
    onMarginChanged: canvas.scheduleWrite()
    onZoomSettingChanged: { canvas.resetCamera(); canvas.scheduleWrite() }
    onBackgroundChanged: canvas.scheduleWrite()
    onAutoReloadChanged: canvas.scheduleWrite()

    // ── Existence probes ───────────────────────────────────────────────────
    // LiveryState and the widgets' own FileViews fall back SILENTLY when a
    // file is missing (a default palette, an empty roster) — indistinguishable
    // on screen from a genuinely empty fixture. These watch the same paths
    // only to say so in the error pane.
    property FileView liveryProbe: FileView {
        id: liveryProbe
        path: canvas.aoideRoot + "/song/stage/livery.json"
        watchChanges: true
        printErrors: false
        onFileChanged: liveryProbe.reload()
        onLoaded: canvas.clearFileError("livery.json")
        onLoadFailed: function (err) { canvas.setFileError("livery.json", liveryProbe.path, err) }
        Component.onCompleted: liveryProbe.reload()
    }
    // The four conducting files a fixture set must carry, one probe each.
    property Instantiator stageProbes: Instantiator {
        model: ["sessions", "projects", "hooks", "herald"]
        delegate: FileView {
            id: stageProbe
            required property string modelData
            readonly property string fileName: stageProbe.modelData + ".json"
            path: canvas.aoideRoot + "/state/stage/" + stageProbe.fileName
            watchChanges: true
            printErrors: false
            onFileChanged: stageProbe.reload()
            onLoaded: canvas.clearFileError(stageProbe.fileName)
            onLoadFailed: function (err) { canvas.setFileError(stageProbe.fileName, stageProbe.path, err) }
            Component.onCompleted: stageProbe.reload()
        }
    }
    property var missingFiles: ({})
    function setFileError(name, path, err) {
        var m = canvas.missingFiles
        m[name] = name + " unreadable (" + err + ") at " + path
        canvas.missingFiles = m
        canvas.renderFileErrors()
    }
    function clearFileError(name) {
        var m = canvas.missingFiles
        if (m[name] === undefined) return
        delete m[name]
        canvas.missingFiles = m
        canvas.renderFileErrors()
    }
    function renderFileErrors() {
        var keys = Object.keys(canvas.missingFiles)
        var lines = []
        for (var i = 0; i < keys.length; i++) lines.push(canvas.missingFiles[keys[i]])
        canvas.errFiles = lines.join("\n")
    }

    // ── The one shell seam ─────────────────────────────────────────────────
    // Fixture and livery are FILE COPIES into the preview root, which is the
    // shell's job. `lyra preview set` does the copy AND rewrites preview.json;
    // the control FileView's watch brings the new state back here, so the
    // canvas never writes those two fields itself. Resolved via `lyraBin`
    // (the launching binary's own path, below); a non-zero exit is logged,
    // never fatal.
    property Process lyra: Process {
        id: lyra
        property string tag: ""
        stdout: StdioCollector { id: lyraOut; waitForEnd: true }
        stderr: StdioCollector { id: lyraErr; waitForEnd: true }
        onExited: function (exitCode, exitStatus) {
            var line = "lyra " + lyra.tag + " → exit " + exitCode
            if (("" + lyraOut.text).trim().length > 0) line += " | out: " + canvas.tail(lyraOut.text)
            if (("" + lyraErr.text).trim().length > 0) line += " | err: " + canvas.tail(lyraErr.text)
            canvas.log(line)
        }
    }
    // `lyra <args…>` — the ONE process this canvas starts. The lyra that
    // built this root names itself in preview.json (`lyra`); a bare `lyra`
    // from PATH is the deployed binary, which may predate the preview
    // commands and then fails every rail action silently, so `lyraBin`
    // prefers the control doc's path and only falls back to PATH when the
    // doc predates this field. A non-zero exit is logged, never fatal.
    readonly property string lyraBin: (canvas.controlDoc && canvas.controlDoc.lyra) ? "" + canvas.controlDoc.lyra : "lyra"
    function runLyra(args, tag) {
        if (lyra.running) { canvas.log("lyra busy — ignored: " + tag); return }
        lyra.tag = tag
        lyra.command = [canvas.lyraBin].concat(args)
        canvas.log("$ " + lyra.command.join(" "))
        lyra.running = true
    }
    function previewSet(args, tag) {
        canvas.runLyra(["preview", "set", "--root", canvas.aoideRoot].concat(args), tag)
    }

    // ── Auto-reload ────────────────────────────────────────────────────────
    // Quickshell 0.3.1 does NOT notice an edit to a file reached through
    // `Qt.createComponent` — neither by default nor with `Quickshell.watchFiles
    // = true` (both probed; its watcher covers the config's static import
    // graph, and a dynamically resolved slot body is not in it). So the canvas
    // watches the paths `lyra preview` lists in `watch` itself and rebuilds the
    // config, which is the only thing that clears Qt's per-URL component cache.
    // State returns from preview.json, so a rebuild is invisible except that
    // the edit is on screen.
    property string lastReloadPath: ""
    property Instantiator reloadWatchers: Instantiator {
        model: canvas.watchPaths
        delegate: FileView {
            required property string modelData
            path: modelData
            watchChanges: true
            blockLoading: false
            printErrors: false
            // `watchChanges` only announces the change; the text stays as
            // first loaded until `reload()`, and `refreshCopies` reads it
            // 300 ms later, so re-read now.
            onFileChanged: { reload(); canvas.noteEdit(modelData) }
        }
    }
    Timer {
        id: reloadDebounce
        interval: 300
        repeat: false
        onTriggered: {
            // The in-memory log does not survive the rebuild it is announcing,
            // so the same line also goes to the Quickshell log, where it does.
            canvas.log("auto-reload ← " + canvas.lastReloadPath)
            console.log("[aoide/widgetpreview] auto-reload ←", canvas.lastReloadPath)
            canvas.requestReload()
        }
    }
    // Every reload path: copy each watched checkout file over the root's own
    // run/qml copy of it, give the atomic writes a moment to land, then
    // rebuild the config from disk.
    Timer {
        id: reloadSettle
        interval: 150
        repeat: false
        onTriggered: Quickshell.reload(false)
    }
    function refreshCopies() {
        var n = 0
        for (var i = 0; i < reloadWatchers.count; i++) {
            var v = reloadWatchers.objectAt(i)
            if (!v) continue
            // Keyed on the watch entry itself (the delegate's modelData), the
            // exact string lyra wrote into `stage` — never on FileView.path,
            // which may come back normalised.
            var dst = canvas.stageMap["" + v.modelData]
            if (!dst) continue
            canvas.writeSidecar("" + dst, v.text())
            n++
        }
        return n
    }
    function requestReload() {
        var n = canvas.refreshCopies()
        // The in-memory log dies with the reload; the console line survives.
        canvas.log("reload: " + n + " copies refreshed")
        console.log("[aoide/widgetpreview] reload:", n, "copies refreshed of", canvas.watchPaths.length, "watched")
        reloadSettle.restart()
    }
    function noteEdit(path) {
        if (!canvas.autoReload) return
        canvas.lastReloadPath = "" + path
        reloadDebounce.restart()
    }

    // ── Declare ────────────────────────────────────────────────────────────
    // Writing the previewed body and palette back into the checkout song is
    // the one irreversible thing the canvas can ask for, so it arms first and
    // fires on the second click; the arming lapses after five seconds.
    property bool declareArmed: false
    Timer {
        id: declareDisarm
        interval: 5000
        repeat: false
        onTriggered: canvas.declareArmed = false
    }
    function declare() {
        if (!canvas.declareArmed) {
            canvas.declareArmed = true
            declareDisarm.restart()
            canvas.log("declare ARMED — click again within 5s to write into the checkout song")
            return
        }
        canvas.declareArmed = false
        declareDisarm.stop()
        canvas.runLyra(["preview", "declare", "--root", canvas.aoideRoot], "declare")
    }

    // ── Widget loading — WidgetSlot's mechanism ────────────────────────────
    property Item widgetItem: null
    property string widgetUrlText: ""

    // A path under a checkout's `song/songbook/<s>/widgets/<file>` is rewritten
    // to `songs/<s>/<file>` so it loads from INSIDE the config root, where the
    // facet types (MoodFaces, ScrollRail, …) its `import "../.."` needs
    // actually sit. Loading the same file as a bare `file://` URL puts it
    // outside the root and every such import fails — the exact reason
    // ConductorPreview needs its CONDUCTOR_WIDGET dance.
    // The checkout file behind `widgetPath`: an absolute path is itself, the
    // `songs/<s>/<file>` and bare-slot spellings are found through the stage
    // map's source side (the copy under run/qml is never the answer).
    function widgetCheckoutPath() {
        var p = ("" + canvas.widgetPath).trim()
        if (p.charAt(0) === "/") return p
        var m = /^songs\/([^\/]+)\/(.+)$/.exec(p)
        var tail = m ? "/song/songbook/" + m[1] + "/widgets/" + m[2] : "/modules/facets/quickshell/qml/" + p
        for (var k in canvas.stageMap) if (k.length > tail.length && k.slice(-tail.length) === tail) return k
        return ""
    }
    // The native picker opens at the current widget's folder; the pick that
    // follows moves `widgetPath` there, so the folder carries over.
    function openWidgetDialog() {
        var c = canvas.widgetCheckoutPath()
        if (c !== "" && c.lastIndexOf("/") > 0) widgetDialog.currentFolder = "file://" + c.slice(0, c.lastIndexOf("/"))
        widgetDialog.open()
    }
    function widgetUrl() {
        var p = ("" + canvas.widgetPath).trim()
        if (p.length === 0) return ""
        if (p.indexOf("://") > 0) return p
        if (p.charAt(0) !== "/") return Qt.resolvedUrl(p)
        var m = /\/song\/songbook\/([^\/]+)\/widgets\/(.+)$/.exec(p)
        if (m) return Qt.resolvedUrl("songs/" + m[1] + "/" + m[2])
        return "file://" + p
    }

    // ONE universal prop set for every slot. See the header: an undeclared
    // initial property warns and the object is still created, so a slot→extras
    // table would buy nothing but a table to keep in sync with slots.md.
    // `clipboard`/`ledger` (the launcher's extras) are passed as null rather
    // than real AoideClipboard/GrimoireLedger instances — both reach the
    // system, and this canvas starts exactly one process.
    function widgetProps() {
        return {
            "livery": canvas.liveryState,
            "bridge": canvas.bridgeStub,
            "shared": canvas.sharedStub,
            "stagingEngine": canvas.stagingEngineReal,
            "powermenu": null,
            "dock": null,
            "clipboard": null,
            "ledger": null,
            "stagePath": canvas.aoideRoot + "/state/stage/sessions.json"
        }
    }

    function destroyWidget() {
        if (canvas.widgetItem) {
            canvas.widgetItem.destroy()
            canvas.widgetItem = null
        }
    }

    // Every build carries a sequence number. A component that is still Loading
    // finishes through a connected handler, and a SECOND rebuild (the control
    // file landing after the first build, an auto-reload, a path edit) can
    // start before that handler fires — without this guard the stale handler
    // still ran createObject and left an orphan widget drawn over the live
    // one, which is exactly what a reload produced.
    property int buildSeq: 0

    function rebuildWidget() {
        canvas.buildSeq++
        var seq = canvas.buildSeq
        canvas.destroyWidget()
        canvas.errWidget = ""
        var url = canvas.widgetUrl()
        canvas.widgetUrlText = "" + url
        if (url === "") { canvas.errWidget = "no widget path set"; return }
        var comp = Qt.createComponent(url)
        if (comp.status === Component.Loading) {
            comp.statusChanged.connect(function () { canvas.finishCreate(comp, seq) })
            return
        }
        canvas.finishCreate(comp, seq)
    }

    function finishCreate(comp, seq) {
        if (seq !== canvas.buildSeq) return
        if (comp.status === Component.Loading) return
        if (comp.status === Component.Error) {
            canvas.errWidget = comp.errorString()
            canvas.log("load FAILED " + canvas.widgetUrlText)
            return
        }
        var item = comp.createObject(widgetHost, canvas.widgetProps())
        if (!item) {
            canvas.errWidget = "createObject returned null for " + canvas.widgetUrlText
                + " — a required property this canvas does not pass, or a root that is not an Item"
            canvas.log("createObject NULL " + canvas.widgetUrlText)
            return
        }
        // A window-rooted slot (dock/powermenu/launcher/herald) MAPS A REAL
        // wlr-layer-shell surface on the live compositor the moment it is
        // created, and the handle cannot be held in `widgetItem` (QObject* to
        // QQuickItem*), so nothing could ever take it down again. Destroyed
        // here, before it is adopted, sized or reported loaded.
        // The discriminator is the PARENT, not the shape: a PanelWindow body
        // creates as qs::wayland::layershell::WaylandPanelInterface, which HAS
        // an implicitWidth (probed: 442) but ignores the visual parent it was
        // handed — an Item-rooted body comes back parented to widgetHost.
        if (item.parent !== widgetHost) {
            item.destroy()
            canvas.errWidget = canvas.widgetPath + " is a window-rooted slot (PanelWindow)"
                + " — the canvas previews Item-rooted bodies only"
            canvas.log("refused (window-rooted) " + canvas.widgetUrlText)
            return
        }
        canvas.widgetItem = item
        canvas.applySizing()
        canvas.log("loaded " + canvas.widgetUrlText)
    }

    // A numeric axis forces item.width/height; `auto` restores the default
    // binding to the item's own implicit size, so the box tracks a widget
    // whose implicit size changes while it is loaded.
    function applySizing() {
        var it = canvas.widgetItem
        if (!it || it.implicitWidth === undefined) return
        if (typeof canvas.widgetWidth === "number") it.width = canvas.widgetWidth
        else it.width = Qt.binding(function () { return it.implicitWidth })
        if (typeof canvas.widgetHeight === "number") it.height = canvas.widgetHeight
        else it.height = Qt.binding(function () { return it.implicitHeight })
    }

    // `built` is the single-build latch. EMPIRICAL (Qt 6.11): an object held in
    // a property — `property FileView controlFile` above — completes AFTER the
    // root that declares it, so the control file is still unread when this
    // handler runs. Building here painted the DEFAULT widget and then rebuilt
    // with preview.json's one: two creations and a visible flash per launch.
    // So the root only arms. Whichever comes first does the single build — the
    // control file's first apply, or `bootBuild` one event-loop pass later,
    // which is what gets a widget on screen when preview.json is unreadable.
    property bool ready: false
    property bool built: false
    function ensureBuilt() {
        if (!canvas.ready || canvas.built) return
        canvas.built = true
        canvas.rebuildWidget()
    }
    Timer { id: bootBuild; interval: 0; repeat: false; onTriggered: canvas.ensureBuilt() }
    Component.onCompleted: { canvas.ready = true; bootBuild.start() }

    // ── Annotate + the agent seam ──────────────────────────────────────────
    // The canvas paints; the shell drives (AGENTS.md rule 7). Everything in
    // this section is reachable from a terminal — `lyra preview shot|tree|
    // notes` talk to the IpcHandler below and to ROOT/notes.json — and the
    // rail's ANNOTATE controls are a convenience over that same state, never
    // a second source of it. Delete every .qml and the work list survives in
    // notes.json; only the painting and the pixel grab are QML's.
    property string annotMode: "off"      // off pick rect ellipse arrow line note
    property string noteText: ""
    // One dimming for a done note, wherever it is drawn: the overlay mark and
    // the rail row are the same note seen twice.
    readonly property real doneOpacity: 0.45
    property var notes: []                // notes.json's `notes`, verbatim
    property var notesDoc: ({})           // its other keys, preserved on write
    readonly property string notesPath: canvas.aoideRoot + "/notes.json"
    property string lastNotesWritten: ""

    // ── Element paths ──────────────────────────────────────────────────────
    // One `Type[i]` per level from the widget root, i counting among SAME-type
    // siblings in `children` order, `#objectName` appended when the item names
    // itself, levels joined by "/". The grammar `lyra preview tree` prints and
    // `--element` accepts, so a note made by clicking and a note made from the
    // shell address the same item.
    //
    // EMPIRICAL (Qt 6.11) — what `String(item)` gives, which is where the type
    // name comes from: a built-in is "QQuickRectangle(0x55…)" or, when it has
    // an objectName, "QQuickText(0x55…, \"title\")"; a type defined in its own
    // file is "Custom_QMLTYPE_1(0x55…)" interpreted and "QQuickItem_QML_42
    // (0x55…)" once compiled, i.e. the BASE type plus a generation tag, not
    // the file name; an inline component is plain "Inline(0x55…)". So: cut at
    // "(", drop a "_QMLTYPE_<n>" or "_QML_<n>" tail, drop a leading "QQuick".
    // Two consequences worth knowing: QQuickCanvasItem reads "CanvasItem", not
    // "Canvas", and a widget whose root is a plain `Item` is "Item", NOT
    // "conductor" — the path grammar names shapes, not files. A Repeater's
    // generated items are siblings of the Repeater in `children`, not its
    // descendants.
    function typeNameOf(o) {
        var s = ("" + o).split("(")[0].replace(/_QML(?:TYPE)?_\d+$/, "")
        if (s.indexOf("QQuick") === 0) s = s.substring(6)
        return s
    }

    function pathOf(item) {
        var root = canvas.widgetItem
        if (!item || !root) return ""
        if (item === root) return ""
        var segs = []
        var cur = item
        while (cur && cur !== root) {
            var p = cur.parent
            if (!p) return ""                  // detached: no path from the root
            var t = canvas.typeNameOf(cur)
            var i = 0
            for (var k = 0; k < p.children.length; k++) {
                if (p.children[k] === cur) break
                if (canvas.typeNameOf(p.children[k]) === t) i++
            }
            segs.unshift(t + "[" + i + "]" + (cur.objectName ? "#" + cur.objectName : ""))
            cur = p
        }
        return (cur === root) ? segs.join("/") : ""
    }

    // The inverse. An empty path (or "-") is the widget root itself; a path
    // whose level no longer exists resolves to null, which is how a note goes
    // "(unresolved)" after the body it pointed at was edited away.
    function itemAt(path) {
        var cur = canvas.widgetItem
        if (!cur) return null
        if (!path || path === "" || path === "-") return cur
        var segs = ("" + path).split("/")
        for (var s = 0; s < segs.length; s++) {
            var seg = segs[s], name = ""
            var hash = seg.indexOf("#")
            if (hash >= 0) { name = seg.substring(hash + 1); seg = seg.substring(0, hash) }
            var ob = seg.indexOf("[")
            var t = (ob >= 0) ? seg.substring(0, ob) : seg
            var want = (ob >= 0) ? parseInt(seg.substring(ob + 1, seg.length - 1), 10) : 0
            var ch = cur.children || []
            var i = 0, found = null
            for (var k = 0; k < ch.length; k++) {
                if (canvas.typeNameOf(ch[k]) !== t) continue
                if (i === want) { found = ch[k]; break }
                i++
            }
            if (!found || (name && found.objectName !== name)) return null
            cur = found
        }
        return cur
    }

    // Deepest visible item under a point in widget-root coordinates. Later
    // siblings paint over earlier ones, so the walk goes back to front and the
    // item returned is the one the eye is on. Ignores item transforms (rotation
    // or scale inside a widget would need mapFromItem per level); nothing in
    // the sonata bodies uses one.
    function hitTest(item, x, y) {
        var ch = item.children || []
        for (var k = ch.length - 1; k >= 0; k--) {
            var c = ch[k]
            if (!c.visible || c.width === undefined) continue
            if (c.width <= 0 || c.height <= 0 || c.opacity <= 0) continue
            var lx = x - c.x, ly = y - c.y
            if (lx < 0 || ly < 0 || lx > c.width || ly > c.height) continue
            return canvas.hitTest(c, lx, ly) || c
        }
        return null
    }
    function deepestAt(x, y) {
        var root = canvas.widgetItem
        if (!root) return null
        return canvas.hitTest(root, x, y) || root
    }

    // The live item tree, widget-local rects. `lyra preview tree` joins this
    // with a static parse of the widget's source to attach file:line.
    function treeOf(item) {
        var root = canvas.widgetItem
        var p = item.mapToItem(root, 0, 0)
        var node = {
            "type": canvas.typeNameOf(item),
            "objectName": item.objectName || "",
            "path": canvas.pathOf(item),
            "rect": { "x": Math.round(p.x), "y": Math.round(p.y),
                      "w": Math.round(item.width), "h": Math.round(item.height) },
            "visible": !!item.visible,
            "children": []
        }
        if (typeof item.text === "string") node.text = item.text
        var ch = item.children || []
        for (var k = 0; k < ch.length; k++) node.children.push(canvas.treeOf(ch[k]))
        return node
    }

    // ── Hover / drag state for the overlay ─────────────────────────────────
    property var hoverItem: null
    property string hoverPath: ""
    property rect hoverRect: Qt.rect(0, 0, 0, 0)
    function clearHover() {
        canvas.hoverItem = null
        canvas.hoverPath = ""
        canvas.hoverRect = Qt.rect(0, 0, 0, 0)
    }
    function hoverAt(x, y) {
        var it = canvas.deepestAt(x, y)
        if (!it) { canvas.clearHover(); return }
        var p = it.mapToItem(canvas.widgetItem, 0, 0)
        canvas.hoverItem = it
        canvas.hoverPath = canvas.pathOf(it)
        canvas.hoverRect = Qt.rect(p.x, p.y, it.width, it.height)
    }
    function hoverLabel() {
        if (!canvas.hoverItem) return ""
        var it = canvas.hoverItem
        return canvas.typeNameOf(it) + (it.objectName ? "#" + it.objectName : "")
            + " · " + Math.round(it.width) + "×" + Math.round(it.height)
    }
    function pickAt(x, y) {
        canvas.hoverAt(x, y)
        if (!canvas.hoverItem) return
        canvas.addNote("highlight", "", { "x": canvas.hoverRect.x, "y": canvas.hoverRect.y,
                                          "w": canvas.hoverRect.width, "h": canvas.hoverRect.height },
                       canvas.hoverPath, canvas.noteText)
    }

    property bool dragging: false
    property real dragX1: 0
    property real dragY1: 0
    property real dragX2: 0
    property real dragY2: 0
    // rect/ellipse normalise; arrow/line keep the SIGN of w/h, because that is
    // the only place in the notes schema the direction of a drag can live and
    // an arrow that always points down-right is not an arrow.
    readonly property bool dragIsStroke: canvas.annotMode === "arrow" || canvas.annotMode === "line"
    function dragRect() {
        var signed = canvas.dragIsStroke
        var dx = canvas.dragX2 - canvas.dragX1, dy = canvas.dragY2 - canvas.dragY1
        if (signed) return { "x": canvas.dragX1, "y": canvas.dragY1, "w": dx, "h": dy }
        return { "x": Math.min(canvas.dragX1, canvas.dragX2), "y": Math.min(canvas.dragY1, canvas.dragY2),
                 "w": Math.abs(dx), "h": Math.abs(dy) }
    }

    // ── notes.json ─────────────────────────────────────────────────────────
    // Same round trip as preview.json: watched, unknown keys preserved, our
    // own write ignored on the way back. `n` is max+1 and never renumbers, so
    // a note number in a commit message keeps meaning something.
    property FileView notesFile: FileView {
        id: notesFile
        path: canvas.notesPath
        watchChanges: true
        printErrors: false
        blockLoading: true
        onFileChanged: notesFile.reload()
        onLoaded: canvas.applyNotes(notesFile.text())
        onSaveFailed: function (err) { canvas.errControl = "notes.json write failed (" + err + ")" }
    }
    function applyNotes(text) {
        if (!text || text.trim().length === 0) return
        if (text === canvas.lastNotesWritten) return
        var doc
        try {
            doc = JSON.parse(text)
        } catch (e) {
            canvas.errControl = "notes.json parse error — " + e + " (keeping the last good notes)"
            return
        }
        if (!doc || typeof doc !== "object" || doc instanceof Array) {
            canvas.errControl = "notes.json is not a JSON object (keeping the last good notes)"
            return
        }
        canvas.errControl = ""
        canvas.notesDoc = doc
        canvas.notes = (doc.notes instanceof Array) ? doc.notes : []
    }
    function writeNotes() {
        var doc = {}
        var keys = Object.keys(canvas.notesDoc || ({}))
        for (var i = 0; i < keys.length; i++) doc[keys[i]] = canvas.notesDoc[keys[i]]
        doc.schemaVersion = 0
        doc.notes = canvas.notes
        var text = JSON.stringify(doc, null, 2)
        canvas.lastNotesWritten = text
        notesFile.setText(text)
    }
    function nextNoteN() {
        var m = 0
        for (var i = 0; i < canvas.notes.length; i++) m = Math.max(m, Number(canvas.notes[i].n) || 0)
        return m + 1
    }
    function addNote(kind, shape, rect, element, text) {
        var note = {
            "n": canvas.nextNoteN(),
            "kind": kind,
            "text": "" + (text || ""),
            "createdAt": new Date().toISOString(),
            "done": false
        }
        if (shape) note.shape = shape
        if (rect) note.rect = { "x": Math.round(rect.x), "y": Math.round(rect.y),
                                "w": Math.round(rect.w), "h": Math.round(rect.h) }
        if (element) note.element = element
        var list = canvas.notes.slice()
        list.push(note)
        canvas.notes = list
        canvas.writeNotes()
        canvas.log("note #" + note.n + " " + kind + (shape ? " " + shape : "")
                   + (element ? " " + element : "") + (note.text ? " — " + note.text : ""))
    }
    function setNoteDone(n, done) {
        var list = []
        for (var i = 0; i < canvas.notes.length; i++) {
            var o = canvas.notes[i]
            if (Number(o.n) === Number(n)) {
                var c = {}
                for (var k in o) c[k] = o[k]
                c.done = !!done
                list.push(c)
            } else list.push(o)
        }
        canvas.notes = list
        canvas.writeNotes()
        canvas.log("note #" + n + (done ? " done" : " reopened"))
    }
    function deleteNote(n) {
        var list = []
        for (var i = 0; i < canvas.notes.length; i++)
            if (Number(canvas.notes[i].n) !== Number(n)) list.push(canvas.notes[i])
        canvas.notes = list
        canvas.writeNotes()
        canvas.log("note #" + n + " deleted")
    }

    // ── Grabs ──────────────────────────────────────────────────────────────
    // A grab is ASYNC — the ipc call returns "queued" and the PNG lands a
    // frame or two later, which is why the Rust side polls for the file rather
    // than trusting the return value, and why a failure has to be written
    // where the CLI can find it: `<out>.error`.
    //
    // ONE WRITER PER WRITE. A single re-pointed FileView loses writes: two
    // shots a second apart repoint `path` while the first `setText` is still
    // outstanding and the first sidecar never appears at all (reproduced three
    // times in review). Each sidecar therefore gets its own FileView, created
    // here and destroyed when the file system acknowledges it.
    Component {
        id: sidecarWriter
        FileView {
            id: sidecar
            printErrors: false
            property string body: ""
            onSaved: { canvas.log("sidecar → " + sidecar.path); sidecar.destroy() }
            onSaveFailed: function (err) {
                canvas.log("sidecar FAILED (" + err + ") " + sidecar.path)
                sidecar.destroy()
            }
            Component.onCompleted: sidecar.setText(sidecar.body)
        }
    }
    function writeSidecar(path, text) {
        if (!sidecarWriter.createObject(canvas, { "path": path, "body": text }))
            canvas.log("sidecar writer could not be created for " + path)
    }

    function shotFailed(out, msg) {
        canvas.errWidget = msg
        canvas.log("shot FAILED — " + msg)
        if (out) canvas.writeSidecar(out + ".error", msg + "\n")
    }
    // Grabbed at 1:1 WIDGET pixels: the canvas zoom scales the rendered
    // viewport, never the item's own layout, so the grab size is the item's
    // own size and a shot is identical at ×0.4 and at ×2.
    function grabTo(item, out) {
        var meta = {
            "widgetRect": { "x": 0, "y": 0, "w": Math.round(item.width), "h": Math.round(item.height) },
            "scale": canvas.effScale
        }
        var ok = item.grabToImage(function (result) {
            if (!result.saveToFile(out)) { canvas.shotFailed(out, "saveToFile refused " + out); return }
            canvas.writeSidecar(out + ".meta.json", JSON.stringify(meta, null, 2))
            canvas.log("shot → " + out)
        }, Qt.size(Math.max(1, Math.round(item.width)), Math.max(1, Math.round(item.height))))
        if (!ok) canvas.shotFailed(out, "grabToImage refused (the item has no renderable size)")
        return ok
    }

    // ── Geometry ───────────────────────────────────────────────────────────
    readonly property real boxW: canvas.widgetItem ? canvas.widgetItem.width : 0
    readonly property real boxH: canvas.widgetItem ? canvas.widgetItem.height : 0

    function anchorX(bw) {
        var a = canvas.anchorSpot
        if (a === "tl" || a === "l" || a === "bl") return canvas.margin
        if (a === "t" || a === "c" || a === "b") return (canvas.viewportWidth - bw) / 2
        return canvas.viewportWidth - bw - canvas.margin
    }
    function anchorY(bh) {
        var a = canvas.anchorSpot
        if (a === "tl" || a === "t" || a === "tr") return canvas.margin
        if (a === "l" || a === "c" || a === "r") return (canvas.viewportHeight - bh) / 2
        return canvas.viewportHeight - bh - canvas.margin
    }

    // fit NEVER upscales — a 240-wide widget on a 1920 viewport is shown at
    // 1:1 inside the pane, not blown up to fill it.
    readonly property real fitScale: {
        var pw = stagePane.width - 24, ph = stagePane.height - 64
        if (canvas.viewportWidth <= 0 || canvas.viewportHeight <= 0 || pw <= 0 || ph <= 0) return 1
        return Math.min(1.0, Math.min(pw / canvas.viewportWidth, ph / canvas.viewportHeight))
    }
    readonly property real effScale:
        (canvas.zoomSetting === "fit") ? canvas.fitScale : Math.max(0.01, Number(canvas.zoomSetting))

    // Zoom about a frame point: the scene point under (px, py) stays under
    // it. Any other zoom change (button, field, preview.json) resets the
    // camera — see onZoomSettingChanged — so this sets the zoom FIRST and
    // places the camera after.
    function zoomAt(px, py, factor) {
        var s = canvas.effScale
        var n = Math.round(Math.min(8, Math.max(0.05, s * factor)) * 1000) / 1000
        var cx = canvas.camX, cy = canvas.camY
        canvas.zoomSetting = n
        canvas.camX = px - (px - cx) * (n / s)
        canvas.camY = py - (py - cy) * (n / s)
        canvas.syncFields()
    }
    function resetCamera() { canvas.camX = 0; canvas.camY = 0 }

    // ── Field edits ────────────────────────────────────────────────────────
    function axisText(v) { return (typeof v === "number") ? ("" + v) : "auto" }

    function setWidgetWidth(raw) {
        var t = ("" + raw).trim().toLowerCase()
        var prevW = canvas.widgetWidth, prevH = canvas.widgetHeight
        var next = (t === "auto" || t === "") ? "auto" : Math.max(1, Math.round(Number(t)))
        if (typeof next === "number" && isNaN(next)) { canvas.syncFields(); return }
        canvas.widgetWidth = next
        if (canvas.widgetAspectLock && typeof next === "number"
                && typeof prevW === "number" && typeof prevH === "number" && prevW > 0)
            canvas.widgetHeight = Math.max(1, Math.round(next * prevH / prevW))
        canvas.syncFields()
    }
    function setWidgetHeight(raw) {
        var t = ("" + raw).trim().toLowerCase()
        var prevW = canvas.widgetWidth, prevH = canvas.widgetHeight
        var next = (t === "auto" || t === "") ? "auto" : Math.max(1, Math.round(Number(t)))
        if (typeof next === "number" && isNaN(next)) { canvas.syncFields(); return }
        canvas.widgetHeight = next
        if (canvas.widgetAspectLock && typeof next === "number"
                && typeof prevW === "number" && typeof prevH === "number" && prevH > 0)
            canvas.widgetWidth = Math.max(1, Math.round(next * prevW / prevH))
        canvas.syncFields()
    }
    function setViewportWidth(raw) {
        var n = Math.round(Number(("" + raw).trim()))
        if (isNaN(n) || n < 1) { canvas.syncFields(); return }
        var ratio = canvas.viewportHeight / canvas.viewportWidth
        canvas.viewportWidth = n
        if (canvas.viewportAspectLock) canvas.viewportHeight = Math.max(1, Math.round(n * ratio))
        canvas.viewportPreset = "custom"
        canvas.syncFields()
    }
    function setViewportHeight(raw) {
        var n = Math.round(Number(("" + raw).trim()))
        if (isNaN(n) || n < 1) { canvas.syncFields(); return }
        var ratio = canvas.viewportWidth / canvas.viewportHeight
        canvas.viewportHeight = n
        if (canvas.viewportAspectLock) canvas.viewportWidth = Math.max(1, Math.round(n * ratio))
        canvas.viewportPreset = "custom"
        canvas.syncFields()
    }
    function setPreset(name, w, h) {
        canvas.viewportPreset = name
        canvas.viewportWidth = w
        canvas.viewportHeight = h
        canvas.syncFields()
    }
    function setZoom(raw) {
        var t = ("" + raw).trim().toLowerCase()
        if (t === "fit" || t === "") { canvas.zoomSetting = "fit"; canvas.syncFields(); return }
        var n = Number(t)
        if (isNaN(n) || n <= 0) { canvas.syncFields(); return }
        canvas.zoomSetting = n
        canvas.syncFields()
    }
    function setMargin(raw) {
        var n = Math.round(Number(("" + raw).trim()))
        if (isNaN(n) || n < 0) { canvas.syncFields(); return }
        canvas.margin = n
        canvas.syncFields()
    }

    // TextInputs hold their own text once typed in (the binding breaks on the
    // first keystroke), so every path that changes state from OUTSIDE a field
    // pushes the canonical values back into the fields here.
    function syncFields() {
        widgetField.setText(canvas.widgetPath)
        songField.setText(canvas.songName)
        wField.setText(canvas.axisText(canvas.widgetWidth))
        hField.setText(canvas.axisText(canvas.widgetHeight))
        marginField.setText("" + canvas.margin)
        vwField.setText("" + canvas.viewportWidth)
        vhField.setText("" + canvas.viewportHeight)
        zoomField.setText(canvas.zoomSetting === "fit" ? "fit" : ("" + canvas.zoomSetting))
        liveryField.setText(canvas.liverySource)
    }

    // ── Chrome, mixed out of the livery ────────────────────────────────────
    // The canvas wears the palette it is previewing. Every colour below is a
    // MIX of `paletteBg` and `paletteFg` (or the accent/urgent roles verbatim),
    // never a fixed hex — so the rail tracks a light livery (sonata) and a dark
    // one (fugue) with no branch and no theme table, and a livery whose own
    // contrast is broken is visibly broken in the instrument panel too, with
    // nowhere to hide. The ONLY literals left in this file are the
    // checkerboard's two neutral greys, which exist precisely to be
    // palette-independent.
    function mix(a, b, t) {
        return Qt.rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1)
    }
    function fade(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
    function lum(c) { return 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b }
    function hexOf(c) {
        function h(v) { var s = Math.round(v * 255).toString(16); return s.length < 2 ? "0" + s : s }
        return "#" + h(c.r) + h(c.g) + h(c.b)
    }
    // Readable ink for a swatch: whichever of the palette's own two poles sits
    // further from it in luminance. Keeps a hex label legible on base00 and on
    // base07 alike without inventing a colour the livery does not contain.
    function inkOn(c) {
        var f = canvas.liveryState.paletteFg, b = canvas.liveryState.paletteBg
        return (Math.abs(canvas.lum(c) - canvas.lum(f)) >= Math.abs(canvas.lum(c) - canvas.lum(b))) ? f : b
    }

    readonly property color chromeBg: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.04)
    readonly property color chromePanel: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.10)
    readonly property color chromeSunk: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.17)
    readonly property color chromeBtn: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.14)
    readonly property color chromeHover: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.22)
    readonly property color chromeLine: canvas.mix(canvas.liveryState.paletteBg, canvas.liveryState.paletteFg, 0.30)
    readonly property color chromeText: canvas.liveryState.paletteFg
    readonly property color chromeDim: canvas.mix(canvas.liveryState.paletteFg, canvas.liveryState.paletteBg, 0.42)
    readonly property color chromeSel: canvas.liveryState.paletteAccent
    readonly property color chromeErr: canvas.liveryState.paletteUrgent

    // ── Palette read-out ───────────────────────────────────────────────────
    // The five palette ROLES every widget binds to, and the sixteen base16
    // slots underneath them. base16 is an OPTIONAL tier in the livery schema
    // (LiveryState falls back to the accent for every one of them when the
    // block is absent), so the strip is empty rather than fabricated when a
    // livery carries no scheme — the rail says so in words.
    readonly property var roleSwatches: [
        { "name": "bg", "value": canvas.liveryState.paletteBg },
        { "name": "fg", "value": canvas.liveryState.paletteFg },
        { "name": "accent", "value": canvas.liveryState.paletteAccent },
        { "name": "urgent", "value": canvas.liveryState.paletteUrgent },
        { "name": "hot", "value": canvas.liveryState.paletteHot }
    ]
    readonly property var base16Swatches: {
        var raw = canvas.liveryState.raw
        var b = (raw && raw.base16) ? raw.base16 : null
        if (!b) return []
        var digits = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F"]
        var out = []
        for (var i = 0; i < digits.length; i++) {
            var key = "base0" + digits[i]
            if (b[key] !== undefined && b[key] !== null) out.push({ "name": "0" + digits[i], "value": b[key] })
        }
        return out
    }

    // Inline components reach `canvas`'s own ids and properties — probed on
    // Qt 6.11, which is NOT what the inline-component docs' scope warning
    // leads you to expect — so the chrome tokens above are read directly
    // instead of being re-passed at each of the thirty call sites.
    // One tooltip for the whole window (a tip inside the rail would be
    // clipped by the rail's own scroller): a hovered button files its text
    // and window position here, and `tipBox` in the window shows it.
    property string tipText: ""
    property real tipX: 0
    property real tipY: 0
    function showTip(item, text) {
        if (text === "") { canvas.tipText = ""; return }
        var p = item.mapToItem(null, 0, item.height + 4)
        canvas.tipX = p.x; canvas.tipY = p.y
        canvas.tipText = text
    }

    // An Iconoir glyph from run/qml/icons/ (the facet's `lyra icon resolve`
    // output, copied there by `lyra preview`). The SVGs are mono
    // (`stroke="currentColor"`), so one MultiEffect tint recolours the whole
    // glyph from the chrome.
    component RailIcon: Item {
        id: ric
        property string name: ""
        property color tint: canvas.chromeDim
        width: 14
        height: 14
        Image {
            id: ricImg
            anchors.fill: parent
            source: ric.name === "" ? "" : Qt.resolvedUrl("icons/iconoir/" + ric.name + ".svg")
            sourceSize: Qt.size(28, 28)
            fillMode: Image.PreserveAspectFit
            smooth: true
            visible: false
        }
        // The resolved glyphs are black strokes; colorization keeps the
        // source luminance, so black would stay black on a dark chrome.
        // Brightness lifts the strokes to white first, then the tint lands.
        MultiEffect {
            anchors.fill: ricImg
            source: ricImg
            brightness: 1.0
            colorization: 1.0
            colorizationColor: ric.tint
        }
    }

    // `icon` swaps the label for a glyph (the label becomes the tooltip
    // unless `tip` says otherwise); `enabled: false` greys it out.
    component RailButton: Rectangle {
        id: rb
        property string label: ""
        property string icon: ""
        property string tip: ""
        property bool active: false
        signal clicked()
        readonly property string tipString: rb.tip !== "" ? rb.tip : (rb.icon !== "" ? rb.label : "")
        // Tab reaches every button; Space/Return/Enter press it, and focus
        // shows its tooltip so a keyboard user sees what a glyph is.
        activeFocusOnTab: true
        Keys.onPressed: function (e) {
            if (e.key !== Qt.Key_Space && e.key !== Qt.Key_Return && e.key !== Qt.Key_Enter) return
            e.accepted = true
            rb.clicked()
        }
        onActiveFocusChanged: canvas.showTip(rb, rb.activeFocus ? rb.tipString : "")
        onTipStringChanged: if (rb.activeFocus || rbMouse.containsMouse) canvas.showTip(rb, rb.tipString)
        implicitWidth: rb.icon !== "" ? 26 : Math.max(26, rbText.implicitWidth + 12)
        implicitHeight: 21
        radius: 3
        opacity: rb.enabled ? 1 : 0.35
        color: rb.active ? canvas.fade(canvas.chromeSel, 0.30)
                         : (rbMouse.containsMouse ? canvas.chromeHover : canvas.chromeBtn)
        border.width: rb.activeFocus ? 2 : 1
        border.color: (rb.active || rb.activeFocus) ? canvas.chromeSel : canvas.chromeLine
        Text {
            id: rbText
            anchors.centerIn: parent
            visible: rb.icon === ""
            text: rb.label
            color: rb.active ? canvas.chromeText : canvas.chromeDim
            font.pixelSize: 11
            font.family: "monospace"
        }
        RailIcon {
            anchors.centerIn: parent
            visible: rb.icon !== ""
            name: rb.icon
            tint: rb.active ? canvas.chromeText : canvas.chromeDim
        }
        MouseArea {
            id: rbMouse
            anchors.fill: parent
            hoverEnabled: true
            onClicked: rb.clicked()
            onContainsMouseChanged: containsMouse ? canvas.showTip(rb, rb.tipString) : canvas.showTip(rb, "")
        }
    }

    component RailInput: Rectangle {
        id: ri
        property string value: ""
        property string placeholder: ""
        signal committed(string text)
        signal edited(string text)          // every keystroke, for live fields
        function setText(v) { if (!riEdit.activeFocus) riEdit.text = ("" + v) }
        implicitHeight: 21
        color: canvas.chromeSunk
        radius: 3
        border.width: 1
        border.color: riEdit.activeFocus ? canvas.chromeSel : canvas.chromeLine
        TextInput {
            id: riEdit
            anchors.fill: parent
            anchors.leftMargin: 5
            anchors.rightMargin: 5
            verticalAlignment: TextInput.AlignVCenter
            color: canvas.chromeText
            font.pixelSize: 11
            font.family: "monospace"
            selectByMouse: true
            clip: true
            onAccepted: ri.committed(riEdit.text)
            onTextChanged: if (riEdit.activeFocus) ri.edited(riEdit.text)
            Component.onCompleted: riEdit.text = ri.value
        }
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 5
            anchors.verticalCenter: parent.verticalCenter
            visible: riEdit.text.length === 0 && !riEdit.activeFocus
            text: ri.placeholder
            color: canvas.chromeDim
            font.pixelSize: 11
            font.family: "monospace"
        }
    }

    component RailLabel: Text {
        color: canvas.chromeDim
        font.pixelSize: 10
        font.family: "monospace"
        font.capitalization: Font.AllUppercase
        topPadding: 6
    }

    // One palette cell: the colour itself, its role/slot name and its hex, with
    // the label ink picked off the swatch's own luminance (`inkOn`).
    component Swatch: Rectangle {
        id: sw
        property string name: ""
        property color value
        implicitWidth: 68
        implicitHeight: 27
        radius: 2
        color: sw.value
        border.width: 1
        border.color: canvas.chromeLine
        Column {
            anchors.centerIn: parent
            spacing: 0
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: sw.name
                color: canvas.inkOn(sw.value)
                font.pixelSize: 9
                font.family: "monospace"
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: canvas.hexOf(sw.value)
                color: canvas.inkOn(sw.value)
                font.pixelSize: 9
                font.family: "monospace"
            }
        }
    }

    // ── The agent seam: `quickshell -p <root>/run/qml/WidgetPreview.qml ipc
    //    call preview <fn> …` ───────────────────────────────────────────────
    // AoideIpc.qml's precedent, one target per surface. `lyra preview shot`
    // and `lyra preview tree` drive exactly these three functions; the Rust
    // side owns the paths, the polling and the sidecar, so this end only picks
    // an item, grabs it and writes what it saw.
    // One annotation drawn on the overlay, in the widget's own coordinates:
    // the rect as stored (highlight/rect), an ellipse, a line, an arrow whose
    // head follows the SIGN of the stored w/h, or a dot for a bare note —
    // always the livery accent, at 40% once the note is done, numbered so the
    // screen and `lyra preview notes` name the same thing. A highlight whose
    // element no longer resolves is not drawn; the rail says "(unresolved)".
    component NoteMark: Item {
        id: nm
        property var note: ({})
        readonly property string kind: "" + (nm.note.kind || "note")
        readonly property string shape: "" + (nm.note.shape || "")
        readonly property var r: nm.note.rect || ({ "x": 0, "y": 0, "w": 0, "h": 0 })
        readonly property bool resolved: !nm.note.element || canvas.itemAt(nm.note.element) !== null
        readonly property real pad: 8
        visible: nm.resolved
        opacity: nm.note.done ? canvas.doneOpacity : 1.0
        x: Math.min(nm.r.x, nm.r.x + nm.r.w) - nm.pad
        y: Math.min(nm.r.y, nm.r.y + nm.r.h) - nm.pad
        width: Math.abs(nm.r.w) + 2 * nm.pad
        height: Math.abs(nm.r.h) + 2 * nm.pad

        Canvas {
            id: nmCv
            anchors.fill: parent
            // A Canvas does not repaint for a binding it never reads through a
            // property, so everything it paints with is held and watched.
            property color ink: canvas.chromeSel
            onInkChanged: nmCv.requestPaint()
            onWidthChanged: nmCv.requestPaint()
            onHeightChanged: nmCv.requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                ctx.strokeStyle = nmCv.ink
                ctx.fillStyle = nmCv.ink
                ctx.lineWidth = 2
                var p = nm.pad
                var w = Math.max(1, width - 2 * p), h = Math.max(1, height - 2 * p)
                if (nm.kind === "highlight" || nm.shape === "rect") {
                    ctx.strokeRect(p, p, w, h)
                } else if (nm.shape === "ellipse") {
                    ctx.beginPath()
                    ctx.ellipse(p, p, w, h)
                    ctx.stroke()
                } else if (nm.shape === "line" || nm.shape === "arrow") {
                    var x1 = (nm.r.w >= 0) ? p : p + w
                    var y1 = (nm.r.h >= 0) ? p : p + h
                    var x2 = (nm.r.w >= 0) ? p + w : p
                    var y2 = (nm.r.h >= 0) ? p + h : p
                    ctx.beginPath()
                    ctx.moveTo(x1, y1)
                    ctx.lineTo(x2, y2)
                    ctx.stroke()
                    if (nm.shape === "arrow") {
                        var a = Math.atan2(y2 - y1, x2 - x1), hl = 11
                        ctx.beginPath()
                        ctx.moveTo(x2, y2)
                        ctx.lineTo(x2 - hl * Math.cos(a - 0.42), y2 - hl * Math.sin(a - 0.42))
                        ctx.lineTo(x2 - hl * Math.cos(a + 0.42), y2 - hl * Math.sin(a + 0.42))
                        ctx.closePath()
                        ctx.fill()
                    }
                } else {
                    ctx.beginPath()
                    ctx.arc(p, p, 6, 0, 2 * Math.PI)
                    ctx.fill()
                }
            }
        }
        Rectangle {
            width: nmBadge.implicitWidth + 6
            height: 14
            radius: 2
            color: canvas.chromeSel
            Text {
                id: nmBadge
                anchors.centerIn: parent
                text: "#" + nm.note.n
                color: canvas.inkOn(canvas.chromeSel)
                font.pixelSize: 9
                font.family: "monospace"
            }
        }
    }

    IpcHandler {
        target: "preview"

        // The canvas grabs the WIDGET, whole, annotated or not. It does not
        // grab an element: a grab of, say, a Loader comes back fully
        // transparent (an 827-byte PNG), and an annotated element crop is not
        // expressible in QML at all. So `lyra preview shot --element` grabs
        // the widget through this same call and crops to the element's live
        // rect from `tree`, and `what=element` here is a refusal that names
        // the right call. The signature keeps its four arguments.
        //   what ∈ widget|element · element = a path or "-" · annotated on|off
        function shot(what: string, element: string, out: string, annotated: string): string {
            if (!out) return "error: no output path"
            if (what === "element") {
                canvas.shotFailed(out, "element shots are cropped by lyra preview shot — call with what=widget")
                return "error: what=element is cropped by lyra, not grabbed here"
            }
            var root = canvas.widgetItem
            if (!root) { canvas.shotFailed(out, "no widget loaded"); return "error: no widget loaded" }
            // stageLayer is the widget box plus the annotation overlay, the
            // same geometry, so on/off are pixel-aligned.
            var item = (annotated === "on") ? stageLayer : root
            canvas.log("ipc shot " + what + " annotated=" + annotated + " → " + out)
            return canvas.grabTo(item, out) ? "queued" : "error: grab refused"
        }

        function tree(out: string): string {
            if (!out) return "error: no output path"
            var root = canvas.widgetItem
            if (!root) return "error: no widget loaded"
            canvas.writeSidecar(out, JSON.stringify(canvas.treeOf(root), null, 2))
            canvas.log("ipc tree → " + out)
            return "ok"
        }

        function reload(): void {
            canvas.log("ipc reload")
            canvas.requestReload()
        }
    }

    // ── The window ─────────────────────────────────────────────────────────
    FloatingWindow {
        id: win
        title: "aoide-widget-preview"
        implicitWidth: 1400
        implicitHeight: 900
        color: canvas.chromeBg

        // Space is the temporary hand. Key events reach the focus item and
        // its ancestors only, so a focused TextInput keeps its space and this
        // item — the focus item whenever no field has it — sees the rest.
        // Losing active focus (a field, or the window going inactive) drops
        // the hand.
        Item {
            id: keyRoot
            anchors.fill: parent
            focus: true
            Keys.onPressed: function (e) {
                if (e.key !== Qt.Key_Space || e.isAutoRepeat) return
                canvas.handHeld = true
                canvas.log("hand: held (Space)")
                e.accepted = true
            }
            Keys.onReleased: function (e) {
                if (e.key !== Qt.Key_Space || e.isAutoRepeat) return
                canvas.log("hand: released")
                canvas.handHeld = false
                canvas.camDragging = false
                e.accepted = true
            }
            onActiveFocusChanged: if (!activeFocus) { canvas.handHeld = false; canvas.camDragging = false }
        }

        Rectangle {
            id: tipBox
            z: 100
            visible: canvas.tipText !== "" && tipDelay.ready
            x: Math.max(0, Math.min(canvas.tipX, win.width - width - 4))
            y: canvas.tipY
            width: tipLabel.implicitWidth + 12
            height: tipLabel.implicitHeight + 8
            radius: 3
            color: canvas.chromePanel
            border.width: 1
            border.color: canvas.chromeLine
            Text {
                id: tipLabel
                anchors.centerIn: parent
                text: canvas.tipText
                color: canvas.chromeText
                font.pixelSize: 11
                font.family: "monospace"
            }
            Timer {
                id: tipDelay
                property bool ready: false
                interval: 500
                onTriggered: tipDelay.ready = true
            }
            Connections {
                target: canvas
                function onTipTextChanged() {
                    tipDelay.ready = false
                    if (canvas.tipText !== "") tipDelay.restart(); else tipDelay.stop()
                }
            }
        }

        // ── Left rail ──────────────────────────────────────────────────────
        Rectangle {
            id: rail
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 300
            color: canvas.chromePanel

            Flickable {
                anchors.fill: parent
                anchors.margins: 8
                contentWidth: width
                contentHeight: railColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: railColumn
                    width: parent.width
                    spacing: 4

                    Text {
                        text: "widget preview canvas"
                        color: canvas.chromeText
                        font.pixelSize: 12
                        font.family: "monospace"
                        font.bold: true
                    }
                    Text {
                        width: railColumn.width
                        text: "root " + canvas.aoideRoot
                        color: canvas.chromeDim
                        font.pixelSize: 9
                        font.family: "monospace"
                        elide: Text.ElideMiddle
                    }

                    // 1 ── widget ──────────────────────────────────────────
                    RailLabel { text: "widget" }
                    RailInput {
                        id: widgetField
                        width: railColumn.width
                        value: canvas.widgetPath
                        placeholder: "songs/<song>/<slot>.qml"
                        onCommitted: function (text) { canvas.widgetPath = text.trim() }
                    }
                    Row {
                        spacing: 4
                        RailButton { label: "Browse…"; icon: "folder"; tip: "browse for a widget file (opens at the current widget's folder)"; onClicked: canvas.openWidgetDialog() }
                        RailButton { label: "Reload"; icon: "refresh-double"; tip: "reload — refresh the copies and Quickshell.reload"; onClicked: canvas.requestReload() }
                        RailButton { label: "Recreate"; onClicked: { canvas.log("recreate widget"); canvas.rebuildWidget() } }
                    }
                    Row {
                        spacing: 4
                        RailButton {
                            label: "auto-reload"
                            active: canvas.autoReload
                            onClicked: canvas.autoReload = !canvas.autoReload
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: canvas.watchPaths.length + " watched"
                            color: canvas.chromeDim
                            font.pixelSize: 10
                            font.family: "monospace"
                        }
                    }
                    RailButton {
                        label: canvas.declareArmed ? "Declare → click again to write into the checkout song"
                                                   : "Declare"
                        active: canvas.declareArmed
                        onClicked: canvas.declare()
                    }

                    // 2 ── song ────────────────────────────────────────────
                    RailLabel { text: "song" }
                    RailInput {
                        id: songField
                        width: railColumn.width
                        value: canvas.songName
                        placeholder: "sonata"
                        onCommitted: function (text) { canvas.songName = text.trim() }
                    }
                    Text {
                        width: railColumn.width
                        text: "livery song: " + (canvas.liveryState.songName === "" ? "(none)" : canvas.liveryState.songName)
                             + " — slots inside the widget resolve off THAT, not this field"
                        color: canvas.chromeDim
                        font.pixelSize: 9
                        font.family: "monospace"
                        wrapMode: Text.WordWrap
                    }

                    // 3 ── widget size ─────────────────────────────────────
                    RailLabel { text: "widget size" }
                    Row {
                        spacing: 4
                        RailInput {
                            id: wField
                            width: 74
                            value: canvas.axisText(canvas.widgetWidth)
                            onCommitted: function (text) { canvas.setWidgetWidth(text) }
                        }
                        RailInput {
                            id: hField
                            width: 74
                            value: canvas.axisText(canvas.widgetHeight)
                            onCommitted: function (text) { canvas.setWidgetHeight(text) }
                        }
                        RailButton {
                            label: "lock"
                            icon: canvas.widgetAspectLock ? "lock" : "lock-slash"
                            tip: canvas.widgetAspectLock ? "widget aspect locked — press to unlock" : "widget aspect unlocked — press to lock"
                            active: canvas.widgetAspectLock
                            onClicked: canvas.widgetAspectLock = !canvas.widgetAspectLock
                        }
                    }
                    Row {
                        spacing: 4
                        RailButton { label: "auto"; active: canvas.widgetWidth === "auto"; onClicked: canvas.setWidgetWidth("auto") }
                        RailButton { label: "240"; active: canvas.widgetWidth === 240; onClicked: canvas.setWidgetWidth(240) }
                        RailButton { label: "360"; active: canvas.widgetWidth === 360; onClicked: canvas.setWidgetWidth(360) }
                        RailButton { label: "720"; active: canvas.widgetWidth === 720; onClicked: canvas.setWidgetWidth(720) }
                    }

                    // 4 ── anchor ──────────────────────────────────────────
                    RailLabel { text: "anchor + margin" }
                    Row {
                        spacing: 8
                        Grid {
                            columns: 3
                            spacing: 3
                            Repeater {
                                model: [
                                    { "s": "tl", "i": "arrow-up-left",   "t": "top-left" },
                                    { "s": "t",  "i": "arrow-up",        "t": "top" },
                                    { "s": "tr", "i": "arrow-up-right",  "t": "top-right" },
                                    { "s": "l",  "i": "arrow-left",      "t": "left" },
                                    { "s": "c",  "i": "position",        "t": "centre" },
                                    { "s": "r",  "i": "arrow-right",     "t": "right" },
                                    { "s": "bl", "i": "arrow-down-left", "t": "bottom-left" },
                                    { "s": "b",  "i": "arrow-down",      "t": "bottom" },
                                    { "s": "br", "i": "arrow-down-right","t": "bottom-right" }
                                ]
                                RailButton {
                                    required property var modelData
                                    label: modelData.s
                                    icon: modelData.i
                                    tip: "anchor " + modelData.t + " (" + modelData.s + ")"
                                    active: canvas.anchorSpot === modelData.s
                                    onClicked: canvas.anchorSpot = modelData.s
                                }
                            }
                        }
                        Column {
                            spacing: 3
                            Text { text: "margin"; color: canvas.chromeDim; font.pixelSize: 10; font.family: "monospace" }
                            RailInput {
                                id: marginField
                                width: 60
                                value: "" + canvas.margin
                                onCommitted: function (text) { canvas.setMargin(text) }
                            }
                        }
                    }

                    // 5 ── viewport ────────────────────────────────────────
                    RailLabel { text: "viewport" }
                    Flow {
                        width: railColumn.width
                        spacing: 4
                        RailButton { label: "16:9"; active: canvas.viewportPreset === "16:9"; onClicked: canvas.setPreset("16:9", 1920, 1080) }
                        RailButton { label: "16:10"; active: canvas.viewportPreset === "16:10"; onClicked: canvas.setPreset("16:10", 1920, 1200) }
                        RailButton { label: "4:3"; active: canvas.viewportPreset === "4:3"; onClicked: canvas.setPreset("4:3", 1600, 1200) }
                        RailButton { label: "ultrawide"; active: canvas.viewportPreset === "ultrawide"; onClicked: canvas.setPreset("ultrawide", 2560, 1080) }
                        RailButton { label: "portrait"; active: canvas.viewportPreset === "portrait"; onClicked: canvas.setPreset("portrait", 1080, 1920) }
                    }
                    Row {
                        spacing: 4
                        RailInput {
                            id: vwField
                            width: 74
                            value: "" + canvas.viewportWidth
                            onCommitted: function (text) { canvas.setViewportWidth(text) }
                        }
                        RailInput {
                            id: vhField
                            width: 74
                            value: "" + canvas.viewportHeight
                            onCommitted: function (text) { canvas.setViewportHeight(text) }
                        }
                        RailButton {
                            label: "lock"
                            icon: canvas.viewportAspectLock ? "lock" : "lock-slash"
                            tip: canvas.viewportAspectLock ? "viewport aspect locked — press to unlock" : "viewport aspect unlocked — press to lock"
                            active: canvas.viewportAspectLock
                            onClicked: canvas.viewportAspectLock = !canvas.viewportAspectLock
                        }
                    }

                    // 6 ── zoom ────────────────────────────────────────────
                    RailLabel { text: "zoom" }
                    Row {
                        spacing: 4
                        RailButton { label: "fit"; icon: "expand"; tip: "fit — whole viewport in the frame, camera reset"; active: canvas.zoomSetting === "fit"; onClicked: canvas.setZoom("fit") }
                        RailButton { label: "1:1"; tip: "1:1 — widget pixels, camera reset"; active: canvas.zoomSetting === 1; onClicked: canvas.setZoom(1) }
                        RailButton { label: "zoom out"; icon: "zoom-out"; tip: "zoom out about the frame centre (Ctrl+wheel: about the pointer)"; onClicked: canvas.zoomAt(stageFrame.width / 2, stageFrame.height / 2, 1 / 1.25) }
                        RailButton { label: "zoom in"; icon: "zoom-in"; tip: "zoom in about the frame centre (Ctrl+wheel: about the pointer)"; onClicked: canvas.zoomAt(stageFrame.width / 2, stageFrame.height / 2, 1.25) }
                        RailInput {
                            id: zoomField
                            width: 58
                            value: canvas.zoomSetting === "fit" ? "fit" : ("" + canvas.zoomSetting)
                            onCommitted: function (text) { canvas.setZoom(text) }
                        }
                    }
                    Text {
                        text: "effective ×" + canvas.effScale.toFixed(3)
                            + (canvas.zoomSetting === "fit" ? "  (fit, capped at 1.0)" : "")
                        color: canvas.chromeDim
                        font.pixelSize: 10
                        font.family: "monospace"
                    }

                    // 7 ── background ──────────────────────────────────────
                    RailLabel { text: "background" }
                    Row {
                        spacing: 4
                        RailButton { label: "livery"; active: canvas.background === "livery"; onClicked: canvas.background = "livery" }
                        RailButton { label: "checker"; active: canvas.background === "checker"; onClicked: canvas.background = "checker" }
                    }

                    // 8 ── fixture + livery (the shell seam) ───────────────
                    RailLabel { text: "fixture  (lyra preview set)" }
                    Flow {
                        width: railColumn.width
                        spacing: 4
                        Repeater {
                            model: canvas.fixtures
                            RailButton {
                                required property string modelData
                                label: modelData
                                active: canvas.fixture === modelData
                                onClicked: canvas.previewSet(["--fixture", modelData], "--fixture " + modelData)
                            }
                        }
                    }
                    Text {
                        visible: canvas.fixtures.length === 0
                        width: railColumn.width
                        text: "no fixtures listed in preview.json"
                        color: canvas.chromeDim
                        font.pixelSize: 10
                        font.family: "monospace"
                    }
                    // 9 ── palette (the same shell seam) ───────────────────
                    // A palette spec is a FILE RESOLUTION (song livery, live
                    // stage, a base16 scheme), so it goes the same way the
                    // fixture does: one `lyra preview set --livery <spec>`, and
                    // LiveryState's own watch on ROOT/song/stage/livery.json
                    // recolours the widget AND this rail in place. No reload,
                    // and no palette parsing in QML.
                    RailLabel { text: "palette  (lyra preview set --livery)" }
                    Flow {
                        width: railColumn.width
                        spacing: 4
                        RailButton {
                            label: "live"
                            active: canvas.liverySource === "live"
                            onClicked: canvas.previewSet(["--livery", "live"], "--livery live")
                        }
                        Repeater {
                            model: canvas.liveries
                            RailButton {
                                required property string modelData
                                label: modelData
                                active: canvas.liverySource === modelData
                                onClicked: canvas.previewSet(["--livery", modelData], "--livery " + modelData)
                            }
                        }
                    }
                    RailInput {
                        id: liveryField
                        width: railColumn.width
                        value: canvas.liverySource
                        placeholder: "song name, /abs/livery.json, base16.yaml"
                        onCommitted: function (text) {
                            if (text.trim().length > 0) canvas.previewSet(["--livery", text.trim()], "--livery " + text.trim())
                        }
                    }
                    Row {
                        spacing: 4
                        RailButton { label: "Browse…"; onClicked: liveryDialog.open() }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "source: " + (canvas.liverySource === "" ? "(unset)" : canvas.liverySource)
                            color: canvas.chromeDim
                            font.pixelSize: 10
                            font.family: "monospace"
                            width: railColumn.width - 80
                            elide: Text.ElideMiddle
                        }
                    }

                    RailLabel { text: "palette roles" }
                    Flow {
                        width: railColumn.width
                        spacing: 3
                        Repeater {
                            model: canvas.roleSwatches
                            Swatch {
                                required property var modelData
                                name: modelData.name
                                value: modelData.value
                            }
                        }
                    }

                    RailLabel { text: "base16 tier" }
                    Flow {
                        width: railColumn.width
                        spacing: 3
                        Repeater {
                            model: canvas.base16Swatches
                            Swatch {
                                required property var modelData
                                name: modelData.name
                                value: modelData.value
                            }
                        }
                    }
                    Text {
                        visible: canvas.base16Swatches.length === 0
                        width: railColumn.width
                        text: "no base16 tier in this livery"
                        color: canvas.chromeDim
                        font.pixelSize: 10
                        font.family: "monospace"
                    }

                    // 12 ── annotate (the work list `lyra preview notes` reads)
                    // A mode arms the overlay; `pick` hit-tests the deepest
                    // item under the pointer and a click attaches the text
                    // field to its element path, so a note points an agent at
                    // one item rather than at a screenshot.
                    RailLabel { text: "annotate  (lyra preview notes)" }
                    Flow {
                        width: railColumn.width
                        spacing: 4
                        Repeater {
                            model: [
                                { "m": "off", "i": "xmark", "t": "off — no annotation tool" },
                                { "m": "pick", "i": "cursor-pointer", "t": "pick — click an element to highlight it by path" },
                                { "m": "rect", "i": "square", "t": "rect — drag a box" },
                                { "m": "ellipse", "i": "circle", "t": "ellipse — drag a box" },
                                { "m": "arrow", "i": "arrow-up-right", "t": "arrow — drag from tail to head" },
                                { "m": "line", "i": "minus", "t": "line — drag" },
                                { "m": "note", "i": "notes", "t": "note — click to pin the note text" }
                            ]
                            RailButton {
                                required property var modelData
                                label: modelData.m
                                icon: modelData.i
                                tip: modelData.t
                                active: canvas.annotMode === modelData.m && !canvas.handHeld
                                onClicked: { canvas.annotMode = modelData.m; canvas.clearHover() }
                            }
                        }
                        // The hand is a mask over the tool above, not a tool:
                        // lit while Space is held or a middle-drag is on, and
                        // a click makes it sticky until Space is released.
                        RailButton {
                            label: "hand"
                            icon: "drag-hand-gesture"
                            tip: "hand — hold Space or middle-drag to pan; click to hold it"
                            active: canvas.handHeld || canvas.camDragging
                            onClicked: { canvas.handHeld = !canvas.handHeld; keyRoot.forceActiveFocus() }
                        }
                    }
                    RailInput {
                        width: railColumn.width
                        placeholder: "note text — attached to the next annotation"
                        value: canvas.noteText
                        onEdited: function (t) { canvas.noteText = t }
                        onCommitted: function (t) { canvas.noteText = t }
                    }
                    Text {
                        width: railColumn.width
                        visible: canvas.annotMode === "pick"
                        text: canvas.hoverPath === "" ? "hover the widget to pick an element"
                                                      : canvas.hoverLabel() + "\n" + canvas.hoverPath
                        wrapMode: Text.WrapAnywhere
                        color: canvas.chromeDim
                        font.pixelSize: 10
                        font.family: "monospace"
                    }
                    Text {
                        width: railColumn.width
                        visible: canvas.notes.length === 0
                        text: "no notes yet — ROOT/notes.json"
                        color: canvas.chromeDim
                        font.pixelSize: 10
                        font.family: "monospace"
                    }
                    Repeater {
                        model: canvas.notes
                        Rectangle {
                            id: noteRow
                            required property var modelData
                            readonly property bool resolved:
                                !noteRow.modelData.element || canvas.itemAt(noteRow.modelData.element) !== null
                            width: railColumn.width
                            height: noteBody.implicitHeight + 10
                            radius: 3
                            color: canvas.chromeSunk
                            border.width: 1
                            border.color: noteRow.modelData.done ? canvas.chromeLine : canvas.fade(canvas.chromeSel, 0.55)
                            opacity: noteRow.modelData.done ? canvas.doneOpacity : 1.0
                            Column {
                                id: noteBody
                                anchors.left: parent.left
                                anchors.right: noteBtns.left
                                anchors.leftMargin: 6
                                anchors.rightMargin: 4
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 1
                                Text {
                                    width: parent.width
                                    text: "#" + noteRow.modelData.n + " · " + noteRow.modelData.kind
                                        + (noteRow.modelData.shape ? " " + noteRow.modelData.shape : "")
                                    color: canvas.chromeText
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                }
                                Text {
                                    width: parent.width
                                    visible: !!noteRow.modelData.element
                                    text: (noteRow.resolved ? "" : "(unresolved) ") + (noteRow.modelData.element || "")
                                    wrapMode: Text.WrapAnywhere
                                    color: noteRow.resolved ? canvas.chromeDim : canvas.chromeErr
                                    font.pixelSize: 9
                                    font.family: "monospace"
                                }
                                Text {
                                    width: parent.width
                                    visible: ("" + noteRow.modelData.text).length > 0
                                    text: "" + noteRow.modelData.text
                                    wrapMode: Text.WordWrap
                                    color: canvas.chromeText
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                }
                            }
                            Row {
                                id: noteBtns
                                anchors.right: parent.right
                                anchors.rightMargin: 5
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 3
                                RailButton {
                                    label: noteRow.modelData.done ? "undo" : "done"
                                    onClicked: canvas.setNoteDone(noteRow.modelData.n, !noteRow.modelData.done)
                                }
                                RailButton {
                                    label: "del"
                                    icon: "xmark"
                                    tip: "Delete"
                                    onClicked: canvas.deleteNote(noteRow.modelData.n)
                                }
                            }
                        }
                    }

                    Item { width: 1; height: 8 }
                }
            }
        }

        // ── Stage pane ─────────────────────────────────────────────────────
        Item {
            id: stagePane
            anchors.left: rail.right
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: bottomPanes.top

            Text {
                id: caption
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.margins: 8
                color: canvas.chromeDim
                font.pixelSize: 11
                font.family: "monospace"
                text: {
                    var it = canvas.widgetItem
                    var iw = (it && it.implicitWidth !== undefined) ? Math.round(it.implicitWidth) : 0
                    var ih = (it && it.implicitHeight !== undefined) ? Math.round(it.implicitHeight) : 0
                    return Math.round(canvas.boxW) + "×" + Math.round(canvas.boxH)
                        + " (implicit " + iw + "×" + ih + ")"
                        + " @ " + canvas.anchorSpot + "+" + canvas.margin
                        + "  ·  viewport " + canvas.viewportWidth + "×" + canvas.viewportHeight
                        + " " + canvas.viewportPreset
                        + "  ·  ×" + canvas.effScale.toFixed(3)
                }
            }

            // The camera frame: `viewport` at (camX, camY), scaled, clipped
            // here. Everything the pointer can touch on the stage is a child
            // of `viewport`, so the widget and the overlay share the one
            // transform.
            Item {
                id: stageFrame
                anchors.top: caption.bottom
                anchors.topMargin: 6
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 8
                clip: true

                // Bottom of the stack: a click that nothing above took
                // moves keyboard focus out of the rail's fields so Space
                // becomes the hand again.
                MouseArea {
                    anchors.fill: parent
                    onPressed: function (m) { keyRoot.forceActiveFocus(); m.accepted = false }
                }

                Rectangle {
                    id: viewport
                    x: canvas.camX
                    y: canvas.camY
                    width: canvas.viewportWidth
                    height: canvas.viewportHeight
                    clip: true
                    transformOrigin: Item.TopLeft
                    scale: canvas.effScale
                    // The two greys below are the checkerboard, and the ONLY
                    // colour literals in this file: a transparency backdrop
                    // that took its colour from the livery would stop being a
                    // transparency backdrop.
                    color: canvas.background === "livery" ? canvas.liveryState.paletteBg : "#ffffff"

                    // 8px checkerboard, so a transparent widget edge is visible.
                    Canvas {
                        anchors.fill: parent
                        visible: canvas.background === "checker"
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            ctx.fillStyle = "#ffffff"
                            ctx.fillRect(0, 0, width, height)
                            ctx.fillStyle = "#cfcfcf"
                            var s = 8
                            for (var y = 0; y < height; y += s) {
                                for (var x = ((y / s) % 2 === 0) ? 0 : s; x < width; x += 2 * s)
                                    ctx.fillRect(x, y, s, s)
                            }
                        }
                    }

                    // The widget and its annotation overlay share one box, so
                    // that a note drawn at (x,y) is at (x,y) in the widget's
                    // own coordinates and `shot --annotated` can grab this one
                    // item and get exactly what is on screen.
                    Item {
                        id: stageLayer
                        width: canvas.boxW
                        height: canvas.boxH
                        x: canvas.anchorX(width)
                        y: canvas.anchorY(height)

                        Item {
                            id: widgetHost
                            anchors.fill: parent
                        }

                        Item {
                            id: annot
                            anchors.fill: parent

                            Repeater {
                                model: canvas.notes
                                NoteMark {
                                    required property var modelData
                                    note: modelData
                                }
                            }

                            // pick mode: the deepest item under the pointer.
                            Rectangle {
                                visible: canvas.annotMode === "pick" && canvas.hoverItem !== null
                                x: canvas.hoverRect.x
                                y: canvas.hoverRect.y
                                width: canvas.hoverRect.width
                                height: canvas.hoverRect.height
                                color: canvas.fade(canvas.chromeSel, 0.12)
                                border.width: 2
                                border.color: canvas.chromeSel
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.bottom: parent.top
                                    width: hoverText.implicitWidth + 8
                                    height: 15
                                    color: canvas.chromeSel
                                    Text {
                                        id: hoverText
                                        anchors.centerIn: parent
                                        text: canvas.hoverLabel()
                                        color: canvas.inkOn(canvas.chromeSel)
                                        font.pixelSize: 10
                                        font.family: "monospace"
                                    }
                                }
                            }

                            // the shape being dragged, before it becomes a note:
                            // rect/ellipse show their box, arrow/line the stroke itself
                            Rectangle {
                                visible: canvas.dragging && !canvas.dragIsStroke
                                x: Math.min(canvas.dragX1, canvas.dragX2)
                                y: Math.min(canvas.dragY1, canvas.dragY2)
                                width: Math.abs(canvas.dragX2 - canvas.dragX1)
                                height: Math.abs(canvas.dragY2 - canvas.dragY1)
                                color: canvas.fade(canvas.chromeSel, 0.10)
                                border.width: 2
                                border.color: canvas.chromeSel
                                radius: canvas.annotMode === "ellipse" ? Math.min(width, height) / 2 : 0
                            }
                            Canvas {
                                id: dragStroke
                                anchors.fill: parent
                                visible: canvas.dragging && canvas.dragIsStroke
                                property real ax: canvas.dragX1
                                property real ay: canvas.dragY1
                                property real bx: canvas.dragX2
                                property real by: canvas.dragY2
                                property bool arrow: canvas.annotMode === "arrow"
                                property color ink: canvas.chromeSel
                                onAxChanged: dragStroke.requestPaint()
                                onAyChanged: dragStroke.requestPaint()
                                onBxChanged: dragStroke.requestPaint()
                                onByChanged: dragStroke.requestPaint()
                                onVisibleChanged: dragStroke.requestPaint()
                                onPaint: {
                                    var ctx = getContext("2d")
                                    ctx.reset()
                                    if (!dragStroke.visible) return
                                    ctx.strokeStyle = dragStroke.ink
                                    ctx.fillStyle = dragStroke.ink
                                    ctx.lineWidth = 2
                                    ctx.beginPath()
                                    ctx.moveTo(dragStroke.ax, dragStroke.ay)
                                    ctx.lineTo(dragStroke.bx, dragStroke.by)
                                    ctx.stroke()
                                    if (!dragStroke.arrow) return
                                    var a = Math.atan2(dragStroke.by - dragStroke.ay, dragStroke.bx - dragStroke.ax), hl = 11
                                    ctx.beginPath()
                                    ctx.moveTo(dragStroke.bx, dragStroke.by)
                                    ctx.lineTo(dragStroke.bx - hl * Math.cos(a - 0.42), dragStroke.by - hl * Math.sin(a - 0.42))
                                    ctx.lineTo(dragStroke.bx - hl * Math.cos(a + 0.42), dragStroke.by - hl * Math.sin(a + 0.42))
                                    ctx.closePath()
                                    ctx.fill()
                                }
                            }

                            // Off means OFF: the widget below keeps its own
                            // hover and clicks (that is how the stub bridge
                            // gets exercised), so this only takes the pointer
                            // while a mode is armed.
                            MouseArea {
                                id: annotMouse
                                anchors.fill: parent
                                enabled: canvas.annotMode !== "off"
                                hoverEnabled: canvas.annotMode === "pick"
                                acceptedButtons: Qt.LeftButton
                                onExited: canvas.clearHover()
                                onCanceled: canvas.dragging = false
                                onPositionChanged: function (m) {
                                    if (canvas.annotMode === "pick") { canvas.hoverAt(m.x, m.y); return }
                                    if (canvas.dragging) { canvas.dragX2 = m.x; canvas.dragY2 = m.y }
                                }
                                onPressed: function (m) {
                                    keyRoot.forceActiveFocus()
                                    if (canvas.annotMode === "pick" || canvas.annotMode === "note") return
                                    canvas.dragging = true
                                    canvas.dragX1 = m.x; canvas.dragY1 = m.y
                                    canvas.dragX2 = m.x; canvas.dragY2 = m.y
                                }
                                onReleased: function (m) {
                                    if (canvas.annotMode === "pick") { canvas.pickAt(m.x, m.y); return }
                                    if (canvas.annotMode === "note") {
                                        canvas.addNote("note", "", { "x": m.x, "y": m.y, "w": 0, "h": 0 },
                                                       "", canvas.noteText)
                                        return
                                    }
                                    if (!canvas.dragging) return
                                    canvas.dragging = false
                                    var r = canvas.dragRect()
                                    if (Math.abs(r.w) < 3 && Math.abs(r.h) < 3) return
                                    canvas.addNote("shape", canvas.annotMode, r, "", canvas.noteText)
                                }
                            }
                        }
                    }

                    // Thin dashed outline around the widget box.
                    Canvas {
                        id: outline
                        // A livery swap changes the dash colour, and a Canvas
                        // does not repaint on a binding it never reads through
                        // a property — so the colour is held here and watched.
                        property color dash: canvas.chromeSel
                        onDashChanged: outline.requestPaint()
                        x: stageLayer.x - 1
                        y: stageLayer.y - 1
                        width: stageLayer.width + 2
                        height: stageLayer.height + 2
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            ctx.strokeStyle = outline.dash
                            ctx.lineWidth = 1
                            ctx.setLineDash([4, 4])
                            ctx.strokeRect(0.5, 0.5, width - 1, height - 1)
                        }
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()
                    }
                }

                // The hand, above the overlay: middle-drag always, left-drag
                // while the hand is held. A left press it does not accept
                // falls through to the overlay, so the tool in use is masked
                // by the hand, never changed by it. Ctrl+wheel zooms about
                // the pointer; any other wheel is left for the widget.
                MouseArea {
                    id: handArea
                    anchors.fill: parent
                    z: 10
                    acceptedButtons: canvas.handHeld ? (Qt.LeftButton | Qt.MiddleButton) : Qt.MiddleButton
                    cursorShape: canvas.camDragging ? Qt.ClosedHandCursor
                               : (canvas.handHeld ? Qt.OpenHandCursor : Qt.ArrowCursor)
                    property real lastX: 0
                    property real lastY: 0
                    onPressed: function (m) {
                        keyRoot.forceActiveFocus()
                        canvas.camDragging = true
                        handArea.lastX = m.x
                        handArea.lastY = m.y
                    }
                    onPositionChanged: function (m) {
                        if (!canvas.camDragging) return
                        canvas.camX += m.x - handArea.lastX
                        canvas.camY += m.y - handArea.lastY
                        handArea.lastX = m.x
                        handArea.lastY = m.y
                    }
                    onReleased: canvas.camDragging = false
                    onCanceled: canvas.camDragging = false
                    onWheel: function (w) {
                        if (!(w.modifiers & Qt.ControlModifier)) { w.accepted = false; return }
                        // A wheel notch is angleDelta 120; a touchpad or a
                        // virtual pointer may only carry pixelDelta.
                        var notches = w.angleDelta.y !== 0 ? w.angleDelta.y / 120 : w.pixelDelta.y / 40
                        canvas.zoomAt(w.x, w.y, Math.pow(1.1, notches))
                        w.accepted = true
                    }
                }
            }
        }

        // ── Error pane + action log ────────────────────────────────────────
        Column {
            id: bottomPanes
            anchors.left: rail.right
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            spacing: 0

            Rectangle {
                width: parent.width
                height: Math.max(30, errText.implicitHeight + 12)
                color: canvas.errorText === "" ? canvas.chromePanel
                                               : canvas.mix(canvas.chromePanel, canvas.chromeErr, 0.28)
                border.width: 1
                border.color: canvas.errorText === "" ? canvas.chromeLine : canvas.chromeErr
                Text {
                    id: errText
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.margins: 6
                    text: canvas.errorText === "" ? "no errors" : canvas.errorText
                    color: canvas.errorText === "" ? canvas.chromeDim
                                                   : canvas.mix(canvas.chromeErr, canvas.chromeText, 0.35)
                    font.pixelSize: 11
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                }
            }

            Rectangle {
                width: parent.width
                height: 150
                color: canvas.chromeSunk
                border.width: 1
                border.color: canvas.chromeLine
                ListView {
                    id: logView
                    anchors.fill: parent
                    anchors.margins: 5
                    clip: true
                    model: canvas.actionLog
                    spacing: 1
                    delegate: Text {
                        required property string modelData
                        width: logView.width
                        text: modelData
                        color: canvas.chromeText
                        font.pixelSize: 10
                        font.family: "monospace"
                        wrapMode: Text.WrapAnywhere
                    }
                    onCountChanged: logView.positionViewAtEnd()
                }
            }
        }

        // ── Native/portal pickers ──────────────────────────────────────────
        FileDialog {
            id: widgetDialog
            title: "Pick a widget .qml"
            nameFilters: ["QML widgets (*.qml)", "All files (*)"]
            onAccepted: {
                var p = ("" + widgetDialog.selectedFile).replace(/^file:\/\//, "")
                canvas.widgetPath = decodeURIComponent(p)
                canvas.syncFields()
                canvas.log("picked widget " + canvas.widgetPath)
            }
        }
        FileDialog {
            id: liveryDialog
            title: "Pick a livery.json"
            nameFilters: ["Livery / base16 scheme (*.json *.yaml *.yml)", "All files (*)"]
            onAccepted: {
                var p = decodeURIComponent(("" + liveryDialog.selectedFile).replace(/^file:\/\//, ""))
                canvas.previewSet(["--livery", p], "--livery")
            }
        }
    }
}
