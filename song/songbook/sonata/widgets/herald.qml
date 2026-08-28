// herald.qml — sonata's "herald" slot: the notification POPUP. Hosted by
// `SurfaceSlot` (shell.qml), so this root is a `PanelWindow` that owns its
// own layer surface, the way powermenu.qml does — hidden until the ledger
// file carries something to cry.
//
// HERALD — the herald is the voice that interrupts the performance: a
// messenger walks on mid-scene and reads out what happened offstage. Each
// notification is one small SELF-FRAMED STELE in the pantheon family the
// bar's popouts wear (opaque marble body, 2px ink border, 1px inset
// signature keyline, cast shadow, radius 0, colour only from livery.*),
// stacked under the bar at the screen's top-right, newest first.
//
//   · SIGNATURE — RUST `livery.base0F`. The family register: calendar wears
//     clay base09, the colonnade murex base0E, power murex in the dock; rust
//     is the notification family's own hue (AudioColonnade's signature
//     grounding names it so), and no bar-popout stele wears it today.
//   · CROWN — ❧ U+2767, the rotated floral heart, pinned EXPLICITLY to
//     "Linux Libertine O": Noto Serif has no U+2767 and an unpinned run
//     falls back to a fake-bolded blob (live-verified on this stack — the
//     same class of lesson bar.qml records for ‖ and 𝄂). Regular weight.
//   · REGISTERS — urgency is the card's keyline: low sits dim, normal
//     carries the rust signature, critical burns terracotta
//     (paletteUrgent) and marks its tag `[ forte ]`. A SUMMONS — a
//     permission ask from an agent session — wears the Attic-gold keyline
//     (the open/working register this family already uses for an active
//     thing awaiting a hand) and carries real, hit-tested APPROVE / DENY
//     chips: two separate MouseAreas on two drawn chips (calendar.qml's
//     pressable-edge control idiom), gold and terracotta, because the whole
//     reason this widget exists is that a drawn "deny" that isn't its own
//     hit region is a lie.
//   · IMAGES live INSIDE the frame: a sender icon/image is a bordered
//     40px plate beside the text band — a mark, never a poster
//     (PreserveAspectCrop clamps a screenshot to the plate).
//   · PROGRESS is a livery gauge inside the frame — the hairline track +
//     rising fill idiom the colonnade's channel rows use, in aegean
//     holoBlue (the information hue), with the mono figure beside it.
//
// ── Data ───────────────────────────────────────────────────────────────────
// Read-only: state/stage/herald.json via FileView (the sessions.json watch
// pattern, bar.qml). Newest is LAST in the array; capped at 20 daemon-side.
// Sender text is UNTRUSTED DATA — every Text rendering it is PlainText, so
// a body carrying `<b>` displays literally.
//
// THE POPUP OWNS THE DISMISS CLOCK. Each record carries `timeoutMs`
// (0 = never — critical and every summons). Expiry is LOCAL: a lapsed card
// leaves this popup but stays in the file for the dock's herald-center
// ledger; nothing daemon-side ever expires a record. Deadlines are pinned
// per id at first sight, so a file rewrite (another arrival) never restarts
// a card's clock.
//
// Write-side, socket only (ShellBridge, the powermenu idiom — QML never
// touches the file):
//   · click a toast            → { cmd: "heralddismiss", id: <record id> }
//   · approve / deny a summons → { cmd: "heraldverdict", id: <sessionId>,
//                                  verdict: "approve" | "deny" }
//     (the daemon auto-dismisses the card on a verdict — no double send)

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected by SurfaceSlot) ───────────────
    required property var livery
    required property var bridge

    // ── Type voices (the pantheon family) ──────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation
    readonly property string faceCrown: "Linux Libertine O"         // ❧ lives here ONLY

    // this family's signature — RUST (base0F); see the header
    readonly property color sig: livery.base0F
    readonly property color ink: livery.paletteFg
    readonly property color aegean: livery.holoBlue      // information (progress)
    readonly property color gold: livery.paletteAccent   // the summons register
    readonly property color fire: livery.paletteUrgent   // critical / deny

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Geometry ───────────────────────────────────────────────────────────
    readonly property int cardW: 296
    readonly property int shadowR: 4     // cast-shadow right strip
    readonly property int shadowB: 5     // cast-shadow bottom strip
    readonly property int stackGap: 10
    readonly property int maxCards: 5    // older unexpired cards wait in the ledger

    // ── The ledger file — read-only, watched (bar.qml's sessions.json idiom) ─
    // herald.json is a CONDUCTING file (CONTRACTS.md §4) — state/stage/,
    // not song/stage/.
    readonly property string heraldPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/stage/herald.json"
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
            } catch (e) { /* absent/garbage → hold what we have */ }
        }
        Component.onCompleted: heraldFile.reload()
    }

    // ── The dismiss clock — deadlines pinned per ARRIVAL at FIRST sight ────
    // A rewrite of herald.json (any new arrival) recreates every delegate;
    // a Timer living in the delegate would restart each card's clock on
    // every neighbour's arrival. So the clock lives here: one map id →
    // epoch-ms deadline (0 = never), one 500ms sweep while cards show.
    //
    // Keyed by id AND `receivedAt`, not id alone: dunst's `stack_duplicates`
    // (modules/dendrites/dunst.nix) reuses ONE notification id for a repeated
    // identical summary+body — e.g. two back-to-back "nothing to reap" reap
    // pings carry the same DUNST_ID. `aoide herald push` still stamps a FRESH
    // `receivedAt` on every call (its own wall clock, `herald.rs`), so the
    // record in the file genuinely changed even though its id didn't. Without
    // the receivedAt check, `lapsed[id]` latched TRUE the first time that id's
    // card expired would carry forward onto every later push reusing that id
    // — the popup would silently stop showing that reap result forever, which
    // is exactly the "ran with nothing to refresh, no ping" symptom this
    // fixes. A push whose receivedAt is unchanged (a redundant FileView
    // reload of the same arrival, no real new event) still inherits its
    // deadline/lapsed state untouched.
    property var deadlines: ({})
    property var lapsed: ({})
    property var arrivals: ({})    // id -> receivedAt of the arrival currently tracked
    property int epoch: 0          // bumped to re-run visibleRecords

    function ingest(arr) {
        var now = Date.now()
        var dl = {}
        var lp = {}
        var av = {}
        for (var i = 0; i < arr.length; i++) {
            var r = arr[i]
            if (!r || r.id === undefined) continue
            var id = "" + r.id
            var receivedAt = "" + (r.receivedAt || "")
            var sameArrival = root.arrivals[id] !== undefined && root.arrivals[id] === receivedAt
            var t = (r.timeoutMs !== undefined) ? (r.timeoutMs | 0) : 0
            dl[id] = (sameArrival && id in root.deadlines) ? root.deadlines[id]
                                            : (t > 0 ? now + t : 0)
            if (sameArrival && root.lapsed[id]) lp[id] = true   // stays lapsed only within the SAME arrival
            av[id] = receivedAt
        }
        root.deadlines = dl                          // dropped ids pruned
        root.lapsed = lp
        root.arrivals = av
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

    // Unlapsed, capped, and ordered for a BOTTOM-anchored stack: the ledger
    // arrives oldest→newest, so the walk runs backwards to SELECT the newest
    // `maxCards`, then reverses so the newest sits LAST — i.e. at the bottom
    // of the column, hard against the corner the herald cries from. (A Column
    // has no vertical layout direction to flip; the order is the only lever.)
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

    function dismiss(id) {
        root.bridge.sendCommand({ cmd: "heralddismiss", id: "" + id })
    }
    // Wave a card off the SCREEN without answering it — the summons' version
    // of a dismiss. It only marks the id lapsed (the same lever the timeout
    // clock pulls), so the ledger row survives: the agent is still blocked
    // and the dock's herald-center must still show the pending verdict.
    function wave(id) {
        var lp = root.lapsed
        lp["" + id] = true
        root.lapsed = lp
        root.epoch++
    }
    function verdict(sessionId, word) {
        root.bridge.sendCommand({ cmd: "heraldverdict",
                                  id: "" + sessionId, verdict: word })
    }

    // ── The urgency register, resolved in one place ────────────────────────
    function isSummons(r) { return r && r.kind === "summons" }
    function keylineOf(r) {
        if (root.isSummons(r)) return root.withA(root.gold, 0.9)
        var u = r ? ("" + r.urgency) : "normal"
        if (u === "critical") return root.withA(root.fire, 0.9)
        if (u === "low") return root.withA(root.sig, 0.45)
        return root.withA(root.sig, 0.8)
    }
    function tagOf(r) {
        if (root.isSummons(r)) return "[ verdict ]"
        var u = r ? ("" + r.urgency) : "normal"
        if (u === "critical") return "[ forte ]"
        if (u === "low") return "[ piano ]"
        return "[ mezzo ]"
    }
    function tagColorOf(r) {
        if (root.isSummons(r)) return root.withA(root.gold, 0.95)
        var u = r ? ("" + r.urgency) : "normal"
        if (u === "critical") return root.withA(root.fire, 0.95)
        if (u === "low") return root.withA(root.ink, 0.45)
        return root.withA(root.sig, 0.9)
    }

    // ── Layer-shell surface — BOTTOM-right (khoa, 2026-08-17) ──────────────
    // The corner the herald cries from. Nothing is anchored to the top any
    // more, so the bar's reserve is irrelevant here; the stack below grows
    // UPWARD out of this corner.
    anchors { bottom: true; right: true }
    margins { bottom: 10; right: 12 }
    exclusiveZone: 0
    color: "transparent"
    visible: root.visibleRecords.length > 0
    implicitWidth: root.cardW + root.shadowR + 1
    implicitHeight: Math.max(1, stack.implicitHeight)
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-herald"

    // ── The stack — newest card nearest the corner, growing upward ─────────
    // Bottom-anchored, so the LAST child sits at the corner and the column
    // grows upward as cards arrive; `visibleRecords` is ordered newest-last to
    // suit. Column has no `verticalLayoutDirection` (checked the hard way — it
    // silently fails the whole surface to load), so the model order is the
    // only lever for this.
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
                readonly property string bodyText: rec ? ("" + (rec.body || "")) : ""
                readonly property string iconPath: rec ? ("" + (rec.icon || "")) : ""
                readonly property int progress:
                    (rec && rec.progress !== undefined) ? (rec.progress | 0) : -1
                readonly property bool critical: rec && ("" + rec.urgency) === "critical"

                width: root.cardW + root.shadowR
                height: card.height + root.shadowB

                // arrival — the card fades in; reflow handles the slide
                opacity: 0
                Component.onCompleted: riseAnim.restart()
                NumberAnimation {
                    id: riseAnim
                    target: slot; property: "opacity"
                    from: 0; to: 1; duration: 180; easing.type: Easing.OutCubic
                }

                // cast shadow — the visible L only (calendar.qml's correction)
                Rectangle {
                    x: card.width; y: root.shadowB
                    width: root.shadowR; height: card.height
                    radius: 0; color: root.withA(root.ink, 0.22)
                }
                Rectangle {
                    x: root.shadowR; y: card.height
                    width: card.width - root.shadowR; height: root.shadowB
                    radius: 0; color: root.withA(root.ink, 0.22)
                }

                // ══ ONE CARD — a small stele ═══════════════════════════════
                Rectangle {
                    id: card
                    width: root.cardW
                    height: col.implicitHeight + 20
                    radius: 0
                    color: root.livery.paletteBg
                    border.color: root.ink
                    border.width: 2

                    // the whole card is its own dismiss chit — declared FIRST
                    // so the summons chips' own MouseAreas (later) win the hit
                    // test (the colonnade's catcher-first house rule), i.e.
                    // this catches every click OUTSIDE approve/deny. A toast
                    // clicked anywhere is gone for good; a summons clicked off
                    // its chips is only waved off the screen — it stays in the
                    // ledger, because the agent behind it is still waiting.
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: slot.summons ? root.wave(slot.rec.id)
                                                : root.dismiss(slot.rec.id)
                    }

                    // inset keyline — the urgency register (see header).
                    // CRITICAL BREATHES HERE, not on the card: pulsing the
                    // whole stele's opacity let the desktop bleed through the
                    // marble mid-cycle (captured — terminal text read
                    // straight through the card), so the pulse rides the INK
                    // — keyline and crown — the way the bar pulses a glyph,
                    // never a body. alwaysRunToEnd settles it opaque.
                    Rectangle {
                        id: keyline
                        anchors.fill: parent; anchors.margins: 4
                        radius: 0; color: "transparent"
                        border.color: root.keylineOf(slot.rec); border.width: 1
                        SequentialAnimation on opacity {
                            running: slot.critical
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutQuad }
                            NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutQuad }
                        }
                    }

                    Column {
                        id: col
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 10
                        spacing: 5

                        // ── crown row: ❧ + sender + register tag ───────────
                        Item {
                            width: parent.width
                            height: 18
                            Text {
                                id: crown
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: "❧"
                                font.family: root.faceCrown   // pinned — see header
                                font.pixelSize: 17
                                color: slot.critical ? root.fire : root.sig
                                // the crown breathes with the keyline (see it)
                                opacity: slot.critical ? keyline.opacity : 1
                            }
                            Text {
                                anchors.left: crown.right; anchors.leftMargin: 7
                                anchors.right: regTag.left; anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: slot.rec ? ("" + (slot.rec.app || "system")) : ""
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font.family: root.faceSerif
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                                font.letterSpacing: 2
                                color: root.ink
                            }
                            Text {
                                id: regTag
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: root.tagOf(slot.rec)
                                font.family: root.faceMono
                                font.pixelSize: 9
                                color: root.tagColorOf(slot.rec)
                            }
                        }

                        // the cleave — one signature hairline under the crown
                        Rectangle {
                            width: parent.width; height: 1
                            color: root.withA(root.sig, 0.55)
                        }

                        // ── the message band: icon/image plate INSIDE the
                        // frame, beside the text — a mark, never a poster.
                        // The PLATE is the design element and its slot is
                        // FIXED (64x44), so the card reads as the same
                        // widget whatever arrives; the image sits centred on
                        // the mount board at PreserveAspectFit — a
                        // screenshot must read as a legible thumbnail of
                        // the whole capture, never a cropped smear of one
                        // corner (Crop was tried first and rejected for
                        // exactly that). Painted size is derived from the
                        // DECODED raster's implicit size, capped at 1x — a
                        // 22px icon paints at 22px on the board, never
                        // inflated to fill the slot — and `sourceSize`
                        // bounds the decode, so a 3000x1600 capture holds a
                        // ~128px raster, not 4.8M pixels. ───────────────────
                        Item {
                            width: parent.width
                            height: Math.max(textCol.implicitHeight,
                                             plate.visible ? plate.height : 0)

                            Rectangle {
                                id: plate
                                visible: slot.iconPath.length > 0
                                anchors.left: parent.left
                                anchors.top: parent.top
                                width: 64; height: 44
                                radius: 0
                                color: root.withA(root.ink, 0.06)
                                border.width: 1
                                border.color: root.withA(root.ink, 0.35)
                                Image {
                                    id: plateImg
                                    anchors.centerIn: parent
                                    source: slot.iconPath.length > 0
                                            ? "file://" + slot.iconPath : ""
                                    fillMode: Image.PreserveAspectFit
                                    asynchronous: true
                                    smooth: true
                                    mipmap: true
                                    sourceSize: Qt.size(128, 90)   // decode bound
                                    // fit within the mount, never upscale
                                    readonly property real fitS:
                                        (implicitWidth > 0 && implicitHeight > 0)
                                        ? Math.min(1, Math.min(60 / implicitWidth,
                                                               40 / implicitHeight))
                                        : 1
                                    width: Math.max(1, Math.round(implicitWidth * fitS))
                                    height: Math.max(1, Math.round(implicitHeight * fitS))
                                }
                            }

                            Column {
                                id: textCol
                                anchors.left: plate.visible ? plate.right : parent.left
                                anchors.leftMargin: plate.visible ? 10 : 0
                                anchors.right: parent.right
                                spacing: 2
                                Text {
                                    width: parent.width
                                    text: slot.rec ? ("" + (slot.rec.summary || "")) : ""
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    font.family: root.faceSerif
                                    font.pixelSize: 13
                                    font.bold: true
                                    color: root.ink
                                }
                                Text {
                                    width: parent.width
                                    visible: slot.bodyText.length > 0
                                    text: slot.bodyText
                                    textFormat: Text.PlainText   // untrusted — literal
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 4
                                    elide: Text.ElideRight
                                    font.family: root.faceSerif
                                    font.pixelSize: 11
                                    color: root.withA(root.ink, 0.75)
                                }
                            }
                        }

                        // ── the gauge — progress as a livery meter, aegean
                        // (the information hue), mono figure beside it ──────
                        Item {
                            width: parent.width
                            height: 12
                            visible: slot.progress >= 0
                            Item {
                                id: track
                                anchors.left: parent.left
                                anchors.right: gaugeFig.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                height: 5
                                Rectangle {
                                    anchors.fill: parent; radius: 0
                                    color: root.withA(root.ink, 0.09)
                                    border.width: 1
                                    border.color: root.withA(root.ink, 0.35)
                                }
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.top: parent.top
                                    anchors.bottom: parent.bottom
                                    anchors.margins: 1
                                    width: (parent.width - 2)
                                           * Math.max(0, Math.min(1, slot.progress / 100))
                                    radius: 0
                                    color: root.withA(root.aegean, 0.9)
                                    Behavior on width {
                                        NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
                                    }
                                }
                            }
                            Text {
                                id: gaugeFig
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 34
                                horizontalAlignment: Text.AlignRight
                                text: Math.max(0, Math.min(100, slot.progress)) + "%"
                                font.family: root.faceMono
                                font.pixelSize: 10
                                color: root.withA(root.ink, 0.8)
                            }
                        }

                        // ── the verdict — TWO drawn chips, TWO hit regions.
                        // The pressable-edge control idiom (calendar.qml's
                        // mode chip), one gold and one terracotta, separated
                        // by clear air; each MouseArea is its own chip's and
                        // nothing else's. Declared after the card's dismiss
                        // catcher, so these win the hit test. ───────────────
                        Item {
                            width: parent.width
                            height: 24
                            visible: slot.summons

                            Rectangle {
                                id: approveChip
                                anchors.right: denyChip.left
                                anchors.rightMargin: 14
                                anchors.verticalCenter: parent.verticalCenter
                                width: 92; height: 22
                                radius: 0
                                color: approveMa.containsMouse
                                       ? root.withA(root.gold, 0.20) : "transparent"
                                border.width: 1
                                border.color: root.withA(root.gold,
                                                  approveMa.containsMouse ? 0.95 : 0.6)
                                Text {
                                    anchors.centerIn: parent
                                    text: "approve"
                                    font.family: root.faceMono
                                    font.pixelSize: 10
                                    font.letterSpacing: 1
                                    color: root.gold
                                    opacity: approveMa.containsMouse ? 1.0 : 0.9
                                }
                                MouseArea {
                                    id: approveMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(slot.rec.sessionId, "approve")
                                }
                            }
                            Rectangle {
                                id: denyChip
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 92; height: 22
                                radius: 0
                                color: denyMa.containsMouse
                                       ? root.withA(root.fire, 0.20) : "transparent"
                                border.width: 1
                                border.color: root.withA(root.fire,
                                                  denyMa.containsMouse ? 0.95 : 0.6)
                                Text {
                                    anchors.centerIn: parent
                                    text: "deny"
                                    font.family: root.faceMono
                                    font.pixelSize: 10
                                    font.letterSpacing: 1
                                    color: root.fire
                                    opacity: denyMa.containsMouse ? 1.0 : 0.9
                                }
                                MouseArea {
                                    id: denyMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(slot.rec.sessionId, "deny")
                                }
                            }
                        }

                        // ── closing rule — the small card ends on the 𝄂 ────
                        Item {
                            width: parent.width
                            height: 12
                            Text {
                                id: closeBar
                                anchors.right: parent.right; anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                text: "\u{1D102}"
                                font.family: root.faceMusic; font.pixelSize: 14
                                color: root.livery.paletteAccent
                            }
                            Rectangle {
                                anchors.left: parent.left; anchors.right: closeBar.left
                                anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                height: 1; color: root.withA(root.sig, 0.55)
                            }
                        }
                    }
                }
            }
        }
    }
}
