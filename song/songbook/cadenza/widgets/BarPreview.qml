// BarPreview.qml — the canvas harness for cadenza's bar (preview only).
//
// A HELPER (uppercase — never a slot; sonata's AudioColonnadePreview.qml
// precedent). The canvas grabs an Item, and the bar's panes live in a
// PopupWindow, so this loads bar.qml BY URL with `inlinePanes: true` (the
// same pane components, drawn under the bar inside this item) and leaves
// room below the line for them.
//
// What pane is open is steered from the preview ROOT, never a fixture path:
// `$AOIDE_ROOT/bar-preview.json` (optional; absent = no pane)
//     { "pane": "vol"|"bt"|"net"|"bat"|"tray"|"clock"|"jack"|"",
//       "jack": 2,            // with pane "jack": whose insight pane
//       "lampMs": 600 }       // slow the lamp down to catch one mid-run
// Everything else the bar reads, it reads itself (the root's stage files,
// state/usage/now.json, Hyprland, the services) — this harness feeds it
// nothing, so the live path and the preview path are the same bar.
//
//   AOIDE_FLAKE_ROOT=<worktree> lyra preview \
//       <worktree>/song/songbook/cadenza/widgets/BarPreview.qml --song cadenza \
//       --fixture <worktree>/song/songbook/cadenza/design/fixtures/switchboard \
//       --root $XDG_RUNTIME_DIR/cadenza-bar
import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: harness

    required property var livery
    required property var bridge
    property var shared: null
    property var stagingEngine: null

    implicitWidth: 1080
    implicitHeight: 28 + 420

    property var control: ({})
    FileView {
        id: ctl
        path: (Quickshell.env("AOIDE_ROOT") || "") + "/bar-preview.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            try { harness.control = JSON.parse(ctl.text()) } catch (e) { harness.control = ({}) }
            harness.apply()
        }
        onLoadFailed: { harness.control = ({}); harness.apply() }
        Component.onCompleted: reload()
    }

    function apply() {
        var b = barLoader.item
        if (!b) return
        var c = harness.control || {}
        b.lampMs = c.lampMs > 0 ? c.lampMs : 600
        b.closePane()
        retry.tries = 0
        if (c.pane === "jack") Qt.callLater(function () { b.previewJack(c.jack || 1) })
        else if (c.pane) Qt.callLater(function () { b.previewPane(c.pane) })
        if (c.pane) retry.restart()
    }
    // the jack row fills in when Hyprland's workspace list arrives, which can
    // land after the first apply(); retry briefly until the pane is up
    Timer {
        id: retry
        property int tries: 0
        interval: 250
        repeat: true
        onTriggered: {
            var b = barLoader.item, c = harness.control || {}
            if (!b || !c.pane || b.openPane !== "" || ++tries > 20) { stop(); return }
            if (c.pane === "jack") b.previewJack(c.jack || 1)
            else b.previewPane(c.pane)
        }
    }

    Loader {
        id: barLoader
        width: harness.width
        Component.onCompleted: setSource(Qt.resolvedUrl("bar.qml"), {
            livery: harness.livery,
            bridge: harness.bridge,
            shared: harness.shared,
            stagingEngine: harness.stagingEngine,
            powermenu: null,
            dock: null,
            inlinePanes: true
        })
        onLoaded: Qt.callLater(harness.apply)
    }
}
