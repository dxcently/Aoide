// BoardPreview.qml — PREVIEW-ONLY harness: the board with its unbuilt feeds
// filled from `design/fixtures/board/` (sonata's AudioColonnadePreview
// precedent). The live path never loads this file.
//
//   AOIDE_FLAKE_ROOT=<worktree> lyra preview \
//       <worktree>/song/songbook/cadenza/widgets/BoardPreview.qml --song cadenza \
//       --fixture <worktree>/song/songbook/cadenza/design/fixtures/board
//
// It loads BoardBody by URL and sets the SAME properties the live adapter
// will set once the seams land — `boardWired` + `boards` (the §D answer of
// `aoide project board`, S8–S10) and `usageNow` (§C now.json, S5/S6). The
// stage files (sessions/projects/hooks/herald) come in through `--fixture`
// as usual; BoardBody itself never reads a fixture path.
//
// The fixture directory: `$CADENZA_BOARD_FIXTURE`, else the checkout's
// `song/songbook/cadenza/design/fixtures/board` under `$AOIDE_FLAKE_ROOT`.
// The tab: clicking works as live. For scripted shots the harness also
// watches `<preview root>/board-tab` (one line: `overview`, `sys`, `notif`,
// `project:<name>`) — a file inside the isolated root, read by this harness
// only.
import QtQuick
import Quickshell
import Quickshell.Io
import "Kit.js" as Kit

Item {
    id: harness

    required property var livery
    required property var bridge
    property var shared: null
    property var stagingEngine: null

    readonly property string fixtureDir: {
        var d = Quickshell.env("CADENZA_BOARD_FIXTURE") || ""
        if (d) return d
        var flake = Quickshell.env("AOIDE_FLAKE_ROOT") || (Quickshell.env("HOME") + "/Aoide")
        return flake + "/song/songbook/cadenza/design/fixtures/board"
    }

    implicitWidth: body.item ? body.item.implicitWidth : 560
    implicitHeight: body.item ? body.item.implicitHeight : 1800

    property var boardAnswer: null
    property var nowAnswer: null

    FileView {
        id: boardFile
        path: harness.fixtureDir + "/board.json"
        blockLoading: false; printErrors: false
        onLoaded: { try { harness.boardAnswer = JSON.parse(text()) } catch (e) { console.warn("[BoardPreview] board.json:", e) } }
    }
    FileView {
        id: nowFile
        path: harness.fixtureDir + "/now.json"
        blockLoading: false; printErrors: false
        onLoaded: { try { harness.nowAnswer = JSON.parse(text()) } catch (e) { console.warn("[BoardPreview] now.json:", e) } }
    }
    FileView {
        id: tabFile
        path: (Quickshell.env("AOIDE_ROOT") || "/nonexistent") + "/board-tab"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: harness.applyTab()
    }
    function applyTab() {
        var t = ("" + (tabFile.text() || "")).trim()
        if (t && body.item) body.item.openTab(t)
    }

    Loader {
        id: body
        anchors.fill: parent
        Component.onCompleted: setSource(Kit.helper("BoardBody"), {
            livery: Qt.binding(() => harness.livery),
            bridge: Qt.binding(() => harness.bridge),
            shared: Qt.binding(() => harness.shared),
            stagingEngine: Qt.binding(() => harness.stagingEngine),
            open: true,
            boardWired: Qt.binding(() => harness.boardAnswer !== null),
            boards: Qt.binding(function () {
                var b = {}
                if (harness.boardAnswer && harness.boardAnswer.project)
                    b[harness.boardAnswer.project] = harness.boardAnswer
                return b
            }),
            usageNow: Qt.binding(() => harness.nowAnswer)
        })
        onLoaded: harness.applyTab()
    }
}
