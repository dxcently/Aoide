// PowerGadget.qml — battery + network status, for the gadget dock.
//
// Battery from Quickshell.Services.UPower (rest-notation icon + ASCII charge
// bar + %); network from /proc/net/route via FileView (no NM service at this
// quickshell rev — files-not-processes, MeterGadget's precedent). All colours
// from notes (zero hardcoded hex); the notation glyphs are content.
//
// Degrade:
//   - desktop / no battery (isLaptopBattery false) → an honest "AC (no battery)"
//     line instead of a fabricated charge;
//   - no essid/ipaddr from procfs without shelling out → the iface name only.

import QtQuick
import Quickshell.Io
import Quickshell.Services.UPower

Item {
    id: root

    required property var notes

    // ── Battery (UPower display device) ────────────────────────────────────
    readonly property var battDev: UPower.displayDevice
    readonly property bool battAvail: battDev && battDev.isLaptopBattery && battDev.isPresent
    readonly property int battPct: battDev ? Math.round(battDev.percentage) : 0
    readonly property bool battCharging: battDev && battDev.state === UPowerDeviceState.Charging
    readonly property bool battFull: battDev && battDev.state === UPowerDeviceState.FullyCharged
    readonly property bool battWarn: battAvail && !battCharging && battPct <= 20

    function battIcon() {
        if (battFull) return "𝆑"
        if (battCharging) return "𝄮"
        var rests = ["𝄽", "𝄾", "𝄿", "𝅀", "𝅁", "𝅂"]
        var idx = Math.floor(battPct / 100 * (rests.length - 1))
        if (idx < 0) idx = 0
        if (idx >= rests.length) idx = rests.length - 1
        return rests[idx]
    }
    function chargeBar(pct) {
        var cells = 10
        var p = pct
        if (p < 0) p = 0
        if (p > 100) p = 100
        var filled = Math.round(p / 100 * cells)
        var s = "["
        for (var i = 0; i < cells; i++) s += (i < filled) ? "▓" : "░"
        s += "]"
        return s
    }

    // ── Network (procfs default-route detection) ───────────────────────────
    property string netKind: "down"   // "wifi" | "eth" | "down"
    property string netIface: ""
    function parseRoute(text) {
        if (!text) { root.netIface = ""; return "down" }
        var lines = ("" + text).split("\n")
        for (var i = 1; i < lines.length; i++) {
            var parts = lines[i].trim().split(/\s+/)
            if (parts.length < 2) continue
            if (parts[1] === "00000000") {
                root.netIface = parts[0]
                var n = parts[0].toLowerCase()
                if (n.indexOf("wl") === 0 || n.indexOf("wlan") === 0) return "wifi"
                return "eth"
            }
        }
        root.netIface = ""
        return "down"
    }
    function netGlyph(kind) {
        if (kind === "wifi") return "𝆹𝅥𝅮"
        if (kind === "eth")  return "𝆺𝅥𝅯"
        return "Disconnected :c"
    }
    FileView {
        id: routeFile
        path: "/proc/net/route"
        onTextChanged: root.netKind = root.parseRoute(routeFile.text)
        Component.onCompleted: routeFile.reload()
    }
    Timer {
        interval: 5000
        repeat: true
        running: true
        onTriggered: routeFile.reload()
    }

    implicitHeight: column.implicitHeight

    Column {
        id: column
        width: parent ? parent.width : implicitWidth
        spacing: 3

        // ── Battery row ──────────────────────────────────────────────────
        Row {
            visible: root.battAvail
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.battIcon()
                color: root.battWarn ? root.notes.paletteUrgent : root.notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 13
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.chargeBar(root.battPct)
                color: root.battWarn ? root.notes.paletteUrgent : root.notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.battPct + "%" + (root.battCharging ? " +" : "")
                color: root.notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
            }
        }

        // ── Battery degrade line (desktop / no battery) ──────────────────
        Text {
            visible: !root.battAvail
            width: parent.width
            text: "AC (no battery)"
            color: root.notes.paletteFg
            opacity: 0.6
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Section divider: short staff run between battery and network
        // (ornament vocabulary — a stave fragment, one per seam).
        Text {
            text: "𝄂𝄚𝅦𝄚"
            color: root.notes.paletteAccent
            opacity: 0.45
            font.family: "monospace"
            font.pixelSize: 11
        }

        // ── Network row ──────────────────────────────────────────────────
        Row {
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.netGlyph(root.netKind)
                color: root.netKind === "down" ? root.notes.paletteFg : root.notes.paletteAccent
                opacity: root.netKind === "down" ? 0.6 : 1.0
                font.family: "monospace"
                font.pixelSize: 13
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.netIface.length > 0
                text: root.netIface
                color: root.notes.paletteFg
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 12
            }
        }
    }
}
