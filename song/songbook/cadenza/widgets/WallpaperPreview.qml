// WallpaperPreview.qml — the canvas harness for cadenza's live board
// (preview only).
//
// A HELPER (uppercase — never a slot; BarPreview.qml's precedent). The board
// fills the whole surface, so the canvas needs something with a viewport of
// its own: this item declares `implicitWidth/Height` (1920×1080 by default,
// the canvas' `lyra preview set --viewport` overrides the shoot), and loads
// `wallpaper.qml` BY URL, handing it the canvas' livery/bridge plus the knobs
// below.
//
// The FEED is real, not faked: `AOIDE_ROOT` points at the preview root, and
// `lyra preview --fixture <dir>` stages that directory into
// `<root>/state/stage/` — so `--fixture …/design/fixtures/board` gives the
// board the fixture agents (working · awaiting · idle) and the fixture hook
// stamps, and the pulses answer exactly as they would live. Nothing here reads
// a fixture path itself.
//
// Knobs — `$AOIDE_ROOT/wallpaper-preview.json` (optional; absent = live values)
//     { "rate": 0.25,      // clock multiplier: 0.25 catches a slow leg mid-run
//       "interval": 33,    // how often the light MOVES (ms) — not a repaint cost
//       "pulseCap": 14,    // the most light the board may carry
//       "idleFloor": 2,    // light with NOTHING running
//       "seed": 7 }        // 0/absent: seeded from the clock (each run differs)
//
//   AOIDE_FLAKE_ROOT=<worktree> lyra preview \
//       <worktree>/song/songbook/cadenza/widgets/WallpaperPreview.qml \
//       --song cadenza \
//       --fixture <worktree>/song/songbook/cadenza/design/fixtures/board \
//       --root $XDG_RUNTIME_DIR/cadenza-wall
import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: harness

    required property var livery
    required property var bridge
    // the canvas hands every slot's extras; accepted so none warns, unused.
    property var shared: null
    property var stagingEngine: null
    property var powermenu: null
    property var dock: null
    property var clipboard: null
    property var ledger: null
    property var stagePath: null

    implicitWidth: 1920
    implicitHeight: 1080

    property var control: ({})
    FileView {
        id: ctl
        path: (Quickshell.env("AOIDE_ROOT") || "") + "/wallpaper-preview.json"
        watchChanges: true
        printErrors: false
        onFileChanged: ctl.reload()
        onTextChanged: {
            try { harness.control = JSON.parse(ctl.text()) } catch (e) { harness.control = ({}) }
            harness.apply()
        }
        onLoadFailed: { harness.control = ({}); harness.apply() }
        Component.onCompleted: reload()
    }

    function apply() {
        var b = boardLoader.item
        if (!b) return
        var c = harness.control || {}
        b.rate = c.rate > 0 ? c.rate : 1.0
        b.interval = c.interval > 0 ? c.interval : 33
        b.pulseCap = c.pulseCap > 0 ? c.pulseCap : 14
        b.idleFloor = c.idleFloor >= 0 ? c.idleFloor : 2
        b.seedWanted = c.seed || 0
    }

    Loader {
        id: boardLoader
        anchors.fill: parent
        Component.onCompleted: setSource(Qt.resolvedUrl("wallpaper.qml"), {
            livery: harness.livery,
            bridge: harness.bridge
        })
        onLoaded: harness.apply()
    }
}
