import QtQuick

// ── THE COLONNADE · a two-column temple porch (audio in + out) ────────────────
// The bar's audio control: BOTH voices of the machine's sound, drawn as a pair
// of literal fluted classical columns sharing one architrave and one stylobate
// — a temple FRONT, not two gauges. The INPUT (source / microphone) is the left
// column, the OUTPUT (sink / speakers) the right. Each column IS its meter: the
// shaft fills bottom-to-top with its level, capital + fluting + neck intact.
// Click a column to mute it (it falls to RUIN — see below), scroll to set level.
//
// khoa, 2026-07-31 — this is now a SELF-FRAMED MARBLE STELE, the same grammar
// ConductorGadget/TerminalsGadget/MetersGadget/PowerVitalsGadget/NotificationCard
// wear (greek-grammar.md §4; NotificationCard.qml documents the shared shell):
// opaque marble body, 2px paletteFg border, 1px inset signature keyline, a cast
// shadow, a carved-serif crown + name, a Canvas frieze, a box-drawing TUI frame
// top AND bottom (closing 𝄂 barline in gold), radius 0, colour only from notes.
// It no longer wears GadgetFrame — it is hosted bare (StelePopout) and draws all
// its own chrome, converging the outer panel on the rest of the pantheon family
// while keeping its OWN distinct order/hue/crown/motif:
//
//   · ORDER   — a DISTYLE PORCH of TWO orders, one per voice, so the columns are
//               told apart by SHAPE as well as hue:
//                 – MIC → IONIC, aegean (`holoBlue`): a volute (scroll) capital,
//                   echoing the aegean Ionic Terminals temple. The cool input.
//                 – VOL → DORIC, Attic gold (`paletteAccent`): a plain echinus
//                   capital, echoing the gold Doric Conductor. The warm output.
//   · SIGNATURE (chrome hue) — base09 amber: the accentSpread slot none of the
//               dock temples claims (Notification took base0F rust). The two
//               columns already spend gold + aegean on their fills, so the
//               stele's own chrome (crown, keyline, frieze, TUI frame) wears
//               amber to read as the porch's own house, not either column's.
//   · CROWN   — ♫ a beamed pair of notes (two voices) in place of a clef, the
//               way Power's ϟ / Notification's ❧ stand in where no clef fits.
//   · FRIEZE  — a bead-and-reel (astragal) course: round beads spaced by reels,
//               a classical moulding none of the other temples use.
//   · MUTED = RUIN — a muted channel is a BROKEN COLUMN: the capital is gone and
//               the shaft is SNAPPED partway up along a jagged terracotta break,
//               with fallen rubble at its foot — a ruined-temple column, legible
//               as "silenced" from the silhouette alone. No slash, no ghost fill.
//
// House rules: every colour from `notes` roles (zero hex); radius 0; no
// QtQuick.Layouts (plain Item/Row/anchors — the documented sizing-loop hazard).
Item {
    id: root

    required property var notes

    // ── Live levels, fed from the bar's Pipewire seams ──────────────────────
    property int  outPct: 0
    property bool outMuted: false
    property bool outAvail: false
    property int  inPct: 0
    property bool inMuted: false
    property bool inAvail: false

    // ── Control intents, handled by the bar (which owns the sink/source) ────
    signal outToggle()
    signal outAdjust(int delta)
    signal inToggle()
    signal inAdjust(int delta)
    // Popout-retention: true while the pointer is over either column.
    readonly property bool hovered: micCol.hovering || volCol.hovering

    width: 268
    implicitHeight: stele.height + 5      // +5 clears the cast shadow's overhang

    // type voices — shared across the pantheon
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // this stele's signature — AMBER (base09), the porch's own chrome hue
    readonly property color sig: notes.base09
    readonly property color ink: notes.paletteFg

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }
    function mix(a, b, t) {
        var ca = Qt.color(a), cb = Qt.color(b)
        return Qt.rgba(ca.r + (cb.r - ca.r) * t, ca.g + (cb.g - ca.g) * t,
                       ca.b + (cb.b - ca.b) * t, 1.0)
    }
    function stepTone(a) {                              // warm marble step tone
        var w = mix(notes.paletteFg, notes.paletteBg, 0.42)
        return Qt.rgba(w.r, w.g, w.b, a)
    }
    // one mood face reading the whole box (kana/punct proven safe by MoodFaces)
    function kaomojiFor() {
        var mutedCount = (outMuted ? 1 : 0) + (inMuted ? 1 : 0)
        if (mutedCount === 2) return "(-_- )"                 // both hushed
        if (mutedCount === 1) return "( ･_･)"                 // one silenced
        if (Math.max(outPct, inPct) >= 85) return "♪(´▽｀)"   // loud & lively
        return "( ･ω･)ﾉ"                                      // attentive
    }

    // colonnade geometry — one place, so architrave/shafts/tally stay aligned
    readonly property int colW: 70
    readonly property int bayGap: 30
    readonly property int shaftW: 24
    readonly property real breakFrac: 0.52   // where a muted column snaps

    // ══ ONE COLUMN — capital + fluted shaft, or a broken ruin when muted ══════
    component Colonna : Item {
        id: col
        property string order: "doric"       // "doric" | "ionic"
        property int pct: 0
        property bool muted: false
        property bool avail: true
        property color fillHue: root.notes.paletteAccent
        property alias hovering: hoverMa.containsMouse
        signal toggle()
        signal adjust(int delta)

        readonly property bool broken: muted && avail
        readonly property real frac: avail ? Math.max(0, Math.min(1, pct / 100)) : 0
        readonly property color liveHue: !avail ? root.withA(root.ink, 0.22) : fillHue

        // ── the capital — order-specific; GONE when the column is broken ────
        Canvas {
            id: capital
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: col.width
            height: 15
            visible: !col.broken
            readonly property color stroke: col.avail ? root.withA(root.ink, 0.85)
                                                      : root.withA(root.ink, 0.4)
            readonly property string ord: col.order
            onStrokeChanged: requestPaint()
            onOrdChanged: requestPaint()
            onVisibleChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset(); ctx.clearRect(0, 0, width, height)
                if (!visible) return
                ctx.strokeStyle = stroke
                var cx = width / 2
                var shaftHalf = root.shaftW / 2
                if (ord === "ionic") {
                    // Ionic — an abacus over two volute scrolls joined by an ovolo
                    ctx.lineWidth = 1.8
                    var abacusHalf = shaftHalf + 9
                    ctx.beginPath()
                    ctx.moveTo(cx - abacusHalf, 1.5); ctx.lineTo(cx + abacusHalf, 1.5)
                    ctx.stroke()
                    var eyeY = height * 0.52
                    function volute(ex, dir) {
                        ctx.beginPath()
                        var turns = 1.7, steps = 46, r0 = 6.2
                        for (var i = 0; i <= steps; i++) {
                            var t = i / steps
                            var ang = dir * (t * turns * 2 * Math.PI) - Math.PI / 2
                            var r = r0 * (1 - 0.66 * t)
                            var x = ex + r * Math.cos(ang), y = eyeY + r * Math.sin(ang)
                            if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y)
                        }
                        ctx.stroke()
                    }
                    volute(cx - shaftHalf - 3,  1)
                    volute(cx + shaftHalf + 3, -1)
                    ctx.lineWidth = 1.4
                    ctx.beginPath()
                    ctx.moveTo(cx - shaftHalf, 3.5)
                    ctx.quadraticCurveTo(cx, height - 1, cx + shaftHalf, 3.5)
                    ctx.stroke()
                } else {
                    // Doric — a THIN abacus slab over a SHALLOW echinus cushion
                    // (wider than tall) + a neck ring. No lampshade.
                    ctx.lineWidth = 1.3
                    var aH = shaftHalf + 8
                    ctx.beginPath()
                    ctx.moveTo(cx - aH, 2);  ctx.lineTo(cx + aH, 2)          // abacus top
                    ctx.moveTo(cx - aH + 1, 5); ctx.lineTo(cx + aH - 1, 5)   // abacus bottom (thin slab)
                    ctx.stroke()
                    ctx.beginPath()                                          // echinus cushion
                    ctx.moveTo(cx - aH + 1, 5)
                    ctx.quadraticCurveTo(cx - shaftHalf - 3, height - 2, cx - shaftHalf, height - 1)
                    ctx.moveTo(cx + aH - 1, 5)
                    ctx.quadraticCurveTo(cx + shaftHalf + 3, height - 2, cx + shaftHalf, height - 1)
                    ctx.moveTo(cx - shaftHalf, height - 1); ctx.lineTo(cx + shaftHalf, height - 1)  // neck ring
                    ctx.stroke()
                }
            }
        }

        // ── THE SHAFT — the fluted meter; a broken stump + rubble when muted ─
        Item {
            id: shaftBox
            anchors.top: parent.top
            anchors.topMargin: col.broken ? 0 : capital.height + 1
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.shaftW
            readonly property real neck: 6
            readonly property real innerH: height - 2 - neck

            // ── INTACT shaft (unmuted): track · rising fill · flutes · neck ──
            Item {
                anchors.fill: parent
                visible: !col.broken

                Rectangle {                                   // unlit stone track
                    anchors.fill: parent; radius: 0
                    color: root.withA(root.ink, 0.09)
                    border.width: 1
                    border.color: root.withA(root.ink, col.avail ? 0.5 : 0.25)
                }
                Rectangle {                                   // rising FILL — the level
                    id: fill
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.bottom: parent.bottom; anchors.margins: 1
                    height: shaftBox.innerH * col.frac
                    radius: 0
                    color: root.withA(col.liveHue, 0.9)
                    Behavior on height { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
                    Rectangle {                               // meniscus — crisp cap line
                        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                        height: 1; visible: col.frac > 0.01
                        color: root.withA(root.ink, 0.55)
                    }
                }
                Canvas {                                      // fluting + trachelion groove
                    anchors.fill: parent
                    readonly property color groove: root.withA(root.ink, col.avail ? 0.35 : 0.2)
                    onGrooveChanged: requestPaint()
                    onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset(); ctx.clearRect(0, 0, width, height)
                        ctx.strokeStyle = groove; ctx.lineWidth = 1
                        for (var i = 1; i <= 3; i++) {
                            var x = Math.round(width * i / 4) + 0.5
                            ctx.beginPath(); ctx.moveTo(x, 2); ctx.lineTo(x, height - 2); ctx.stroke()
                        }
                        var ny = Math.round(shaftBox.neck) + 0.5
                        ctx.beginPath(); ctx.moveTo(1.5, ny); ctx.lineTo(width - 1.5, ny); ctx.stroke()
                    }
                }
            }

            // ── BROKEN shaft (muted): a snapped stump + jagged break + rubble ─
            Canvas {
                id: ruin
                anchors.fill: parent
                visible: col.broken
                readonly property color stone: root.withA(root.ink, 0.16)
                readonly property color edge:  root.withA(root.ink, 0.5)
                // the fracture — dark STONE, not a red wound (khoa: no red on the
                // broken part of the column; the ruin reads by silhouette).
                readonly property color wound: root.withA(root.ink, 0.72)
                onVisibleChanged: requestPaint()
                onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset(); ctx.clearRect(0, 0, width, height)
                    if (!visible) return
                    var topY = height * (1 - root.breakFrac)       // the stump's break height
                    // a LOW irregular crest atop a solid stub (not a tall zigzag):
                    // short teeth of ±2px so it reads as a fracture, not a bolt.
                    var jag = [topY + 1, topY - 2, topY,     topY - 1,
                               topY + 1, topY - 2, topY + 1, topY - 1, topY]
                    // the stump body (dim stone), capped by the jagged break
                    ctx.beginPath()
                    ctx.moveTo(1, height - 1)
                    ctx.lineTo(1, jag[0])
                    for (var i = 0; i < jag.length; i++)
                        ctx.lineTo(1 + (width - 2) * (i / (jag.length - 1)), jag[i])
                    ctx.lineTo(width - 1, height - 1)
                    ctx.closePath()
                    ctx.fillStyle = stone; ctx.fill()
                    // side edges of the stump
                    ctx.strokeStyle = edge; ctx.lineWidth = 1
                    ctx.beginPath()
                    ctx.moveTo(1.5, height - 1); ctx.lineTo(1.5, jag[0])
                    ctx.moveTo(width - 1.5, height - 1); ctx.lineTo(width - 1.5, jag[jag.length - 1])
                    ctx.stroke()
                    // fluting on the stump (below the break)
                    ctx.strokeStyle = root.withA(root.ink, 0.32)
                    for (var f = 1; f <= 3; f++) {
                        var fx = Math.round(width * f / 4) + 0.5
                        var fyTop = jag[Math.min(jag.length - 1, Math.round((jag.length - 1) * f / 4))] + 3
                        ctx.beginPath(); ctx.moveTo(fx, fyTop); ctx.lineTo(fx, height - 2); ctx.stroke()
                    }
                    // the fracture — the jagged break line, in dark STONE (not red)
                    ctx.strokeStyle = wound; ctx.lineWidth = 1.6
                    ctx.beginPath()
                    for (var j = 0; j < jag.length; j++) {
                        var jx = 1 + (width - 2) * (j / (jag.length - 1))
                        if (j === 0) ctx.moveTo(jx, jag[j]); else ctx.lineTo(jx, jag[j])
                    }
                    ctx.stroke()
                    // (the fallen debris is drawn by the wider `rubble` field
                    //  below, so the pieces can scatter across the whole foot.)
                }
            }
        }

        // ── SCATTERED RUBBLE — the toppled capital + chips strewn across the
        //    whole foot of a broken column (spans the bay, not just the shaft,
        //    so the pieces scatter AROUND the base, tumbled at varied angles) ─
        Canvas {
            id: rubble
            anchors.bottom: parent.bottom
            anchors.bottomMargin: -1
            anchors.horizontalCenter: parent.horizontalCenter
            width: col.width
            height: 24
            visible: col.broken
            onVisibleChanged: requestPaint()
            onWidthChanged: requestPaint(); onHeightChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset(); ctx.clearRect(0, 0, width, height)
                if (!visible) return
                var fill = root.withA(root.ink, 0.2)
                var edge = root.withA(root.ink, 0.62)
                var cx = width / 2
                function chip(x, y, w, h, rot) {
                    ctx.save(); ctx.translate(x, y); ctx.rotate(rot)
                    ctx.fillStyle = fill; ctx.fillRect(-w / 2, -h / 2, w, h)
                    ctx.strokeStyle = edge; ctx.lineWidth = 1; ctx.strokeRect(-w / 2, -h / 2, w, h)
                    ctx.restore()
                }
                // the fallen capital — a wide abacus slab tumbled off to one side,
                // its order still legible (a volute scroll on its end for Ionic)
                ctx.save()
                ctx.translate(cx - 21, height * 0.52); ctx.rotate(-0.52)
                ctx.fillStyle = fill; ctx.fillRect(-9, -5, 18, 10)
                ctx.strokeStyle = edge; ctx.lineWidth = 1; ctx.strokeRect(-9, -5, 18, 10)
                if (col.order === "ionic") {
                    ctx.beginPath(); ctx.arc(-9, 0, 3.0, 0, 2 * Math.PI); ctx.stroke()
                    ctx.beginPath(); ctx.arc(-9, 0, 1.2, 0, 2 * Math.PI); ctx.stroke()
                }
                ctx.restore()
                // drum fragments + chips, scattered across the foot at varied angles
                chip(cx + 20, height * 0.60, 10, 6,  0.50)
                chip(cx + 30, height * 0.40,  6, 5, -0.35)
                chip(cx +  4, height * 0.72,  7, 5,  0.18)
                chip(cx + 11, height * 0.46,  5, 4, -0.62)
                chip(cx - 30, height * 0.66,  6, 5,  0.40)
                chip(cx -  8, height * 0.78,  5, 4, -0.15)
            }
        }

        MouseArea {
            id: hoverMa
            anchors.fill: parent
            hoverEnabled: true
            enabled: col.avail
            cursorShape: Qt.PointingHandCursor
            onClicked: col.toggle()
            onWheel: { col.adjust(wheel.angleDelta.y > 0 ? 2 : -2); wheel.accepted = true }
        }
    }

    // cast shadow — shared pantheon idiom ───────────────────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: root.withA(root.ink, 0.22)
    }

    // ══ THE STELE — opaque marble body, border, inset keyline ════════════════
    Rectangle {
        id: stele
        width: parent.width
        anchors.top: parent.top
        radius: 0
        color: root.notes.paletteBg
        border.color: root.ink
        border.width: 2
        height: content.implicitHeight + 20

        Rectangle {                                       // inset amber keyline
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.sig; border.width: 1
        }

        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 11
            spacing: 4

            // ── ENTABLATURE: crown + carved name + order tag ─────────────────
            Item {
                width: parent.width; height: 24
                Text {
                    id: crown
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -2
                    text: "♫"; font.family: root.faceMusic; font.pixelSize: 22
                    color: root.sig
                }
                Text {
                    anchors.left: crown.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "AUDIO"
                    font.family: root.faceSerif; font.pixelSize: 15
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: root.ink
                }
                Text {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "[ pipewire ]"
                    font.family: root.faceMono; font.pixelSize: 10
                    color: root.withA(root.sig, 0.9)
                }
            }

            // ── bead-and-reel (astragal) frieze, in amber ───────────────────
            Canvas {
                id: frieze
                width: parent.width; height: 10
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset(); ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = root.withA(root.sig, 0.85)
                    ctx.fillStyle = root.withA(root.sig, 0.85)
                    ctx.lineWidth = 1
                    var cy = height / 2, period = 13
                    for (var x = 3; x < width - 3; x += period) {
                        ctx.beginPath(); ctx.arc(x, cy, 2.4, 0, 2 * Math.PI); ctx.fill()   // round bead
                        // the reel — a spool/lozenge between beads (double-cone), so
                        // the course reads as carved bead-AND-reel, not a dotted rule
                        var rx = x + period / 2
                        ctx.beginPath()
                        ctx.moveTo(rx - 3.5, cy); ctx.lineTo(rx, cy - 2.2)
                        ctx.lineTo(rx + 3.5, cy); ctx.lineTo(rx, cy + 2.2)
                        ctx.closePath(); ctx.fill()
                    }
                }
            }

            // ── box-drawing top frame ───────────────────────────────────────
            Item {
                width: parent.width; height: 15
                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ levels ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter; anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }

            // ══ THE PORCH — architrave · two columns · stylobate · tallies ══
            Item {
                id: porch
                width: parent.width
                height: 150

                // shared architrave beam carrying the MIC / VOL inscriptions
                Item {
                    id: architrave
                    anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right
                    height: 22
                    readonly property real inset: (parent.width - (root.colW * 2 + root.bayGap)) / 2
                    Rectangle {                               // cornice (top rule)
                        anchors.top: parent.top
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: architrave.inset; anchors.rightMargin: architrave.inset
                        height: 1; color: root.withA(root.ink, 0.3)
                    }
                    Row {
                        anchors.top: parent.top; anchors.topMargin: 3
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: root.bayGap
                        Text {
                            width: root.colW; horizontalAlignment: Text.AlignHCenter
                            text: "MIC"; font.family: root.faceSerif; font.pixelSize: 12
                            font.weight: Font.DemiBold; font.letterSpacing: 3
                            color: root.inAvail ? root.ink : root.withA(root.ink, 0.4)
                        }
                        Text {
                            width: root.colW; horizontalAlignment: Text.AlignHCenter
                            text: "VOL"; font.family: root.faceSerif; font.pixelSize: 12
                            font.weight: Font.DemiBold; font.letterSpacing: 3
                            color: root.outAvail ? root.ink : root.withA(root.ink, 0.4)
                        }
                    }
                    Rectangle {                               // architrave (the carried beam)
                        anchors.bottom: parent.bottom
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: architrave.inset; anchors.rightMargin: architrave.inset
                        height: 1.5; color: root.withA(root.ink, 0.5)
                    }
                }

                // the shared stepped stylobate the columns stand on
                Item {
                    id: stylo
                    anchors.bottom: tallyRow.top; anchors.bottomMargin: 3
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 8
                    readonly property real inset: (parent.width - (root.colW * 2 + root.bayGap)) / 2
                    Rectangle {
                        anchors.top: parent.top
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: stylo.inset - 2; anchors.rightMargin: stylo.inset - 2
                        height: 4; color: root.stepTone(0.85)
                    }
                    Rectangle {
                        anchors.top: parent.top; anchors.topMargin: 4
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.leftMargin: stylo.inset - 9; anchors.rightMargin: stylo.inset - 9
                        height: 3; color: root.stepTone(0.6)
                    }
                }

                // the two columns, between architrave and stylobate
                Row {
                    anchors.top: architrave.bottom; anchors.bottom: stylo.top
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: root.bayGap
                    Colonna {
                        id: micCol
                        width: root.colW; height: parent.height
                        order: "ionic"
                        pct: root.inPct; muted: root.inMuted; avail: root.inAvail
                        fillHue: root.notes.holoBlue          // aegean — cool input
                        onToggle: root.inToggle()
                        onAdjust: function(d) { root.inAdjust(d) }
                    }
                    Colonna {
                        id: volCol
                        width: root.colW; height: parent.height
                        order: "doric"
                        pct: root.outPct; muted: root.outMuted; avail: root.outAvail
                        fillHue: root.notes.paletteAccent     // Attic gold — warm output
                        onToggle: root.outToggle()
                        onAdjust: function(d) { root.outAdjust(d) }
                    }
                }

                // the tally row — level percent / MUTED / absent, under each column
                Row {
                    id: tallyRow
                    anchors.bottom: parent.bottom
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: root.bayGap
                    component Tally : Text {
                        property bool muted: false
                        property bool avail: true
                        property int pct: 0
                        width: root.colW; horizontalAlignment: Text.AlignHCenter
                        text: !avail ? "—" : (muted ? "MUTED" : pct + "%")
                        font.family: muted ? root.faceSerif : root.faceMono
                        font.pixelSize: muted ? 11 : 13
                        font.letterSpacing: muted ? 3 : 0
                        font.weight: muted ? Font.DemiBold : Font.Normal
                        color: muted ? root.notes.paletteUrgent
                                     : (avail ? root.ink : root.withA(root.ink, 0.4))
                    }
                    Tally { pct: root.inPct;  muted: root.inMuted;  avail: root.inAvail }
                    Tally { pct: root.outPct; muted: root.outMuted; avail: root.outAvail }
                }
            }

            // ── ledger line: kaomoji reads the box · amber detail ───────────
            Item {
                width: parent.width; height: 16
                Rectangle {
                    anchors.top: parent.top
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 1; color: root.withA(root.sig, 0.3)
                }
                Text {
                    anchors.left: parent.left; anchors.bottom: parent.bottom
                    text: root.kaomojiFor()
                    font.pixelSize: 11
                    color: root.withA(root.sig, 0.9)
                }
                Text {
                    anchors.right: parent.right; anchors.bottom: parent.bottom
                    text: "scroll · set   click · mute"
                    font.family: root.faceMono; font.pixelSize: 9
                    color: root.withA(root.ink, 0.5)
                }
            }

            // ── box-drawing bottom frame — closing 𝄂 barline, gold ink ──────
            Item {
                width: parent.width; height: 18
                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ out " + (root.outAvail ? (root.outMuted ? "×" : root.outPct) : "—")
                          + " · in " + (root.inAvail ? (root.inMuted ? "×" : root.inPct) : "—") + " ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.ink, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"; font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.sig, 0.95)
                }
                Text {
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"; font.family: root.faceMusic; font.pixelSize: 16
                    color: root.notes.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter; anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.sig, 0.55)
                }
            }
        }
    }
}
