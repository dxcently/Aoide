// notifications.qml — sonata's "notifications" slot: one popup notification,
// as a Tuscan temple stele. Lives here (not the facet) as of the widget-slot
// expansion (CONTRACTS.md §5) — sonata is the baseline-fallback floor every
// song resolves to when it hasn't authored its own `notifications.qml`, so
// this file IS the shared default now, not a separate facet-side Component.
// Fixed slot contract: `notes`/`bridge` always, `notification` (the tracked
// Notification model item) as this slot's one extra — see
// modules/facets/quickshell/qml/slots.md.
//
// khoa, 2026-07-30: rebuilt off the wallpaper-picker-style glass card (v1) to
// actually join the PANTHEON family — the same marble-stele grammar
// ConductorGadget/TerminalsGadget/MetersGadget/PowerVitalsGadget wear (that
// shared body-chrome convention lives in each of those files' own headers
// now, not a cross-cutting grammar section), with its own FIFTH order rather
// than copying one of theirs:
//
//   · ORDER    — TUSCAN, the plainest of the five classical orders (Doric,
//                Ionic, Corinthian, Tuscan, Composite — the temples above
//                already claim the first three). Historically the Tuscan
//                entablature carries NO ornament frieze at all — fitting for
//                a lean, transient herald that doesn't linger like a dock
//                gauge — so its frieze is a plain double rule, not a Canvas
//                ornament (no meander, no egg-and-dart, no triglyph/metope:
//                genuinely unornamented, which is what makes it read as its
//                own order rather than a knockoff of the other four).
//   · SIGNATURE — base0F (rust), the one accentSpread slot none of the four
//                dock temples claims (LiveryState.qml's own comment already
//                calls it "unnamed elsewhere"). Critical urgency overrides
//                the signature to notifUrgent (terracotta) throughout.
//   · CROWN    — ❧ (a rotated floral heart / printer's notice-ornament,
//                U+2767) stands in for a clef, the same way Power's ϟ
//                (Greek koppa) does when "no clef fits" — a herald has no
//                voice range to notate.
//   · NO ICONS — khoa, 2026-07-30: explicitly dropped. An earlier version
//                rendered the sender's themed appIcon (or a fallback glyph)
//                in the header; this rebuild carries the app name straight
//                into the TUI top-frame label instead, matching how the
//                other temples caption their own body (Power's
//                "┌─┤ ♪ lifelines ├").
//
// Shared pantheon bars kept: opaque marble stele, 2px paletteFg border, 1px
// inset signature keyline, a cast shadow, a carved-serif crown + name, a box-
// drawing TUI frame top AND bottom (closing 𝄂 barline in gold), a kaomoji
// reading the notification's urgency on a ledger line, gold ink for the one
// short word-tally, radius 0 throughout. Colors ONLY from notes.
//
// Lifecycle: owns its own auto-dismiss timer and a `closeNotification()`
// helper that calls expire()/dismiss() WITHOUT touching `tracked` at all —
// see the comment on `closeNotification` below for why untracking isn't this
// card's job, on any close path (ours or the sender's).
//
// API grounded against the compiled quickshell-service-notifications.qmltypes
// (see AoideNotifications.qml's header for the full grounding note).
//
// Layout: plain Column/Row/Item + explicit `width: parent.width` + anchor-
// between-siblings for flexible text — the SAME idiom GadgetFrame/AoidePanel/
// PowerVitalsGadget use throughout this codebase. Deliberately NOT
// QtQuick.Layouts (ColumnLayout/RowLayout/Layout.fillWidth): the v1 card hit
// a real Qt Quick Layouts sizing-negotiation loop ("RangeError: Maximum call
// stack size exceeded", live in journalctl -u aoide-quickshell.service) the
// moment wrapped body text rendered — nested Layout items under an ancestor
// whose implicitHeight derives from the layout's own implicitHeight.

import QtQuick
import Quickshell.Services.Notifications

