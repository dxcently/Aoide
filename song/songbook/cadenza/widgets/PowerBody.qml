// PowerBody.qml — everything the cadenza power menu draws (intent §3.6).
//
//     ┌─ SHUTDOWN ──────────────────────────────── esc ┐
//     │ [l] lock      screen locked, session kept       │
//     │ [s] suspend   sleep to ram                      │
//     │ [r] reboot    restart the machine               │
//     │ [p] poweroff  poweroff now? [y/N]               │   a destructive row asks inline
//     │ [o] logout    end the hyprland session          │
//     │                                                 │
//     │ > poweroff _                                    │   the prompt echoes the choice
//     └─────────────────────────────────────────────────┘
//
// A HELPER (uppercase — never a slot). `powermenu.qml` is the thin
// PanelWindow shell (namespace, layer, focus, toggle, shortcut) and loads
// this body BY URL (design/kit.md §1); the preview canvas loads it directly,
// since the canvas refuses a PanelWindow root.
//
// ── KEYS (keyboard-first) ─────────────────────────────────────────────────
// The letter fires: `l`/`s` at once, `r`/`p`/`o` (destructive) arm an inline
// `[y/N]` on their row. In that confirm only `y` fires; `n`, Esc, anything
// else disarms. j/k, ↑/↓, Tab move the selection; Return fires it. Esc with
// nothing armed asks the shell to close (`dismissed`). Hover selects, click
// fires exactly as the letter would; a click on the scrim closes.
//
// ── THE WIRE ──────────────────────────────────────────────────────────────
// Exactly `{ cmd: "power", action }` through `bridge.sendCommand` — the only
// outbound path; the shellbridge daemon owns the spawn (rule 7). The action
// strings are the daemon's closed set (`PowerAction::from_wire`): "lock",
// "suspend", "reboot", "shutdown", "logout". The row reads `poweroff` (the
// terminal's word); its wire action is `shutdown`, sonata's string.
//
// ── COLOUR ────────────────────────────────────────────────────────────────
// At rest nothing is amber. The selected row gets the `select` band, its
// `[x]` index turns amber (the ONE live thing), its name `match`. The confirm
// question is `urgent` red: a summons waiting on you. No hex here.
import QtQuick
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    property var bridge: null

    // The shell drives these; the canvas leaves the defaults (drawn open).
    property bool shown: true
    property int sel: -1            // selected row, -1 = none
    property int armed: -1          // row whose [y/N] is up, -1 = none
    signal dismissed()

    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    component Use: Loader {
        required property var kit
        required property string helper
        property var props: ({})
        Component.onCompleted: {
            var p = { kit: Qt.binding(() => kit) }
            for (var k in props) p[k] = props[k]
            setSource(kit.helper(helper), p)
        }
    }

    // ── the five endings (intent §3.6), wire actions = the daemon's set ───
    readonly property var endings: [
        { key: "l", name: "lock",     action: "lock",     destructive: false, what: "screen locked, session kept" },
        { key: "s", name: "suspend",  action: "suspend",  destructive: false, what: "sleep to ram" },
        { key: "r", name: "reboot",   action: "reboot",   destructive: true,  what: "restart the machine" },
        { key: "p", name: "poweroff", action: "shutdown", destructive: true,  what: "halt and power off" },
        { key: "o", name: "logout",   action: "logout",   destructive: true,  what: "end the hyprland session" }
    ]
    readonly property int nameCells: 10
    readonly property int innerCols: 4 + nameCells + 28     // "[x] " + name + description + air

    // Reset to a clean prompt; the shell calls this on every show.
    function reset() { root.sel = -1; root.armed = -1 }

    function invoke(action) {
        if (root.bridge) root.bridge.sendCommand({ cmd: "power", action: action })
        root.reset()
        root.dismissed()
    }

    function fire(i) {
        if (i < 0 || i >= root.endings.length) return
        var e = root.endings[i]
        root.sel = i
        if (e.destructive) root.armed = i
        else root.invoke(e.action)
    }

    focus: true
    Keys.onPressed: function (event) {
        var n = root.endings.length
        if (root.armed >= 0) {
            if (event.key === Qt.Key_Y)
                root.invoke(root.endings[root.armed].action)
            else
                root.armed = -1
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Escape) {
            root.reset(); root.dismissed()
        } else if (event.key === Qt.Key_Down || event.key === Qt.Key_J || event.key === Qt.Key_Tab) {
            root.sel = root.sel < 0 ? 0 : (root.sel + 1) % n
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_K || event.key === Qt.Key_Backtab) {
            root.sel = root.sel < 0 ? n - 1 : (root.sel + n - 1) % n
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.fire(root.sel)
        } else {
            var t = (event.text || "").toLowerCase()
            var hit = -1
            for (var i = 0; i < n; i++) if (root.endings[i].key === t) hit = i
            if (hit < 0) return
            root.fire(hit)
        }
        event.accepted = true
    }

    implicitWidth: kit.cells(innerCols + 2) + kit.cells(12)
    implicitHeight: kit.lines(7 + 1.5) + kit.lines(6)

    // ── scrim: CRT black, opaque-ish, no blur (intent §5: no glass) ──────
    Rectangle {
        anchors.fill: parent
        color: root.kit.withA(root.kit.ground, 0.82)
        opacity: root.shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: root.shown ? 160 : 120 } }
    }
    MouseArea {
        anchors.fill: parent
        onClicked: { root.reset(); root.dismissed() }
    }

    // ── the pane ──────────────────────────────────────────────────────────
    Use {
        id: paneUse
        kit: root.kit
        helper: "Pane"
        anchors.centerIn: parent
        props: ({
            title: "shutdown",
            stat: "esc",
            statColor: Qt.binding(() => root.kit.dim),
            focused: true,
            animateOnCreate: true,
            open: Qt.binding(() => root.shown),
            cols: root.innerCols,
            rows: 7,
            content: menuBody })
        // clicks inside the pane never reach the scrim
        MouseArea { anchors.fill: parent; z: -1 }
    }

    Component {
        id: menuBody
        Column {
            Repeater {
                model: root.endings
                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool lit: root.sel === row.index
                    readonly property bool asking: root.armed === row.index
                    width: root.kit.cells(root.innerCols)
                    height: root.kit.cellH

                    Rectangle {
                        anchors.fill: parent
                        color: root.kit.select
                        visible: row.lit
                    }
                    Row {
                        Text {
                            text: "["; font: root.kit.font; textFormat: Text.PlainText
                            color: row.lit ? root.kit.match : root.kit.dim
                        }
                        Text {
                            text: row.modelData.key; font: root.kit.titleFont; textFormat: Text.PlainText
                            color: row.lit ? root.kit.hot : root.kit.bright
                        }
                        Text {
                            text: "] "; font: root.kit.font; textFormat: Text.PlainText
                            color: row.lit ? root.kit.match : root.kit.dim
                        }
                        Text {
                            text: root.kit.padR(row.modelData.name, root.nameCells)
                            font: root.kit.font; textFormat: Text.PlainText
                            color: row.lit ? root.kit.match : root.kit.ink
                        }
                        // at rest: what it does; armed: the inline y/N
                        Text {
                            visible: !row.asking
                            text: row.modelData.what
                            font: root.kit.font; textFormat: Text.PlainText
                            color: row.lit ? root.kit.mid : root.kit.dim
                        }
                        Text {
                            visible: row.asking
                            text: row.modelData.name + " now? "
                            font: root.kit.font; textFormat: Text.PlainText
                            color: root.kit.urgent
                        }
                        Text {
                            visible: row.asking
                            text: "[y/N]"
                            font: root.kit.titleFont; textFormat: Text.PlainText
                            color: root.kit.match
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        onEntered: if (root.armed < 0) root.sel = row.index
                        onClicked: {
                            if (root.armed === row.index) root.armed = -1
                            else root.fire(row.index)
                        }
                    }
                }
            }
            // blank line, then the prompt: echoes the choice like a typed command
            Item { width: 1; height: root.kit.cellH }
            Row {
                Text {
                    text: "> "; font: root.kit.titleFont; textFormat: Text.PlainText
                    color: root.kit.title
                }
                Text {
                    text: root.sel >= 0 ? root.endings[root.sel].name + " " : ""
                    font: root.kit.font; textFormat: Text.PlainText
                    color: root.kit.ink
                }
                Text {
                    text: "_"; font: root.kit.font; textFormat: Text.PlainText
                    color: root.kit.ink
                }
                Text {
                    visible: root.sel < 0
                    text: "   press a letter · j/k ⏎ · esc"
                    font: root.kit.font; textFormat: Text.PlainText
                    color: root.kit.dim
                }
            }
        }
    }
}
