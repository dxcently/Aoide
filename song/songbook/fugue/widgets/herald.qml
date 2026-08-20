// herald.qml -- fugue's "herald" slot: the notification popup, hosted by
// SurfaceSlot (shell.qml's heraldSlot), so this root is a PanelWindow that
// owns its own layer surface -- hidden while the ledger is empty.
//
// Mechanism only borrowed from sonata's herald.qml (read as reference, not
// copied): read-only song/stage/herald.json via FileView, newest last, a
// LOCAL dismiss clock keyed by id (0 timeoutMs = never expires), and two
// outbound commands over the bridge socket -- heralddismiss / heraldverdict
// -- never a file write from QML (CONTRACTS.md section 4).
//
// Grammar is unmistakably fugue's, not sonata's (design/intent.md "Why a
// screenshot can never be confused with sonata"): each record draws as a
// stack of Cell instances (Cell.qml, this song's own uppercase helper) --
// flat rectangles, hairline separators, a 2px rule for the live/urgent
// register, no marble, no crown glyph, no cast shadow.
//
// No Process, no execDetached, no shelling out to any binary from this
// file (hard constraint) -- outbound traffic is the bridge socket only.
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

PanelWindow {
    id: root

    // -- Injected props -- exactly what shell.qml's heraldSlot hands over ---
    // (shell.qml: SurfaceSlot { slot: "herald" }, no extraProps configured)
    required property var livery
    required property var bridge

    readonly property int cardW: 260
    readonly property int stackGap: 6
    readonly property int maxCards: 5

    // -- The ledger file -- read-only, watched -------------------------------
    readonly property string heraldPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/herald.json"
    property var records: []
    FileView {
        id: heraldFile
        path: root.heraldPath
        watchChanges: true
        onFileChanged: heraldFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(heraldFile.text())
                root.ingest((d && d.notifications) ? d.notifications : [])
            } catch (e) { /* absent/garbage -- hold what we have */ }
        }
        Component.onCompleted: heraldFile.reload()
    }

    // -- The dismiss clock -- one map id -> epoch-ms deadline (0 = never) ----
    property var deadlines: ({})
    property var lapsed: ({})
    property int epoch: 0

    function ingest(arr) {
        var now = Date.now()
        var dl = {}
        var lp = {}
        for (var i = 0; i < arr.length; i++) {
            var r = arr[i]
            if (!r || r.id === undefined) continue
            var id = "" + r.id
            var t = (r.timeoutMs !== undefined) ? (r.timeoutMs | 0) : 0
            dl[id] = (id in root.deadlines) ? root.deadlines[id] : (t > 0 ? now + t : 0)
            if (root.lapsed[id]) lp[id] = true
        }
        root.deadlines = dl
        root.lapsed = lp
        root.records = arr
        root.epoch++
    }
    Timer {
        interval: 500
        repeat: true
        running: root.visibleRecords.length > 0
        onTriggered: {
            var now = Date.now()
            var changed = false
            var lp = root.lapsed
            for (var id in root.deadlines) {
                var d = root.deadlines[id]
                if (d > 0 && now >= d && !lp[id]) { lp[id] = true; changed = true }
            }
            if (changed) { root.lapsed = lp; root.epoch++ }
        }
    }

    // newest last -> bottom-anchored stack grows upward out of the corner
    readonly property var visibleRecords: {
        var e = root.epoch
        var out = []
        for (var i = root.records.length - 1; i >= 0; i--) {
            var r = root.records[i]
            if (!r || r.id === undefined) continue
            if (root.lapsed["" + r.id]) continue
            out.push(r)
            if (out.length >= root.maxCards) break
        }
        return out.reverse()
    }

    function isSummons(r) { return r && r.kind === "summons" }
    function urgencyOf(r) { return r ? ("" + (r.urgency || "normal")) : "normal" }
    function tagOf(r) {
        if (root.isSummons(r)) return "verdict"
        return root.urgencyOf(r)
    }
    function dismiss(id) {
        root.bridge.sendCommand({ cmd: "heralddismiss", id: "" + id })
    }
    function wave(id) {
        var lp = root.lapsed
        lp["" + id] = true
        root.lapsed = lp
        root.epoch++
    }
    function verdict(sessionId, word) {
        root.bridge.sendCommand({ cmd: "heraldverdict", id: "" + sessionId, verdict: word })
    }

    // -- Layer-shell surface -- bottom-right, the corner the herald cries
    // from (dormant while the ledger is empty) ------------------------------
    anchors { bottom: true; right: true }
    margins { bottom: 8; right: 10 }
    exclusiveZone: 0
    color: "transparent"
    visible: root.visibleRecords.length > 0
    implicitWidth: root.cardW + 1
    implicitHeight: Math.max(1, stack.implicitHeight)
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-herald"

    Column {
        id: stack
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        spacing: root.stackGap

        Repeater {
            model: root.visibleRecords
            delegate: Item {
                id: slot
                required property var modelData
                readonly property var rec: modelData
                readonly property bool summons: root.isSummons(rec)
                readonly property bool critical: root.urgencyOf(rec) === "critical"
                readonly property string bodyText: rec ? ("" + (rec.body || "")) : ""

                width: root.cardW
                height: col.implicitHeight

                Rectangle {
                    anchors.fill: parent
                    radius: 0
                    color: root.livery.notifBg
                }

                MouseArea {
                    // whole-card dismiss catcher, declared first so the
                    // verdict chips (below) win the hit test over it
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: slot.summons ? root.wave(slot.rec.id) : root.dismiss(slot.rec.id)
                }

                Column {
                    id: col
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    spacing: 0

                    Row {
                        spacing: 0
                        Cell {
                            livery: root.livery
                            paper: root.livery.notifBg
                            ink: root.livery.notifFg
                            label: "app"
                            value: slot.rec ? ("" + (slot.rec.app || "system")) : "system"
                        }
                        Cell {
                            livery: root.livery
                            paper: root.livery.notifBg
                            ink: root.livery.notifFg
                            label: "tag"
                            value: root.tagOf(slot.rec)
                            urgent: slot.critical || slot.summons
                            active: slot.critical
                        }
                    }

                    Text {
                        width: parent.width
                        leftPadding: 8
                        rightPadding: 8
                        topPadding: 4
                        bottomPadding: 4
                        visible: text.length > 0
                        text: slot.rec ? ("" + (slot.rec.summary || "")) : ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        font.family: "JetBrainsMono Nerd Font"
                        font.pixelSize: 11
                        color: root.livery.notifFg
                    }
                    Text {
                        width: parent.width
                        leftPadding: 8
                        rightPadding: 8
                        bottomPadding: 4
                        visible: slot.bodyText.length > 0
                        text: slot.bodyText
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        font.family: "JetBrainsMono Nerd Font"
                        font.pixelSize: 10
                        color: root.livery.notifFg
                        opacity: 0.75
                    }

                    Row {
                        visible: slot.summons
                        spacing: 0
                        Cell {
                            livery: root.livery
                            paper: root.livery.notifBg
                            ink: root.livery.notifFg
                            value: "approve"
                            interactive: true
                            onActivated: root.verdict(slot.rec.sessionId, "approve")
                        }
                        Cell {
                            livery: root.livery
                            paper: root.livery.notifBg
                            ink: root.livery.notifFg
                            value: "deny"
                            urgent: true
                            interactive: true
                            onActivated: root.verdict(slot.rec.sessionId, "deny")
                        }
                    }
                }
            }
        }
    }
}