Item {
    id: root

    required property var notes
    required property var bridge         // ShellBridge — unused here, part of the fixed slot contract
    required property var notification   // qs::service::notifications::Notification

    readonly property bool isCritical:
        root.notification && root.notification.urgency === NotificationUrgency.Critical
    readonly property bool isLow:
        root.notification && root.notification.urgency === NotificationUrgency.Low

    // this temple's signature — RUST (Tuscan); critical overrides to terracotta
    readonly property color signature: root.isCritical ? root.notes.notifUrgent : root.notes.base0F

    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }
    function urgencyWord() {
        if (root.isCritical) return "critical"
        if (root.isLow) return "low"
        return "normal"
    }
    // Same kana/punctuation vocabulary MoodFaces.qml already proves safe on
    // this system (ω, ｏ, ノ, ｀´ — no Thai/Hangul: an earlier pick used those
    // and rendered as tofu-adjacent junk in live testing).
    function kaomojiFor() {
        if (root.isCritical) return "(ノ｀ｏ´)ノ"   // alarmed — a critical herald
        if (root.isLow) return "(´ω｀)"            // at ease, unhurried
        return "( ・ω・)ノ"                         // attentive, on the case
    }

    // ── Smart payload reading ────────────────────────────────────────────
    // Senders arrive in two shapes:
    //   1. Proper freedesktop: appName + summary (title) + body (message).
    //   2. Terminal-forwarded OSC-9 (kimi via kitty): the WHOLE
    //      "Title: message" string lands in `summary`, `body` is empty,
    //      and appName is the forwarder ("kitty"), not the real program.
    // These helpers are generic — they read whatever shape arrived, for
    // any program, nothing is kimi-specific:
    //   - parentTitle(): desktop-entry id first (file-ish ids like
    //     "firefox.desktop" read worse than "firefox"), then appName,
    //     then the old "notice" floor.
    //   - titleText()/contextText(): when the body is empty, split a
    //     "Title: message" summary at its first ": " so the notif's own
    //     title and its message render as two differentiated tiers
    //     instead of one mashed line. The split only fires when the part
    //     before the colon reads like a real multi-word title ("Re:",
    //     "12:30" and single-word app names are left alone).
    function strippedEntry(e) {
        var s = (e || "").trim()
        if (s.slice(-8) === ".desktop") s = s.slice(0, -8)
        return s
    }
    function parentTitle() {
        var n = root.notification
        if (!n) return "notice"
        var d = strippedEntry(n.desktopEntry)
        if (d.length > 0) {
            return d.length > 28 ? d.slice(0, 27) + "…" : d
        }
        var a = (n.appName || "").trim()
        if (a.length > 0) {
            return a.length > 28 ? a.slice(0, 27) + "…" : a
        }
        return "notice"
    }
    function splitAt() {
        var n = root.notification
        if (!n) return -1
        var s = (n.summary || "").trim()
        if (s.length === 0) return -1
        var b = (n.body || "").trim()
        if (b.length > 0) return -1          // body already carries the message
        var idx = s.indexOf(": ")
        if (idx <= 5 || idx >= 60) return -1 // too short/too long to be a title
        if (s.slice(0, idx).indexOf(" ") <= 0) return -1 // single word — not a title
        return idx
    }
    function titleText() {
        var n = root.notification
        if (!n) return ""
        var s = (n.summary || "").trim()
        var idx = root.splitAt()
        return idx > 0 ? s.slice(0, idx) : s
    }
    function contextText() {
        var n = root.notification
        if (!n) return ""
        var b = (n.body || "").trim()
        if (b.length > 0) return b
        var s = (n.summary || "").trim()
        var idx = root.splitAt()
        return idx > 0 ? s.slice(idx + 2) : ""
    }

    width: 360
    implicitHeight: stele.height + 5   // +5 clears the cast shadow's overhang

    // ── Auto-dismiss (freedesktop notification-spec semantics) ─────────────
    // Critical never gets a timer — it persists until clicked, an action is
    // taken, or the sender itself closes it. Low/Normal: the sender's own
    // expireTimeout (ms) when positive; an explicit 0 from the sender also
    // means "never" (spec); -1 (sender left it to us) falls back to a sane
    // house default of 5s.
    readonly property int autoTimeout: {
        var n = root.notification
        if (!n || root.isCritical) return 0
        if (n.expireTimeout > 0) return n.expireTimeout
        if (n.expireTimeout === 0) return 0
        return 5000
    }
    Timer {
        interval: Math.max(1, root.autoTimeout)
        running: root.autoTimeout > 0
        onTriggered: root.closeNotification(function (n) { n.expire() })
    }

    // Single close path for anything WE initiate (timer expiry, click-dismiss,
    // an action button): just call the notification's own method. Do NOT ever
    // set `n.tracked = false` from this card, on any path.
    //
    // Grounded against server.cpp/notification.cpp: `close(reason)` sets the
    // notification's internal close-reason BEFORE `deleteNotification` runs,
    // which unconditionally removes the notification from the server's own
    // `trackedNotifications` — that removal is what the Repeater in
    // AoideNotifications.qml diffs on to destroy this delegate. Setting
    // `tracked = false` ourselves is therefore always redundant, and on the
    // sender-initiated `CloseNotification` D-Bus path it's actively harmful:
    // that path calls `deleteNotification` directly, WITHOUT going through
    // `close()` first, so the close-reason is still unset (`isTracked()` still
    // true) at the moment `closed` fires. A first version of this file had a
    // `Connections.onClosed` handler that wrote `n.tracked = false` there —
    // for our OWN closes that write was already a harmless no-op (the reason
    // was already set), but for a sender-initiated close it re-entered
    // `setTracked(false)` -> `close(Dismissed)` -> a second, nested
    // `deleteNotification` call, which emits a SECOND `NotificationClosed`
    // D-Bus signal carrying the wrong reason (Dismissed instead of
    // CloseRequested) before the original call's correct signal goes out —
    // protocol-visible misbehavior to any spec-compliant client watching that
    // signal. Removed entirely; nothing else depended on that write.
    function closeNotification(method) {
        var n = root.notification
        if (!n) return
        method(n)
    }

    // cast shadow — shared pantheon idiom (PowerVitalsGadget/MetersGadget) ───
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: root.withA(root.notes.paletteFg, 0.22)
    }

    Rectangle {
        id: stele
        width: parent.width
        anchors.top: parent.top
        radius: 0
        color: root.notes.notifBg
        border.color: root.notes.paletteFg
        border.width: 2
        height: content.implicitHeight + 20

        // inset keyline — signature hue (rust; terracotta while critical)
        Rectangle {
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.signature; border.width: 1
        }

        // Critical breathes — the terracotta-summons pulse idiom shared with
        // the bar's urgent workspace glyph — so a persistent card still
        // reads as LIVE, not stuck.
        SequentialAnimation on opacity {
            running: root.isCritical
            loops: Animation.Infinite
            NumberAnimation { to: 0.8; duration: 700; easing.type: Easing.InOutQuad }
            NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutQuad }
        }

        // ── Click-anywhere-else dismisses ───────────────────────────────
        // Declared FIRST so the action buttons' own MouseAreas — nested
        // inside `content`, declared after — win the hit test over their
        // own area; this one only catches clicks landing outside any button.
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.closeNotification(function (n) { n.dismiss() })
        }

        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 10
            spacing: 4

            // ── ENTABLATURE: crown + carved name + order tag ─────────────
            Item {
                width: parent.width
                height: 24

                Text {
                    id: crown
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "❧"
                    font.family: root.faceSerif
                    font.pixelSize: 20
                    font.bold: true
                    color: root.signature
                }
                Text {
                    anchors.left: crown.right; anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "HERALD"
                    font.family: root.faceSerif
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    font.letterSpacing: 3
                    color: root.notes.notifFg
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ notify ]"
                    font.family: root.faceMono
                    font.pixelSize: 10
                    color: root.withA(root.signature, 0.55)
                }
            }

            // ── Tuscan frieze — a PLAIN double rule, deliberately un-
            // ornamented (no meander/egg-and-dart/triglyph: that bareness
            // IS the Tuscan order, distinct from the other four temples) ──
            Item {
                width: parent.width
                height: 5
                Rectangle { width: parent.width; height: 1; y: 0; color: root.withA(root.signature, 0.7) }
                Rectangle { width: parent.width; height: 1; y: 4; color: root.withA(root.signature, 0.35) }
            }

            // ── box-drawing top frame — app name in the label slot ───────
            Item {
                width: parent.width
                height: 15
                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ " + root.parentTitle() + " ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.signature, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.signature, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 1; anchors.rightMargin: 1
                    anchors.verticalCenter: parent.verticalCenter
                    height: 1; color: root.withA(root.signature, 0.8)
                }
            }

            // ── Title/message cut — one plain rule cleaves the program-
            // name frame (the title) from the message below it, the way a
            // sender's own toast separates its header from its body. The
            // Tuscan frieze above stays the entablature's ornament; this
            // lower rule is the entablature/shaft boundary of the stele ──
            Item {
                width: parent.width
                height: 9
                Rectangle {
                    anchors.top: parent.top; anchors.topMargin: 1
                    width: parent.width; height: 1
                    color: root.withA(root.signature, 0.55)
                }
            }

            // ── Title / context — two differentiated tiers ────────────────
            // titleText()/contextText() above split the mashed OSC-9 shape
            // ("Title: message" in summary, empty body) so the notif's own
            // title and its message render separately, like the sender's own
            // toast. The title is the bold serif carve; the context sits
            // below it, smaller, dimmer, indented behind a faint signature
            // hairline — same voice, clearly second.
            Text {
                width: parent.width
                text: root.titleText()
                textFormat: Text.PlainText
                visible: text.length > 0
                color: root.isCritical ? root.notes.notifUrgent : root.notes.notifFg
                font.family: root.faceSerif
                font.bold: true
                font.pixelSize: 14
                wrapMode: Text.WordWrap
                topPadding: 2
            }
            Item {
                width: parent.width
                visible: contextLabel.text.length > 0
                height: contextLabel.implicitHeight + 5
                Rectangle {
                    anchors.left: parent.left
                    anchors.top: contextLabel.top
                    anchors.bottom: contextLabel.bottom
                    width: 1
                    color: root.withA(root.signature, 0.45)
                }
                Text {
                    id: contextLabel
                    anchors.top: parent.top; anchors.topMargin: 5
                    anchors.left: parent.left; anchors.leftMargin: 10
                    anchors.right: parent.right
                    text: root.contextText()
                    textFormat: Text.PlainText
                    color: root.notes.notifFg
                    opacity: 0.85
                    font.pixelSize: 12
                    lineHeight: 1.3
                    wrapMode: Text.WordWrap
                }
            }

            // ── Actions — each a clickable button; invoke() alone (it closes
            // the notification itself for any non-resident sender) ─────────
            Flow {
                width: parent.width
                spacing: 6
                visible: actionRepeater.count > 0

                Repeater {
                    id: actionRepeater
                    model: root.notification ? root.notification.actions : []
                    delegate: Rectangle {
                        id: actBtn
                        required property var modelData
                        required property int index
                        // the FIRST action is this temple's one laurel standout
                        // (the launcher crowns its selected row the same way)
                        readonly property bool laurel: actBtn.index === 0
                        radius: 0
                        color: root.withA(actBtn.laurel ? root.notes.paletteHot : root.notes.paletteAccent, 0.16)
                        border.width: 1
                        border.color: actBtn.laurel ? root.notes.paletteHot : root.notes.paletteAccent
                        implicitWidth: actLabel.implicitWidth + 16
                        implicitHeight: actLabel.implicitHeight + 8

                        Text {
                            id: actLabel
                            anchors.centerIn: parent
                            text: actBtn.modelData ? actBtn.modelData.text : ""
                            color: root.notes.paletteFg
                            font.pixelSize: 11
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                // invoke() already calls close(Dismissed) itself for any
                                // non-resident notification (notification.cpp) — do NOT
                                // also dismiss() here. A first version did, and every
                                // action click logged "Cannot close destroyed
                                // notification": invoke()'s internal close() had already
                                // fired `closed`/deleteNotification synchronously, so the
                                // second dismiss() landed on an already-retained object
                                // and hit the isRetained() guard. Live-verified fixed.
                                if (actBtn.modelData) actBtn.modelData.invoke()
                            }
                        }
                    }
                }
            }

            // ── ledger line: kaomoji + urgency word in gold ──────────────
            Item {
                width: parent.width
                height: 18
                Rectangle {
                    anchors.top: parent.top
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 1; color: root.withA(root.signature, 0.3)
                }
                Text {
                    anchors.left: parent.left
                    anchors.bottom: parent.bottom
                    text: root.kaomojiFor()
                    font.pixelSize: 11
                    color: root.withA(root.signature, 0.9)
                }
                Text {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    text: root.urgencyWord()
                    font.family: root.faceMono
                    font.pixelSize: 10
                    color: root.isCritical ? root.notes.notifUrgent : root.notes.paletteAccent
                }
            }

            // ── box-drawing bottom frame — closing 𝄂 barline, gold ink ───
            Item {
                width: parent.width
                height: 18
                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + root.urgencyWord() + " ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.notes.notifFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.signature, 0.95)
                }
                Text {
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"
                    font.family: "Noto Music"; font.pixelSize: 16
                    color: root.notes.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    height: 1; color: root.withA(root.signature, 0.55)
                }
            }
        }
    }
}
