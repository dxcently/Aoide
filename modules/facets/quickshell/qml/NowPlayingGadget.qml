// NowPlayingGadget.qml — mpris now-playing as ASCII, for the gadget dock.
//
// Binds the active MPRIS player via Quickshell.Services.Mpris (the real-service
// idiom, no shell-out). Renders, in Win7-ASCII spirit:
//   - track/artist in ♪« … » framing, marquee-scrolled when it overflows;
//   - an ASCII progress bar (▮▯) from position/length when supported;
//   - play/pause/prev/next controls as glyphs, each enabled/dimmed by the
//     player's can* flags.
// All colours from notes (zero hardcoded hex); the ♪«» / notation are content.
// Degrade: no player → a dimmed "no media" line (mirrors the roster empty state).

import QtQuick
import Quickshell.Services.Mpris

Item {
    id: root

    required property var notes

    // ── Active player: prefer a playing one, else the first available ──────
    readonly property var players: (Mpris.players && Mpris.players.values)
                                    ? Mpris.players.values : []
    readonly property var player: {
        var ps = root.players
        for (var i = 0; i < ps.length; i++)
            if (ps[i] && ps[i].isPlaying) return ps[i]
        return ps.length > 0 ? ps[0] : null
    }
    readonly property bool hasPlayer: player !== null

    // ── Position poll (position is not push-notified reliably) ─────────────
    property real posSec: 0
    Timer {
        interval: 1000
        repeat: true
        running: root.hasPlayer
        onTriggered: {
            if (root.player && root.player.positionSupported)
                root.posSec = root.player.position
        }
    }

    // ── Marquee + progress helpers ─────────────────────────────────────────
    readonly property string nowText: {
        if (!player) return ""
        var artist = ("" + (player.trackArtist || "")).trim()
        var title = ("" + (player.trackTitle || "")).trim()
        if (artist.length > 0 && title.length > 0)
            return "♪« " + artist + " - " + title + " »"
        if (title.length > 0) return "♪« " + title + " »"
        return "♪« … »"
    }
    function progressBar(pos, len) {
        var cells = 16
        if (!len || len <= 0) {
            var e = ""
            for (var k = 0; k < cells; k++) e += "▯"
            return e
        }
        var frac = pos / len
        if (frac < 0) frac = 0
        if (frac > 1) frac = 1
        var filled = Math.round(frac * cells)
        var s = ""
        for (var i = 0; i < cells; i++) s += (i < filled) ? "▮" : "▯"
        return s
    }

    implicitHeight: column.implicitHeight

    Column {
        id: column
        width: parent ? parent.width : implicitWidth
        spacing: 3

        // ── Empty state ──────────────────────────────────────────────────
        Text {
            visible: !root.hasPlayer
            width: parent.width
            text: "no media"
            color: root.notes.paletteFg
            opacity: 0.6
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Marquee track/artist in ♪«» framing ──────────────────────────
        Item {
            visible: root.hasPlayer
            width: parent.width
            height: marquee.implicitHeight
            clip: true

            Text {
                id: marquee
                text: root.nowText
                color: root.notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
                // Marquee: if the text overflows, ease it left and back.
                readonly property bool overflow: implicitWidth > parent.width
                x: 0
                SequentialAnimation on x {
                    running: marquee.overflow && root.hasPlayer
                    loops: Animation.Infinite
                    PauseAnimation { duration: 1200 }
                    NumberAnimation {
                        to: Math.min(0, marquee.parent.width - marquee.implicitWidth)
                        duration: 4000
                        easing.type: Easing.InOutQuad
                    }
                    PauseAnimation { duration: 1200 }
                    NumberAnimation { to: 0; duration: 4000; easing.type: Easing.InOutQuad }
                }
                // Crossfade on track change: a quick fade-out/in pulse fired
                // whenever nowText changes.
                onTextChanged: crossfade.restart()
                SequentialAnimation {
                    id: crossfade
                    NumberAnimation { target: marquee; property: "opacity"; to: 0.2; duration: 120 }
                    NumberAnimation { target: marquee; property: "opacity"; to: 1.0; duration: 180 }
                }
            }
        }

        // ── ASCII progress bar ───────────────────────────────────────────
        Text {
            visible: root.hasPlayer
            text: root.progressBar(root.posSec,
                                   root.player ? root.player.length : 0)
            color: root.notes.paletteAccent
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Controls: prev / play-pause / next ───────────────────────────
        Row {
            visible: root.hasPlayer
            spacing: 12

            Text {
                text: "𝄆◁"
                color: root.notes.paletteFg
                opacity: (root.player && root.player.canGoPrevious) ? 1.0 : 0.3
                font.family: "monospace"
                font.pixelSize: 14
                MouseArea {
                    anchors.fill: parent
                    enabled: root.player && root.player.canGoPrevious
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.previous()
                }
            }
            Text {
                text: (root.player && root.player.isPlaying) ? "𝄽▮▮" : "▷"
                color: root.notes.paletteAccent
                opacity: (root.player && root.player.canTogglePlaying) ? 1.0 : 0.3
                font.family: "monospace"
                font.pixelSize: 14
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    enabled: root.player && root.player.canTogglePlaying
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.togglePlaying()
                }
            }
            Text {
                text: "▷𝄇"
                color: root.notes.paletteFg
                opacity: (root.player && root.player.canGoNext) ? 1.0 : 0.3
                font.family: "monospace"
                font.pixelSize: 14
                MouseArea {
                    anchors.fill: parent
                    enabled: root.player && root.player.canGoNext
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.next()
                }
            }
        }
    }
}
