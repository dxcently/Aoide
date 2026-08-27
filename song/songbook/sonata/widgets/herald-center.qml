// herald-center.qml — sonata's "herald-center" slot: the notification
// LEDGER, the herald's standing record in the dock (WidgetSlot host,
// AoidePanel.qml — root is an Item that sizes itself off its content).
//
// The popup (herald.qml) is the herald CRYING the news; this is the roll he
// cried it FROM: one self-framed stele in the pantheon family (opaque
// marble, 2px ink border, 1px inset signature keyline, cast shadow, radius
// 0, colour only from livery.*), carrying every record still on the books as
// a compact manuscript row — the colonnade's DeviceRow grammar: margin
// mark, serif name, mono gloss, hairline rule, hover promotes the rule.
//
//   · SIGNATURE — RUST `livery.base0F`, shared with the popup: one family,
//     one hue (the same one-signature-per-family rule the calendar's clay
//     and the colonnade's murex follow).
//   · CROWN — ❧ U+2767 pinned to "Linux Libertine O" (Noto Serif has no
//     U+2767; unpinned it falls back to a fake-bolded blob — live-verified
//     on this stack). Regular weight. The mono frame bands never carry ❧ —
//     JetBrainsMono is not proven to hold it, and bar.qml's glyph lesson
//     says ANY unproven non-ASCII glyph needs a live render check first.
//   · ROWS — newest first. The margin mark is the urgency register: a dim
//     interpunct for low, the rust fleuron for normal, the terracotta
//     fleuron (breathing, the bar's urgent-pulse idiom) for critical, the
//     gold fleuron for a summons. Icons sit INSIDE the row as a 16px mark;
//     progress is the hairline aegean gauge INSIDE the row band. Clicking a
//     toast row dismisses it ({ cmd: "heralddismiss" }); a summons row
//     carries its own hit-tested approve / deny chips instead and cannot be
//     waved away.
//   · Sender text is UNTRUSTED DATA — every Text rendering it is
//     PlainText; `<b>` displays literally.
//
// Read side: state/stage/herald.json via FileView (bar.qml's sessions.json
// watch). Newest LAST in the array, capped at 20 daemon-side. This surface
// never expires anything — the popup owns the dismiss clock; the ledger
// only drops a record when a human dismisses it (row click, or the
// [ notifs ]/[ clear ] tag → heralddismiss "*") or answers it (heraldverdict,
// auto-dismisses daemon-side).

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    // ── Note + bridge dependencies (injected by WidgetSlot) ────────────────
    required property var livery
    required property var bridge

    // ── Type voices (the pantheon family) ──────────────────────────────────
    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"
    readonly property string faceCrown: "Linux Libertine O"   // ❧ lives here ONLY

    readonly property color sig: livery.base0F        // rust — the family signature
    readonly property color ink: livery.paletteFg
    readonly property color aegean: livery.holoBlue   // information (progress)
    readonly property color gold: livery.paletteAccent
    readonly property color fire: livery.paletteUrgent

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // WidgetSlot sizes itself off this item; the dock pins the slot's width,
    // so width follows the host (bar.qml's parent-width idiom) and height is
    // this file's own content.
    width: parent ? parent.width : 360
    implicitHeight: stele.height + 5     // +5 clears the cast shadow's overhang

    // ── The ledger file — read-only, watched ───────────────────────────────
    // herald.json is a CONDUCTING file (CONTRACTS.md §4) — state/stage/,
    // not song/stage/.
    readonly property string heraldPath:
        Quickshell.env("HOME") + "/Aoide/state/stage/herald.json"
    property var records: []
    FileView {
        id: heraldFile
        path: root.heraldPath
        watchChanges: true
        onFileChanged: heraldFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(heraldFile.text())
                root.records = (d && d.notifications) ? d.notifications : []
            } catch (e) { /* absent/garbage → hold */ }
        }
        Component.onCompleted: heraldFile.reload()
    }
    // newest first for the roll
    readonly property var rows: {
        var out = []
        for (var i = root.records.length - 1; i >= 0; i--)
            if (root.records[i] && root.records[i].id !== undefined)
                out.push(root.records[i])
        return out
    }

    function dismiss(id) {
        root.bridge.sendCommand({ cmd: "heralddismiss", id: "" + id })
    }
    function verdict(sessionId, word) {
        root.bridge.sendCommand({ cmd: "heraldverdict",
                                  id: "" + sessionId, verdict: word })
    }

    // ── Relative arrival time, off a 30s tick ──────────────────────────────
    property date now: new Date()
    Timer {
        interval: 30000; repeat: true; running: root.rows.length > 0
        onTriggered: root.now = new Date()
    }
    function relTime(iso) {
        if (!iso) return ""
        var t = new Date("" + iso)
        if (isNaN(t.getTime())) return ""
        var s = Math.floor((root.now.getTime() - t.getTime()) / 1000)
        if (s < 45) return "now"
        if (s < 3600) return Math.round(s / 60) + "m"
        if (s < 86400) return Math.round(s / 3600) + "h"
        return Math.round(s / 86400) + "d"
    }

    function markColor(r) {
        if (r && r.kind === "summons") return root.gold
        var u = r ? ("" + r.urgency) : "normal"
        if (u === "critical") return root.fire
        if (u === "low") return root.withA(root.ink, 0.3)
        return root.withA(root.sig, 0.85)
    }

    // cast shadow — shared pantheon idiom
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: root.withA(root.ink, 0.22)
    }

    // ══ THE STELE ═══════════════════════════════════════════════════════════
    Rectangle {
        id: stele
        width: parent.width
        anchors.top: parent.top
        radius: 0
        color: root.livery.paletteBg
        border.color: root.ink
        border.width: 2
        height: content.implicitHeight + 20

        Rectangle {                                   // inset rust keyline
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.withA(root.sig, 0.8); border.width: 1
        }

        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 11
            spacing: 4

            // ── ENTABLATURE: ❧ crown + carved name + tag ────────────────
            Item {
                width: parent.width; height: 22
                Rectangle {                  // deeper marble band behind the
                    anchors.fill: parent     // inscription — the dock gadgets'
                    anchors.margins: -2      // own highlight (ConductorGadget)
                    color: root.withA(root.ink, 0.05)
                }
                Text {
                    id: crown
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "❧"
                    font.family: root.faceCrown       // pinned — see header
                    font.pixelSize: 20
                    color: root.sig
                }
                Text {
                    anchors.left: crown.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "HERALD"
                    font.family: root.faceSerif; font.pixelSize: 15
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: root.ink
                }
                // The tag slot IS the clear control (khoa, 2026-08-17). It
                // names what the ledger is at rest and what clicking it does
                // under the pointer, so the one affordance costs no extra
                // chrome — the old free-floating chip on the frame band below
                // is gone.
                //
                // The width is pinned to the LONGER of the two labels so the
                // swap cannot jiggle the right edge of the inscription band;
                // a right-anchored Text that resizes on hover twitches.
                TextMetrics {
                    id: tagMetrics
                    font.family: root.faceMono; font.pixelSize: 10
                    text: "[ notifs ]"
                }
                Text {
                    id: ledgerTag
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: tagMetrics.width
                    horizontalAlignment: Text.AlignRight
                    readonly property bool armed: tagMa.containsMouse
                                                  && root.rows.length > 0
                    text: ledgerTag.armed ? "[ clear ]" : "[ notifs ]"
                    font.family: root.faceMono; font.pixelSize: 10
                    color: root.withA(root.sig, ledgerTag.armed ? 1.0 : 0.9)
                    MouseArea {
                        id: tagMa
                        anchors.fill: parent; anchors.margins: -3
                        hoverEnabled: true
                        // Nothing to clear → not a button, and the label never
                        // offers an action that would do nothing.
                        enabled: root.rows.length > 0
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.dismiss("*")
                    }
                }
            }

            // ── frieze — a running fleuron vine: stem + alternating leaves ─
            Canvas {
                width: parent.width; height: 9
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset(); ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = root.withA(root.sig, 0.7)
                    ctx.lineWidth = 1.1
                    var cy = height / 2, period = 14
                    ctx.beginPath()
                    ctx.moveTo(2, cy); ctx.lineTo(width - 2, cy)   // the stem
                    ctx.stroke()
                    var i = 0
                    for (var x = 8; x < width - 8; x += period, i++) {
                        var up = (i % 2 === 0) ? -1 : 1            // alternating
                        ctx.beginPath()                            // leaf curl
                        ctx.moveTo(x, cy)
                        ctx.quadraticCurveTo(x + 3.5, cy + up * 3.6,
                                             x + 7, cy + up * 1.2)
                        ctx.quadraticCurveTo(x + 4.5, cy + up * 1.8, x + 3, cy)
                        ctx.stroke()
                    }
                }
            }

            // ── box-drawing top frame — an unbroken rule now that the clear
            // control lives in the entablature's tag slot above ─────────────
            Item {
                width: parent.width; height: 15
                Text {
                    id: tfL
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ledger ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "┐"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right
                    anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 5
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }

            // ── the roll — manuscript rows, newest first ───────────────────
            Column {
                width: parent.width
                spacing: 0

                // empty state — the herald rests
                Item {
                    width: parent.width; height: 34
                    visible: root.rows.length === 0
                    Text {
                        anchors.centerIn: parent
                        text: "( ˘ω˘ )  nothing to cry"
                        font.family: root.faceMono; font.pixelSize: 10
                        color: root.withA(root.ink, 0.45)
                    }
                }

                Repeater {
                    model: root.rows
                    delegate: Item {
                        id: row
                        required property var modelData
                        readonly property var rec: modelData
                        readonly property bool summons: rec && rec.kind === "summons"
                        readonly property bool critical:
                            rec && ("" + rec.urgency) === "critical"
                        readonly property string bodyText:
                            rec ? ("" + (rec.body || "")) : ""
                        readonly property string iconPath:
                            rec ? ("" + (rec.icon || "")) : ""
                        readonly property int progress:
                            (rec && rec.progress !== undefined) ? (rec.progress | 0) : -1
                        readonly property bool hot: rowMa.containsMouse

                        width: parent.width
                        height: 20 + (bodyLine.visible ? 13 : 0)
                                + (gaugeLine.visible ? 11 : 0)
                                + (chipLine.visible ? 20 : 0) + 5

                        // critical breathes (the bar's urgent-pulse idiom)
                        SequentialAnimation on opacity {
                            running: row.critical
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 0.55; duration: 700; easing.type: Easing.InOutQuad }
                            NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutQuad }
                        }

                        // row-wide catcher FIRST — chips (later) win the hit
                        // test. A toast row dismisses on click; a summons row
                        // is answered, never waved away.
                        MouseArea {
                            id: rowMa
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: row.summons ? Qt.ArrowCursor
                                                     : Qt.PointingHandCursor
                            onClicked: if (!row.summons) root.dismiss(row.rec.id)
                        }

                        // line 1 — mark · icon mark · summary · time / ✕
                        Item {
                            id: line1
                            anchors.left: parent.left; anchors.right: parent.right
                            anchors.top: parent.top
                            height: 20
                            Text {
                                id: mark
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                width: 14
                                horizontalAlignment: Text.AlignHCenter
                                text: (row.rec && ("" + row.rec.urgency) === "low"
                                       && !row.summons) ? "·" : "❧"
                                font.family: root.faceCrown
                                font.pixelSize: 12
                                color: root.markColor(row.rec)
                            }
                            Rectangle {
                                id: rowPlate
                                visible: row.iconPath.length > 0
                                anchors.left: mark.right; anchors.leftMargin: 3
                                anchors.verticalCenter: parent.verticalCenter
                                width: 16; height: 16
                                radius: 0
                                color: root.withA(root.ink, 0.06)
                                border.width: 1
                                border.color: root.withA(root.ink, 0.35)
                                // fit on the mount, never upscale, decode
                                // bounded — the popup plate's rules at row
                                // scale (see herald.qml's plate note)
                                Image {
                                    anchors.centerIn: parent
                                    source: row.iconPath.length > 0
                                            ? "file://" + row.iconPath : ""
                                    fillMode: Image.PreserveAspectFit
                                    asynchronous: true; smooth: true
                                    mipmap: true
                                    sourceSize: Qt.size(32, 32)
                                    readonly property real fitS:
                                        (implicitWidth > 0 && implicitHeight > 0)
                                        ? Math.min(1, Math.min(14 / implicitWidth,
                                                               14 / implicitHeight))
                                        : 1
                                    width: Math.max(1, Math.round(implicitWidth * fitS))
                                    height: Math.max(1, Math.round(implicitHeight * fitS))
                                }
                            }
                            Text {
                                anchors.left: rowPlate.visible ? rowPlate.right : mark.right
                                anchors.leftMargin: 5
                                anchors.right: stamp.left; anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: row.rec ? ("" + (row.rec.summary || "")) : ""
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font.family: root.faceSerif
                                font.pixelSize: 11
                                font.weight: (row.critical || row.summons)
                                             ? Font.DemiBold : Font.Normal
                                color: root.withA(root.ink, row.hot ? 1.0 : 0.85)
                            }
                            Text {
                                id: stamp
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                // hover turns the clock into the dismiss hint
                                text: (row.hot && !row.summons)
                                      ? "x" : root.relTime(row.rec ? row.rec.receivedAt : "")
                                font.family: root.faceMono
                                font.pixelSize: 8
                                font.letterSpacing: 1
                                color: (row.hot && !row.summons)
                                       ? root.fire : root.withA(root.ink, 0.45)
                            }
                        }

                        // line 2 — who said it · what else it said
                        Text {
                            id: bodyLine
                            visible: row.rec
                                     && (("" + (row.rec.app || "")).length > 0
                                         || row.bodyText.length > 0)
                            anchors.left: parent.left; anchors.leftMargin: 22
                            anchors.right: parent.right
                            anchors.top: line1.bottom
                            height: visible ? 13 : 0
                            text: (row.rec ? ("" + (row.rec.app || "")) : "")
                                  + (row.bodyText.length > 0
                                     ? ("  ·  " + row.bodyText) : "")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            font.family: root.faceMono
                            font.pixelSize: 8
                            font.letterSpacing: 1
                            color: root.withA(root.ink, 0.45)
                        }

                        // the gauge — progress inside the row band, aegean
                        Item {
                            id: gaugeLine
                            visible: row.progress >= 0
                            anchors.left: parent.left; anchors.leftMargin: 22
                            anchors.right: parent.right
                            anchors.top: bodyLine.bottom
                            height: visible ? 11 : 0
                            Item {
                                anchors.left: parent.left
                                anchors.right: rowFig.left; anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                height: 4
                                Rectangle {
                                    anchors.fill: parent; radius: 0
                                    color: root.withA(root.ink, 0.09)
                                    border.width: 1
                                    border.color: root.withA(root.ink, 0.35)
                                }
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.top: parent.top; anchors.bottom: parent.bottom
                                    anchors.margins: 1
                                    width: (parent.width - 2)
                                           * Math.max(0, Math.min(1, row.progress / 100))
                                    radius: 0
                                    color: root.withA(root.aegean, 0.9)
                                }
                            }
                            Text {
                                id: rowFig
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 28
                                horizontalAlignment: Text.AlignRight
                                text: Math.max(0, Math.min(100, row.progress)) + "%"
                                font.family: root.faceMono; font.pixelSize: 8
                                color: root.withA(root.ink, 0.7)
                            }
                        }

                        // the verdict chips — two hit regions, gold / fire
                        Item {
                            id: chipLine
                            visible: row.summons
                            anchors.left: parent.left; anchors.leftMargin: 22
                            anchors.right: parent.right
                            anchors.top: gaugeLine.bottom
                            height: visible ? 20 : 0
                            Rectangle {
                                id: rowApprove
                                anchors.right: rowDeny.left; anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                width: 72; height: 16
                                radius: 0
                                color: rowApproveMa.containsMouse
                                       ? root.withA(root.gold, 0.20) : "transparent"
                                border.width: 1
                                border.color: root.withA(root.gold,
                                                  rowApproveMa.containsMouse ? 0.95 : 0.6)
                                Text {
                                    anchors.centerIn: parent
                                    text: "approve"
                                    font.family: root.faceMono; font.pixelSize: 9
                                    color: root.gold
                                }
                                MouseArea {
                                    id: rowApproveMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(row.rec.sessionId, "approve")
                                }
                            }
                            Rectangle {
                                id: rowDeny
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 72; height: 16
                                radius: 0
                                color: rowDenyMa.containsMouse
                                       ? root.withA(root.fire, 0.20) : "transparent"
                                border.width: 1
                                border.color: root.withA(root.fire,
                                                  rowDenyMa.containsMouse ? 0.95 : 0.6)
                                Text {
                                    anchors.centerIn: parent
                                    text: "deny"
                                    font.family: root.faceMono; font.pixelSize: 9
                                    color: root.fire
                                }
                                MouseArea {
                                    id: rowDenyMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(row.rec.sessionId, "deny")
                                }
                            }
                        }

                        // the rule — hover promotes it (DeviceRow idiom)
                        Rectangle {
                            anchors.left: parent.left; anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            height: 1
                            color: root.withA(row.hot ? root.sig : root.ink,
                                              row.hot ? 0.6 : 0.13)
                            Behavior on color { ColorAnimation { duration: 150 } }
                        }
                    }
                }
            }

            // ── box-drawing bottom frame — the count, closing 𝄂 ────────────
            Item {
                width: parent.width; height: 16
                Text {
                    id: ffL
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + root.rows.length
                          + (root.rows.length === 1 ? " notice ├" : " notices ├")
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.ink, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "┘"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: "\u{1D102}"
                    font.family: root.faceMusic; font.pixelSize: 15
                    color: root.livery.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }
        }
    }
}
