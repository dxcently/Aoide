import QtQuick
import QtQuick.Shapes
import Quickshell
import Quickshell.Wayland
import "Kit.js" as Kit
import "Rice.js" as Rice

// The Declare row's two surfaces: the Platinum alert that asks first (Cancel is the default)
// and the balloon over the Start orb that says what the bridge answered.
Item {
    id: root

    required property var bridge
    property var screen: null
    property Item anchorItem: null

    readonly property string g3Dir: "file://" + (Quickshell.env("STRILLIX_ROOT") || (Quickshell.env("HOME") + "/.strillix")) + "/rices/barcarolle/icons/launcher/"
    readonly property string fSys: Qt.fontFamilies().indexOf("Tahoma") >= 0 ? "Tahoma" : Kit.family
    readonly property var pl: ({ face: "#dddddd", hi: "#ffffff", sh: "#8a8a8a", line: "#1d1d1d", sel: "#0f6d80" })

    property var row: null
    property var answer: null
    property string question: ""
    property bool busy: false
    property var note: null
    property int asked: 0

    function ask(row, answer) {
        if (!row || !row.live || root.busy) return
        root.note = null
        root.question = Rice.declareQuestion(answer, row)
        root.answer = answer
        root.row = row
    }
    function cancel() {
        if (!root.busy) root.row = null
    }
    function confirm() {
        if (!root.row || root.busy) return
        var ask = ++root.asked
        root.busy = true
        Rice.declare(root.bridge, root.row, root.answer, function (note) {
            if (ask !== root.asked) return
            root.busy = false
            root.row = null
            root.note = note
            noteLife.restart()
        })
    }
    Timer { id: noteLife; interval: 12000; onTriggered: root.note = null }

    component PBtn: Item {
        id: pb
        property string label: ""
        property bool dflt: false
        property bool live: true
        signal chosen()
        width: 64
        height: 20
        Rectangle {
            visible: pb.dflt
            x: -3; y: -3
            width: pb.width + 6; height: pb.height + 6
            radius: 6
            color: "transparent"
            border.width: 2
            border.color: root.pl.line
        }
        Rectangle {
            anchors.fill: parent
            radius: 4
            color: pbMa.pressed ? "#bcbcbc" : root.pl.face
            border.width: 1
            border.color: pb.live ? root.pl.line : "#9a9a9a"
        }
        Text {
            anchors.centerIn: parent
            text: pb.label
            color: pb.live ? "#000000" : "#8a8a8a"
            renderType: Text.NativeRendering
            textFormat: Text.PlainText
            font.family: root.fSys
            font.pixelSize: 12
        }
        MouseArea { id: pbMa; anchors.fill: parent; enabled: pb.live; onClicked: pb.chosen() }
    }

    PanelWindow {
        screen: root.screen
        visible: root.row !== null
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        color: Kit.withA("#000000", 0.18)
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-declare"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
        Item {
            anchors.fill: parent
            focus: true
            Keys.onPressed: function (e) {
                if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Escape) { root.cancel(); e.accepted = true }
            }
        }
        Rectangle {
            width: 300
            height: Math.max(120, words.y + words.height + 52)
            x: Math.round((parent.width - width) / 2)
            y: Math.round(parent.height * 0.3)
            color: root.pl.face
            border.width: 1
            border.color: root.pl.line
            Rectangle { x: 1; y: 1; width: parent.width - 2; height: 1; color: root.pl.hi }
            Rectangle { x: 1; y: 1; width: 1; height: parent.height - 2; color: root.pl.hi }
            Rectangle { x: 1; y: parent.height - 2; width: parent.width - 2; height: 1; color: root.pl.sh }
            Rectangle { x: parent.width - 2; y: 1; width: 1; height: parent.height - 2; color: root.pl.sh }
            Image { x: 16; y: 16; width: 32; height: 32; source: root.g3Dir + "finder-caution.svg"; sourceSize: Qt.size(32, 32) }
            Text {
                id: words
                x: 60; y: 16
                width: 224
                wrapMode: Text.WordWrap
                text: root.question
                renderType: Text.NativeRendering
                textFormat: Text.PlainText
                font.family: root.fSys
                font.pixelSize: 12
                lineHeight: 14
                lineHeightMode: Text.FixedHeight
            }
            PBtn { x: parent.width - 156; y: parent.height - 32; label: "Cancel"; dflt: true; live: !root.busy; onChosen: root.cancel() }
            PBtn { x: parent.width - 82; y: parent.height - 32; label: root.busy ? "…" : "Declare"; live: !root.busy; onChosen: root.confirm() }
        }
    }

    PopupWindow {
        visible: root.note !== null && !!root.anchorItem
        color: "transparent"
        anchor.item: root.anchorItem
        anchor.edges: Edges.Top | Edges.Left
        anchor.gravity: Edges.Top | Edges.Right
        anchor.adjustment: PopupAdjustment.Slide
        implicitWidth: 300
        implicitHeight: plate.height + 12
        Rectangle {
            id: plate
            x: 0; y: 0
            width: 300
            height: body.y + body.height + 12
            radius: 4
            color: "#ffffff"
            border.width: 1
            border.color: Kit.dark(0.55)
            Text {
                id: title
                x: 12; y: 9
                width: parent.width - 24
                text: root.note ? root.note.title : ""
                color: Kit.ink
                font: Kit.boldFont(12)
                textFormat: Text.PlainText
            }
            Text {
                id: body
                x: 12; y: title.y + title.height + 4
                width: parent.width - 24
                wrapMode: Text.WordWrap
                text: root.note ? root.note.text : ""
                color: Kit.ink
                font: Kit.bodyFont(12)
                textFormat: Text.PlainText
            }
            MouseArea { anchors.fill: parent; onClicked: root.note = null }
        }
        Shape {
            x: 14; y: plate.height - 1
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: Kit.dark(0.55)
                strokeWidth: 1
                fillColor: "#ffffff"
                startX: 0; startY: 0
                PathLine { x: 6; y: 11 }
                PathLine { x: 14; y: 0 }
            }
        }
    }
}
