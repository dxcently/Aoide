// AoideLauncher.qml — the app launcher (surface #3, "launcher").
//
// Quickshell-native application launcher — replaces rofi. A summoned overlay:
// hidden by default, it drops onto the OVERLAY layer with an EXCLUSIVE keyboard
// grab, filters the installed .desktop entries as you type, and launches the
// chosen one. Trigger is a Hyprland GLOBAL shortcut (aoide:launcher) the
// launcher registers in-process — the compositor binds SUPER+SPACE to it (see
// modules/facets/compositor: `bind = SUPER, SPACE, global, aoide:launcher`).
// Escape / a click on the scrim / launching an app dismisses it. NONE of that
// wiring has ever changed across the chrome redesigns — the trigger name,
// keybind, and compositor namespace (`aoide-launcher`) are stable contracts.
//
// ── Why GlobalShortcut, not the bridge ──────────────────────────────────────
// ShellBridge is OUTBOUND-only (QML → shellbridge socket), and the old
// `aoide shell launcher toggle` CLI verb the keybind used to call is an
// unimplemented stub (not in the aoide command schema). Rather than build a new
// inbound CLI+socket path, the launcher registers a Hyprland global shortcut and
// receives the keypress IN-PROCESS — the cleanest inbound trigger for a surface
// that lives in the shell. No new aoided verb, no SocketServer.
//
// ── Enumerate + launch (exec discipline) ────────────────────────────────────
// Apps come from Quickshell's built-in DesktopEntries (parsed XDG .desktop
// files); launching calls DesktopEntry.execute() directly — the same
// Quickshell-native-service idiom the rest of the shell uses for side effects.
// The Grimoire's ledger write follows the same idiom: QML writes its stage
// file directly, no new aoided verb.
//
// ══ THE GRIMOIRE — a rectangular open book with an incantation strip ════════
// Fourth chrome (khoa, 2026-07-31). Iteration history, all same-day: the
// permanent −6° tilt was scrapped; a slab-with-book-trim rebuild was rejected
// as "exactly like the old one"; a curved-silhouette build with a Pantheon
// holoBlue depth stack and leader-line callouts was then cut back on khoa's
// steer: "remove the pantheon holo-blue framing stuff — make it a rectangular
// book." This is that build. No depth-stack copies, no connector ticks, no
// callout leaders — the book stands on its own.
//
//   • THE BOOK — two clean RECTANGULAR glass pages side by side (the Aero
//     body: translucent `paletteBg` over compositor blur, 2px ink border,
//     gloss sheen, gold margin keylines — the illuminated page borders).
//     They meet at a double-hairline crease with gutter shading receding
//     into the fold. Below the spread, the book's thickness: a fading stack
//     of ink fore-edge rules closed by the COVER BOARDS — a glass band
//     wider than the pages carrying the spine inscription
//     "GRIMOIRE · βίβλος" — with the accent ribbon slipping out of the
//     fold and dangling past them.
//   • THE SEARCH BAR — a separate floating glass incantation strip above
//     the book, with its own glow. You speak into it; the book answers.
//   • ROWS are ruled manuscript lines (no row boxes): each entry sits on a
//     hairline rule; the SELECTED line's rule ignites in `paletteHot` and
//     its margin dot becomes the sung ♪ — the one neon element, nowhere else.
//   • NAVIGATION is book-native: thumb-index tabs on the right fore-edge
//     (α β γ … per alphabetical spread, ♪ for the frequency index),
//     dog-eared bottom corners to turn back/forward, running headers naming
//     the open chapter, folios in the bottom margins.
//   • SUMMON — a real 3D opening (khoa: "make it more like a 3d book",
//     pointing at the side dock's fold — AoideJournal.qml lineage, whose
//     Y-rotation Qt renders with true perspective): the book is built as
//     TWO COVER HALVES hinged at the spine, each carrying its glass page,
//     board segment, fore-edge lines, content, and chrome. Shut, they sit
//     edge-on at ±88°; the summon swings both flat to 0° like a pop-up
//     book while the glow blooms and an opening shade lifts. The spine
//     inscription splits across the boards ("GRIMOIRE ·" | "βίβλος") so
//     each half swings with its cover; the ribbon rides the hinge axis and
//     holds still. Rest pose is dead flat, face-on — zero resting rotation.
//     The chapter leaf-turn is now two per-cover content rotations about
//     the SAME spine axis (equal angles about one shared axis compose into
//     exactly the old whole-spread turn).
//
// ── The ledger — `song/stage/grimoire.json` (CONTRACTS.md §4) ──────────────
// `GrimoireLedger.qml`, instantiated below as a plain child object (not a
// shell.qml singleton — nothing else needs launch-frequency data). Every
// successful launch calls `ledger.record`; page one — MOST SUMMONED — reads
// it back ranked. Cold start shows an HONEST sparse state (khoa's decision):
// real ranked entries first, a kaomoji filler line, never padded filler.
//
// ── Chapters ─────────────────────────────────────────────────────────────
// Page one is always the frequency index (an index, not a move); every other
// spread is the alphabet, `pageSize` entries at a time. The chapter hue tints
// that spread's running headers and its thumb tab.

import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge

    // ── Type voices (the Conductor family) ─────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // alpha on a role string
    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // Greek alphabetic numeral for tab n (0-based); falls back to arabic.
    readonly property string greekLetters: "αβγδεζηθικλμνξοπρστυφχψω"
    function greekNum(n) {
        return (n >= 0 && n < root.greekLetters.length)
               ? root.greekLetters.charAt(n) : ("" + (n + 1))
    }

    // ── The ledger (frequency chapter's data source) ────────────────────────
    GrimoireLedger { id: ledger }

    // ── Visibility gate + the book-opening summon ───────────────────────────
    // `shown` is the logical toggle; `fold` (0 shut .. 1 open) is the animated
    // state. The two cover halves read it: ±88° edge-on at 0, flat 0° at 1
    // (their Rotations live on coverL/coverR below). 300ms — a touch longer
    // than the journal's 230 so the perspective swing reads. The surface
    // stays visible until the close-fold eases out.
    property bool shown: false
    property real fold: 0
    Behavior on fold { NumberAnimation { duration: 300; easing.type: Easing.OutCubic } }

    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }

    onShownChanged: {
        // Stop any in-flight leaf-turn before a hard reset — otherwise the
        // running NumberAnimation keeps writing turn.angle on later frames
        // and fights the `turn.angle = 0` below.
        flipOut.stop()
        flipIn.stop()
        root.fold = root.shown ? 1 : 0
        if (root.shown) {
            // Clear the TextInput's OWN text, not root.query directly: the
            // TextInput binds `text: root.query` but also does
            // `onTextChanged: root.query = text`, so the first keystroke
            // breaks the query→text half of that binding. Assigning
            // root.query here would silently leave stale text on screen;
            // going through searchInput.text keeps the box and the state in
            // sync (the text→query half still fires forward).
            searchInput.text = ""
            root.selIndex = 0
            root.chapterIndex = 0
            turn.angle = 0
            root.flipping = false
            searchInput.forceActiveFocus()
        } else {
            searchInput.text = ""
        }
    }

    // ── Layer-shell surface (Overlay, full-screen, transparent) ────────────
    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown || root.fold > 0.01
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-launcher"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    // ── Trigger: Hyprland global shortcut (aoide:launcher) ─────────────────
    GlobalShortcut {
        appid: "aoide"
        name: "launcher"
        description: "Summon the Aoide app launcher"
        onPressed: root.toggle()
    }

    // ── Search + selection state ────────────────────────────────────────────
    property string query: ""
    property int selIndex: 0
    property int chapterIndex: 0

    // ── Apps + chapter taxonomy ──────────────────────────────────────────────
    readonly property var apps: {
        var m = DesktopEntries.applications
        return (m && m.values) ? m.values : []
    }
    readonly property var appsById: {
        var m = {}
        for (var i = 0; i < root.apps.length; i++) {
            var a = root.apps[i]
            if (a && !a.noDisplay) m[a.id] = a
        }
        return m
    }

    function byName(a, b) {
        var an = ("" + (a.name || "")).toLowerCase()
        var bn = ("" + (b.name || "")).toLowerCase()
        return an < bn ? -1 : (an > bn ? 1 : 0)
    }

    // How many entries a spread (both pages together) holds before turning —
    // sized to roughly fill both pages at the 38px manuscript-line height.
    readonly property int pageSize: 18

    // Page one (index 0) is ALWAYS the frequency chapter — a book index, never
    // omitted even when sparse (the honest-empty-state decision handles the
    // sparse read). Apps are NOT partitioned by category — everything else
    // flows alphabetically, a spread at a time, so an app appears on BOTH the
    // frequency index and its alphabetical page.
    readonly property var chapters: {
        var out = []

        var freqIds = ledger.rankedIds()
        var freqEntries = []
        for (var f = 0; f < freqIds.length; f++) {
            var fe = root.appsById[freqIds[f]]
            if (fe) freqEntries.push(fe)
        }
        out.push({ id: "frequency", title: "MOST SUMMONED", whisper: "συνήθεια",
                   hue: root.notes.paletteAccent, entries: freqEntries })

        var all = []
        for (var k in root.appsById) all.push(root.appsById[k])
        all.sort(root.byName)

        for (var p = 0; p < all.length; p += root.pageSize) {
            var slice = all.slice(p, p + root.pageSize)
            var pageNum = Math.floor(p / root.pageSize) + 1
            var firstCh = slice.length ? ("" + (slice[0].name || "?")).charAt(0).toUpperCase() : "?"
            var lastCh  = slice.length ? ("" + (slice[slice.length - 1].name || "?")).charAt(0).toUpperCase() : "?"
            out.push({ id: "page" + pageNum, title: firstCh + " – " + lastCh,
                       whisper: "σελίς " + pageNum, hue: root.notes.wireCyan, entries: slice })
        }
        return out
    }

    onChaptersChanged: {
        if (root.chapterIndex >= root.chapters.length)
            root.chapterIndex = Math.max(0, root.chapters.length - 1)
    }

    readonly property var currentChapter:
        (root.chapters.length > 0) ? root.chapters[Math.min(root.chapterIndex, root.chapters.length - 1)] : null

    readonly property color currentHue:
        (root.currentChapter && root.currentChapter.hue) ? root.currentChapter.hue
                                                           : root.withA(root.notes.paletteFg, 0.5)

    // ── Search — substring/prefix ranked, extended with keywords + frequency ─
    readonly property bool searching: ("" + root.query).trim().length > 0
    readonly property var searchResults: {
        var q = ("" + root.query).trim().toLowerCase()
        if (q.length === 0) return []
        var out = []
        for (var i = 0; i < root.apps.length; i++) {
            var e = root.apps[i]
            if (!e || e.noDisplay) continue
            var name = ("" + (e.name || "")).toLowerCase()
            var gen  = ("" + (e.genericName || "")).toLowerCase()
            var com  = ("" + (e.comment || "")).toLowerCase()
            var kws  = e.keywords || []
            var rank
            if (name.indexOf(q) === 0) {
                rank = 0
            } else if (name.indexOf(q) !== -1) {
                rank = 1
            } else if ((function () {
                for (var kI = 0; kI < kws.length; kI++)
                    if (("" + kws[kI]).toLowerCase().indexOf(q) !== -1) return true
                return false
            })()) {
                rank = 2
            } else if (gen.indexOf(q) !== -1 || com.indexOf(q) !== -1) {
                rank = 3
            } else {
                continue
            }
            out.push({ entry: e, rank: rank, name: name, count: ledger.count(e.id) })
        }
        out.sort(function (a, b) {
            if (a.rank !== b.rank) return a.rank - b.rank
            if (b.count !== a.count) return b.count - a.count
            return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0)
        })
        var res = []
        for (var j = 0; j < out.length; j++) res.push(out[j].entry)
        return res
    }

    readonly property var chapterEntries: (root.currentChapter) ? root.currentChapter.entries : []
    readonly property var currentList: root.searching ? root.searchResults : root.chapterEntries
    readonly property int splitAt: Math.ceil(root.currentList.length / 2)
    readonly property var leftEntries: root.currentList.slice(0, root.splitAt)
    readonly property var rightEntries: root.currentList.slice(root.splitAt)

    readonly property bool freqSparse:
        !root.searching && root.currentChapter && root.currentChapter.id === "frequency"
        && root.currentList.length < 4

    onQueryChanged: root.selIndex = 0
    onCurrentListChanged: {
        if (root.selIndex >= root.currentList.length)
            root.selIndex = Math.max(0, root.currentList.length - 1)
    }

    // ── Navigation helpers ──────────────────────────────────────────────────
    function move(delta) {
        var n = root.currentList.length
        if (n === 0) return
        var i = root.selIndex + delta
        if (i < 0) i = 0
        if (i > n - 1) i = n - 1
        root.selIndex = i
    }
    function launchSelected() {
        var list = root.currentList
        if (root.selIndex < 0 || root.selIndex >= list.length) return
        var e = list[root.selIndex]
        if (e && e.execute) {
            e.execute()
            ledger.record(e.id)
        }
        root.hide()
    }

    // ── Chapter flip — the two-phase leaf-turn (content only; the book
    // itself never moves) ───────────────────────────────────────────────────
    property bool flipping: false
    property int  flipDir: 1
    property int  pendingChapter: 0

    function flipToChapter(t) {
        var n = root.chapters.length
        if (n === 0) return
        t = Math.max(0, Math.min(n - 1, t))
        if (t === root.chapterIndex || root.flipping) return
        root.flipDir = t > root.chapterIndex ? 1 : -1
        root.pendingChapter = t
        root.flipping = true
        flipOut.start()
    }
    function nextChapter() { root.flipToChapter(root.chapterIndex + 1) }
    function prevChapter() { root.flipToChapter(root.chapterIndex - 1) }

    NumberAnimation {
        id: flipOut
        target: turn; property: "angle"
        to: root.flipDir * -82; duration: 120; easing.type: Easing.InQuad
        onFinished: {
            root.chapterIndex = root.pendingChapter
            root.selIndex = 0
            turn.angle = root.flipDir * 82
            flipIn.start()
        }
    }
    NumberAnimation {
        id: flipIn
        target: turn; property: "angle"
        to: 0; duration: 140; easing.type: Easing.OutQuad
        onFinished: root.flipping = false
    }

    // ══ Dim scrim — a click anywhere outside dismisses. Umber veil keyed off
    // the song's ink so the darken belongs to the palette. ═══════════════════
    MouseArea {
        anchors.fill: parent
        onClicked: root.hide()
        Rectangle {
            anchors.fill: parent
            color: root.withA(root.notes.paletteFg, 0.28)
        }
    }

    // ── One manuscript line — an entry ruled onto the page, no row box ──────
    // Hairline rule under each entry; the SELECTED line's rule ignites in
    // paletteHot and its margin dot becomes the sung ♪ — the one neon element.
    component ManuscriptRow: Item {
        id: row
        required property var modelData
        required property int index
        required property int leafOffset
        readonly property int globalIndex: leafOffset + index
        readonly property bool isSel: root.selIndex === row.globalIndex

        width: ListView.view ? ListView.view.width : 0
        height: 38

        // the ruling
        Rectangle {
            anchors.left: parent.left; anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: row.isSel ? 2 : 1
            color: row.isSel ? root.notes.paletteHot
                             : root.withA(root.notes.paletteFg, 0.13)
        }

        Row {
            anchors {
                left: parent.left
                right: parent.right
                verticalCenter: parent.verticalCenter
                verticalCenterOffset: -1
            }
            spacing: 8

            Text {   // margin marker
                anchors.verticalCenter: parent.verticalCenter
                width: 14
                horizontalAlignment: Text.AlignHCenter
                text: row.isSel ? "♪" : "·"
                font.family: root.faceMusic
                font.pixelSize: row.isSel ? 15 : 13
                color: row.isSel ? root.notes.paletteHot
                                 : root.withA(root.notes.paletteFg, 0.3)
            }
            Image {
                anchors.verticalCenter: parent.verticalCenter
                width: 20; height: 20
                sourceSize.width: 20; sourceSize.height: 20
                fillMode: Image.PreserveAspectFit
                source: (row.modelData && row.modelData.icon)
                        ? Quickshell.iconPath(row.modelData.icon, "application-x-executable")
                        : Quickshell.iconPath("application-x-executable")
            }
            Text {
                id: nameText
                anchors.verticalCenter: parent.verticalCenter
                text: (row.modelData && row.modelData.name) ? row.modelData.name : ""
                color: root.notes.paletteFg
                opacity: row.isSel ? 1.0 : 0.88
                font.family: root.faceSerif
                font.pixelSize: 14
                font.weight: row.isSel ? Font.Bold : Font.Medium
                elide: Text.ElideRight
                // leave room for the gloss note; cap so long names elide
                width: Math.min(implicitWidth, parent.width * 0.58)
            }
            Text {   // generic-name gloss, inline after the name
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                readonly property string sub:
                    (row.modelData && row.modelData.genericName) ? row.modelData.genericName
                    : ((row.modelData && row.modelData.comment) ? row.modelData.comment : "")
                visible: sub.length > 0
                text: sub
                color: root.withA(root.notes.holoBlue, 0.8)
                font.family: root.faceMono
                font.pixelSize: 10
                elide: Text.ElideRight
                width: Math.max(0, parent.width - nameText.width - 58)
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: root.selIndex = row.globalIndex
            onClicked: {
                root.selIndex = row.globalIndex
                root.launchSelected()
            }
        }
    }

    // ── One page of the open spread: running header, ruled lines, folio ─────
    component BookPage: Item {
        id: page
        required property var entries
        required property int leafOffset
        // isLeft gates the sparse filler; NOT the same test as leafOffset===0
        // (an empty list gives BOTH pages offset 0 — splitAt = ceil(0/2) = 0).
        required property bool isLeft

        width: book.pageW
        height: book.pageH

        // ── chapter head — a BOOK's heading, not a widget band: the chapter
        // title centred over a hairline rule broken by a small central knot.
        // The knot is a SHORT Greek-key (the one pantheon nod), presented the
        // way an illuminated book breaks its chapter rule with a fleuron —
        // integral to the page, hued to the chapter. ────────────────────────
        Text {
            id: chapterTitle
            anchors.top: parent.top; anchors.topMargin: 22
            anchors.horizontalCenter: parent.horizontalCenter
            text: page.isLeft
                  ? (root.searching ? "ζήτησις"
                        : (root.currentChapter ? root.currentChapter.title : ""))
                  : (root.searching
                        ? (root.currentList.length + " match" + (root.currentList.length === 1 ? "" : "es"))
                        : (root.currentChapter ? root.currentChapter.whisper : ""))
            font.family: root.faceSerif
            font.pixelSize: 13
            font.letterSpacing: 3
            color: root.currentHue
        }

        Item {
            id: headRule
            anchors.top: chapterTitle.bottom; anchors.topMargin: 7
            anchors.left: parent.left; anchors.leftMargin: 46
            anchors.right: parent.right; anchors.rightMargin: 46
            height: 12

            Canvas {
                id: knot
                anchors.centerIn: parent
                width: 52; height: 11
                property color hue: root.currentHue
                onHueChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = root.withA(hue, 0.9)
                    ctx.lineWidth = 1.2
                    var u = height / 5
                    var top = u * 0.5, bot = height - u * 0.5, period = u * 6
                    ctx.beginPath()
                    for (var x = 0; x < width; x += period) {
                        ctx.moveTo(x + u,     top)
                        ctx.lineTo(x + u,     bot - u)
                        ctx.lineTo(x + u * 4, bot - u)
                        ctx.lineTo(x + u * 4, top + u)
                        ctx.lineTo(x + u * 2, top + u)
                        ctx.lineTo(x + u * 2, bot - u * 2)
                        ctx.lineTo(x + u * 3, bot - u * 2)
                    }
                    ctx.stroke()
                }
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left; anchors.right: knot.left; anchors.rightMargin: 9
                height: 1
                color: root.withA(root.currentHue, 0.45)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: knot.right; anchors.leftMargin: 9; anchors.right: parent.right
                height: 1
                color: root.withA(root.currentHue, 0.45)
            }
        }

        ListView {
            id: lv
            anchors.top: parent.top
            anchors.topMargin: 62
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 36
            anchors.left: parent.left; anchors.leftMargin: 28
            anchors.right: parent.right; anchors.rightMargin: 28
            clip: true
            model: page.entries
            boundsBehavior: Flickable.StopAtBounds
            delegate: ManuscriptRow { leafOffset: page.leafOffset }

            // Keep the selected line in view — per-page, reacting to the
            // shared flat selIndex.
            Connections {
                target: root
                function onSelIndexChanged() {
                    var localIdx = root.selIndex - page.leafOffset
                    if (localIdx >= 0 && localIdx < page.entries.length)
                        lv.positionViewAtIndex(localIdx, ListView.Contain)
                }
            }
        }

        // faint manuscript rules — fill blank page space below the entries
        // at the same 38px pitch as a real ManuscriptRow (see `height: 38`
        // on that component), so an empty/partial page reads as ruled
        // parchment rather than blank cream. Same hairline treatment as the
        // real unselected row rule. Sibling of `lv`, placed before the sparse
        // filler so the filler text sits in front of the rules.
        Item {
            id: blankRules
            anchors.left: lv.left; anchors.right: lv.right
            y: lv.y + lv.contentHeight
            height: Math.max(0, (parent.height - 36) - y)
            visible: height > 0

            readonly property int rowPitch: 38
            readonly property int ruleCount: Math.floor(blankRules.height / blankRules.rowPitch)

            Repeater {
                model: blankRules.ruleCount
                Rectangle {
                    y: (index + 1) * blankRules.rowPitch - 1
                    width: blankRules.width
                    height: 1
                    color: root.withA(root.notes.paletteFg, 0.13)
                }
            }
        }

        // honest sparse filler — LEFT page only (few entries all land there).
        // A sibling of the ListView, not a footer: a footer counts toward
        // contentHeight and sizing off it loops.
        Item {
            anchors.left: lv.left; anchors.right: lv.right
            anchors.top: lv.top; anchors.topMargin: lv.contentHeight + 10
            anchors.bottom: lv.bottom
            visible: root.freqSparse && page.isLeft && height > 20
            Text {
                anchors.centerIn: parent
                width: parent.width - 16
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: "the grimoire is still\nlearning your habits\n(´･ω･`)"
                color: root.notes.paletteFg
                opacity: 0.5
                font.family: root.faceMono
                font.pixelSize: 12
            }
        }

        // folio, outer bottom margin: count on the left, position on the right
        Text {
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 13
            anchors.left: page.isLeft ? parent.left : undefined
            anchors.right: page.isLeft ? undefined : parent.right
            anchors.leftMargin: 28
            anchors.rightMargin: 28
            text: page.isLeft
                  ? (root.currentList.length + " " + (root.searching ? "result" : "app")
                     + (root.currentList.length === 1 ? "" : "s"))
                  : ((root.chapterIndex + 1) + " / " + root.chapters.length)
            font.family: root.faceMono
            font.pixelSize: 10
            color: root.withA(root.notes.paletteFg, 0.45)
        }
    }

    // ══ THE ASSEMBLY — incantation strip floating over the open book ════════
    Item {
        id: rig
        anchors.centerIn: parent
        width: book.tomeW
        height: 48 + 10 + book.height

        opacity: Math.min(1, root.fold * 1.4)

        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow clicks

        // ── the spell's emanation — sigils that POP OUT of the book ─────────
        // khoa likes this: a scatter of celestial/alchemical marks and clefs
        // that spring OUTWARD from the book's heart as the covers swing (each
        // lerps from the centre at fold 0 to just past the book's edge at fold
        // 1), so they read as magic escaping the opening tome — not floating
        // chrome. wireCyan / gold only; NEVER paletteHot (the selected line).
        Repeater {
            model: [
                { x: -70,  y: 60,  g: "✦", s: 22, c: 1, f: 0, a: 0.85 },
                { x: -46,  y: 130, g: "♪", s: 17, c: 0, f: 1, a: 0.7  },
                { x: -44,  y: 210, g: "⟡", s: 14, c: 0, f: 0, a: 0.6  },
                { x: -78,  y: 300, g: "❈", s: 18, c: 0, f: 0, a: 0.7  },
                { x: -50,  y: 380, g: "♫", s: 16, c: 1, f: 1, a: 0.68 },
                { x: -40,  y: 450, g: "✧", s: 12, c: 0, f: 0, a: 0.55 },
                { x: -72,  y: 540, g: "✶", s: 20, c: 1, f: 0, a: 0.78 },
                { x: 902,  y: 60,  g: "✦", s: 22, c: 1, f: 0, a: 0.85 },
                { x: 884,  y: 130, g: "♫", s: 17, c: 0, f: 1, a: 0.7  },
                { x: 884,  y: 210, g: "⟡", s: 14, c: 0, f: 0, a: 0.6  },
                { x: 922,  y: 300, g: "❈", s: 18, c: 0, f: 0, a: 0.7  },
                { x: 890,  y: 380, g: "♪", s: 16, c: 1, f: 1, a: 0.68 },
                { x: 886,  y: 450, g: "✧", s: 12, c: 0, f: 0, a: 0.55 },
                { x: 904,  y: 540, g: "✶", s: 20, c: 1, f: 0, a: 0.78 },
                { x: 388,  y: 600, g: "♪", s: 15, c: 1, f: 1, a: 0.72 },
                { x: 452,  y: 604, g: "✧", s: 11, c: 0, f: 0, a: 0.6  }
            ]
            Item {
                readonly property real cx: book.tomeW / 2
                readonly property real cy: 320
                x: cx + (modelData.x - cx) * root.fold
                y: cy + (modelData.y - cy) * root.fold
                opacity: root.fold * modelData.a
                Text {
                    text: modelData.g
                    font.family: modelData.f === 1 ? root.faceMusic : root.faceSerif
                    font.pixelSize: modelData.s
                    color: modelData.c === 1 ? root.withA(root.notes.paletteAccent, 0.95)
                                             : root.withA(root.notes.wireCyan, 0.9)
                }
            }
        }

        // ── glow — blooms behind both objects as the book opens ─────────────
        Rectangle {
            id: glowBook
            x: -book.boardPad - 6; y: book.y - 6
            width: book.tomeW + 2 * book.boardPad + 12
            height: book.height + 12
            visible: false
            color: "transparent"
            border.color: "white"
            border.width: 6
        }
        MultiEffect {
            source: glowBook
            anchors.fill: glowBook
            blurEnabled: true
            blur: 1.0
            blurMax: 48
            autoPaddingEnabled: true
            opacity: 0.55 * root.fold
        }
        Rectangle {
            id: glowStrip
            anchors.fill: incantation
            anchors.margins: -4
            visible: false
            color: "transparent"
            border.color: "white"
            border.width: 5
        }
        MultiEffect {
            source: glowStrip
            anchors.fill: glowStrip
            blurEnabled: true
            blur: 1.0
            blurMax: 40
            autoPaddingEnabled: true
            opacity: 0.5 * root.fold
        }

        // ── bookmark ribbon — ties the incantation strip to the book's spine.
        // Same accent-ribbon treatment as the one dangling at the tome's foot
        // (see book's `ribbon` Rectangle below): paletteAccent at 0.85 alpha.
        // The strip's horizontal center and the book's spine (gutter between
        // the covers) both sit at rig.width/2, so a single vertical bar
        // reaches both without going diagonal. Declared before `incantation`
        // and `book` so both paint over it — the strip hides its top end,
        // the covers swallow its tail, and it reads as tucked into the spine.
        Rectangle {
            id: stripRibbon
            width: 8
            x: (rig.width - width) / 2
            y: incantation.y + incantation.height
            height: Math.max(0, (book.y + 14) - y)
            color: root.withA(root.notes.paletteAccent, 0.85)
            opacity: root.fold * root.fold
        }

        // ── THE INCANTATION STRIP — the search bar, floating above the book ─
        // It does not fold with the covers; it drifts down into place as the
        // book opens beneath it.
        Rectangle {
            id: incantation
            x: (rig.width - 640) / 2
            y: -14 * (1 - root.fold)
            width: 640; height: 48
            radius: 0
            color: root.withA(root.notes.paletteBg, 0.72)
            border.color: root.notes.paletteFg
            border.width: 2

            Rectangle {   // gloss
                anchors.fill: parent; anchors.margins: 2
                gradient: Gradient {
                    GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.14) }
                    GradientStop { position: 0.48; color: Qt.rgba(1, 1, 1, 0.03) }
                    GradientStop { position: 0.52; color: Qt.rgba(1, 1, 1, 0.00) }
                    GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.03) }
                }
            }
            Rectangle {   // gold keyline
                anchors.fill: parent; anchors.margins: 4
                color: "transparent"
                border.color: root.withA(root.notes.paletteAccent, 0.8)
                border.width: 1
            }

            Text {
                id: prompt
                anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
                text: "♪"
                color: root.notes.paletteAccent
                font.family: root.faceMusic
                font.pixelSize: 17
                font.bold: true
            }
            Text {
                id: keyHint
                anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                text: "⌘ spc"
                font.family: root.faceMono
                font.pixelSize: 10
                color: root.withA(root.notes.paletteFg, 0.5)
            }

            TextInput {
                id: searchInput
                anchors {
                    left: prompt.right; leftMargin: 10
                    right: keyHint.left; rightMargin: 10
                    verticalCenter: parent.verticalCenter
                }
                focus: true
                color: root.notes.paletteFg
                font.family: root.faceMono
                font.pixelSize: 15
                selectionColor: root.notes.paletteAccent
                selectedTextColor: root.notes.paletteBg
                clip: true
                text: root.query
                onTextChanged: root.query = text

                Text {
                    anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                    text: "summon application"
                    color: root.notes.paletteFg
                    opacity: 0.4
                    font.family: root.faceMono
                    font.pixelSize: 15
                    visible: searchInput.text.length === 0
                }

                // Up/Down + Ctrl+J/K move selection; Enter launches; Escape
                // clears the query first, dismisses on the second press.
                // Ctrl+L/H always flip; plain Left/Right flip only when the
                // query is empty (they edit the cursor otherwise).
                Keys.onPressed: function (event) {
                    var ctrl = (event.modifiers & Qt.ControlModifier)
                    if (event.key === Qt.Key_Down
                            || (event.key === Qt.Key_J && ctrl)) {
                        root.move(1); event.accepted = true
                    } else if (event.key === Qt.Key_Up
                            || (event.key === Qt.Key_K && ctrl)) {
                        root.move(-1); event.accepted = true
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        root.launchSelected(); event.accepted = true
                    } else if (event.key === Qt.Key_Escape) {
                        if (root.query.length > 0) searchInput.text = ""
                        else root.hide()
                        event.accepted = true
                    } else if (ctrl && event.key === Qt.Key_L) {
                        root.nextChapter(); event.accepted = true
                    } else if (ctrl && event.key === Qt.Key_H) {
                        root.prevChapter(); event.accepted = true
                    } else if (event.key === Qt.Key_Right && !ctrl && root.query.length === 0) {
                        root.nextChapter(); event.accepted = true
                    } else if (event.key === Qt.Key_Left && !ctrl && root.query.length === 0) {
                        root.prevChapter(); event.accepted = true
                    }
                }
            }
        }

        // ══ THE BOOK — two cover halves hinged at the spine ═════════════════
        // The 3D open/close (journal lineage — AoideJournal.qml's fold, a
        // Y-rotation Qt renders with true perspective): each HALF of the book
        // — glass page, its board segment, fore-edge lines, content, chrome —
        // swings about the shared spine axis, ±88° edge-on when shut easing
        // to 0° flat. Everything book-shaped lives inside one of the two
        // halves so the whole object folds; only the ribbon (on the hinge
        // axis) and the incantation strip stand apart.
        Item {
            id: book
            x: 0
            y: 58

            readonly property int pageW: 430
            readonly property int pageH: 470
            readonly property int stackH: 22     // fore-edge thickness below the spread
            readonly property int boardPad: 12   // cover boards outgrow the pages
            readonly property int tomeW: 2 * pageW

            width: tomeW
            height: pageH + stackH + 14

            // ── shared building blocks ──────────────────────────────────────
            // The page is a designed leaf, not a blank pane: laid-paper texture,
            // a double illuminated border with corner rosettes, and a hairline
            // header/foot rule — all integral to the page (clipped to it), so
            // nothing reads as floating chrome.
            component GlassPage: Rectangle {
                id: gp
                width: book.pageW
                height: book.pageH
                radius: 0
                clip: true
                color: root.withA(root.notes.paletteBg, 0.82)
                border.color: root.notes.paletteFg
                border.width: 2

                Rectangle {   // gloss sheen
                    anchors.fill: parent; anchors.margins: 2
                    gradient: Gradient {
                        GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.13) }
                        GradientStop { position: 0.45; color: Qt.rgba(1, 1, 1, 0.03) }
                        GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
                        GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.03) }
                    }
                }

                // double illuminated border — a gold keyline with a finer
                // verdigris rule just inside it
                Rectangle {
                    anchors.fill: parent; anchors.margins: 12
                    color: "transparent"
                    border.color: root.withA(root.notes.paletteAccent, 0.75)
                    border.width: 1
                }
                Rectangle {
                    anchors.fill: parent; anchors.margins: 16
                    color: "transparent"
                    border.color: root.withA(root.notes.wireCyan, 0.35)
                    border.width: 1
                }

                // corner rosettes — a small ◆ knot centred exactly on each
                // corner of the gold keyline (margin 12), via a zero-size
                // anchor at the corner so alignment can't drift
                Repeater {
                    model: [ { hx: 0, vy: 0 }, { hx: 1, vy: 0 },
                             { hx: 0, vy: 1 }, { hx: 1, vy: 1 } ]
                    Item {
                        width: 0; height: 0
                        x: modelData.hx === 0 ? 12 : book.pageW - 12
                        y: modelData.vy === 0 ? 12 : book.pageH - 12
                        Text {
                            anchors.centerIn: parent
                            text: "◆"
                            font.family: root.faceMusic
                            font.pixelSize: 9
                            color: root.withA(root.notes.paletteAccent, 0.75)
                        }
                    }
                }
            }

            // the layered page block — the OUTER fore-edge popping out as a
            // ribbed stack of leaves, so the book carries real thickness even
            // lying flat (khoa: "layered to give the 3d effect — the side of
            // the pages should pop out"). Full page rectangles stepped only
            // OUTWARD (no vertical offset) so the frontmost glass page covers
            // all but a sliver of each — the exposed left/right edges stack
            // into a ribbed side face; back leaves darken to recede. Sits
            // behind the page inside the cover, so it swings in the 3D fold.
            component PageStack: Item {
                id: ps
                property int side: -1          // -1 left cover · +1 right cover
                readonly property int layers: 11
                readonly property real step: 2.4
                width: book.pageW
                height: book.pageH
                Repeater {
                    model: ps.layers
                    Rectangle {
                        readonly property int d: ps.layers - index   // backmost drawn first
                        x: ps.side < 0 ? -d * ps.step : d * ps.step
                        y: 0
                        width: book.pageW
                        height: book.pageH
                        radius: 0
                        // back leaves sit in shadow, front leaves near the page
                        color: root.withA(Qt.darker(root.notes.paletteBg, 1.0 + d * 0.03), 0.9)
                        border.color: root.withA(root.notes.paletteFg, 0.16 + (ps.layers - d) * 0.02)
                        border.width: 1
                    }
                }
            }

            // one board segment — each cover half carries its own, hinged
            // with it (real boards hinge at the spine)
            component BoardSegment: Rectangle {
                height: book.stackH + 10
                radius: 0
                color: root.withA(root.notes.paletteBg, 0.82)
                border.color: root.notes.paletteFg
                border.width: 2
                Rectangle {
                    anchors.fill: parent; anchors.margins: 3
                    color: "transparent"
                    border.color: root.withA(root.notes.paletteAccent, 0.5)
                    border.width: 1
                }
            }

            // wheel over the margins turns the page (page lists still scroll
            // themselves — this sits behind them)
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.NoButton
                onWheel: function (w) {
                    if (w.angleDelta.y < 0 || w.angleDelta.x > 0) root.nextChapter()
                    else if (w.angleDelta.y > 0 || w.angleDelta.x < 0) root.prevChapter()
                }
            }

            // ── LEFT COVER — hinged at its right edge (the spine) ───────────
            Item {
                id: coverL
                x: 0; y: 0
                width: book.pageW
                height: book.height
                transform: Rotation {
                    origin.x: coverL.width; origin.y: coverL.height / 2
                    axis { x: 0; y: 1; z: 0 }
                    angle: (1 - root.fold) * 88
                }

                PageStack { side: -1 }    // the left cover's outer fore-edge
                GlassPage { x: 0 }

                // gutter shading — the fold recedes
                Rectangle {
                    x: book.pageW - 60; y: 2
                    width: 60; height: book.pageH - 4
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: root.withA(root.notes.paletteFg, 0.0) }
                        GradientStop { position: 1.0; color: root.withA(root.notes.paletteFg, 0.2) }
                    }
                }
                Rectangle {   // crease hairline at the hinge
                    x: book.pageW - 2; y: 2
                    width: 1; height: book.pageH - 4
                    color: root.withA(root.notes.paletteFg, 0.35)
                }

                // this half's board, carrying its half of the inscription
                BoardSegment {
                    x: -book.boardPad
                    y: book.pageH + 2
                    width: book.pageW + book.boardPad
                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: "GRIMOIRE ·"
                        font.family: root.faceSerif
                        font.pixelSize: 11
                        font.letterSpacing: 4
                        color: root.withA(root.notes.paletteAccent, 0.95)
                    }
                }

                // page content — the chapter flip is a rotation about the
                // SAME spine axis, so the two halves' turns compose into the
                // old whole-spread leaf-turn exactly
                Item {
                    id: contentL
                    width: book.pageW
                    height: book.pageH
                    transform: Rotation {
                        origin.x: book.pageW; origin.y: book.pageH / 2
                        axis { x: 0; y: 1; z: 0 }
                        angle: turn.angle
                    }
                    BookPage { entries: root.leftEntries; leafOffset: 0; isLeft: true }

                    Rectangle {   // turning shadow, outer edge inward
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.4) }
                            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.0) }
                        }
                        opacity: 0.35 * Math.abs(turn.angle) / 82
                    }
                }

                // opening shade — the cover reads lit as it swings flat
                Rectangle {
                    x: -book.boardPad; y: 0
                    width: book.pageW + book.boardPad
                    height: book.height
                    color: root.withA(root.notes.paletteFg, 0.3)
                    opacity: (1 - root.fold) * 0.5
                }

                // dog-ear — turn back
                Canvas {
                    id: earPrev
                    x: 4; y: book.pageH - 32
                    width: 28; height: 28
                    visible: root.chapterIndex > 0
                    property color earC: root.notes.paletteAccent
                    onEarCChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset()
                        ctx.clearRect(0, 0, width, height)
                        ctx.fillStyle = root.withA(root.notes.paletteBg, 0.9)
                        ctx.strokeStyle = root.withA(earC, 0.85)
                        ctx.lineWidth = 1.4
                        ctx.beginPath()
                        ctx.moveTo(2, height - 2)
                        ctx.lineTo(width - 4, height - 2)
                        ctx.lineTo(2, 4)
                        ctx.closePath()
                        ctx.fill(); ctx.stroke()
                    }
                    MouseArea {
                        anchors.fill: parent; anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.prevChapter()
                    }
                }
            }

            // ── RIGHT COVER — hinged at its left edge (the spine) ───────────
            Item {
                id: coverR
                x: book.pageW; y: 0
                width: book.pageW
                height: book.height
                transform: Rotation {
                    origin.x: 0; origin.y: coverR.height / 2
                    axis { x: 0; y: 1; z: 0 }
                    angle: (1 - root.fold) * -88
                }

                PageStack { side: 1 }     // the right cover's outer fore-edge
                GlassPage { x: 0 }

                Rectangle {   // gutter shading
                    x: 0; y: 2
                    width: 60; height: book.pageH - 4
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: root.withA(root.notes.paletteFg, 0.2) }
                        GradientStop { position: 1.0; color: root.withA(root.notes.paletteFg, 0.0) }
                    }
                }
                Rectangle {   // crease hairline at the hinge
                    x: 1; y: 2
                    width: 1; height: book.pageH - 4
                    color: root.withA(root.notes.paletteFg, 0.35)
                }

                BoardSegment {
                    x: 0
                    y: book.pageH + 2
                    width: book.pageW + book.boardPad
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: "βίβλος"
                        font.family: root.faceSerif
                        font.pixelSize: 11
                        font.letterSpacing: 4
                        color: root.withA(root.notes.paletteAccent, 0.95)
                    }
                }

                Item {
                    id: contentR
                    width: book.pageW
                    height: book.pageH
                    transform: Rotation {
                        origin.x: 0; origin.y: book.pageH / 2
                        axis { x: 0; y: 1; z: 0 }
                        angle: turn.angle
                    }
                    BookPage { entries: root.rightEntries; leafOffset: root.splitAt; isLeft: false }

                    Rectangle {   // turning shadow, outer edge inward
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.0) }
                            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.4) }
                        }
                        opacity: 0.35 * Math.abs(turn.angle) / 82
                    }
                }

                // opening shade
                Rectangle {
                    x: 0; y: 0
                    width: book.pageW + book.boardPad
                    height: book.height
                    color: root.withA(root.notes.paletteFg, 0.3)
                    opacity: (1 - root.fold) * 0.5
                }

                // thumb-index tabs ride the right cover's fore-edge
                Column {
                    x: book.pageW - 2
                    y: 34
                    spacing: 4
                    Repeater {
                        model: root.chapters.length
                        Rectangle {
                            readonly property bool isCur: index === root.chapterIndex && !root.searching
                            width: isCur ? 30 : 24
                            height: Math.max(20, Math.min(34,
                                    Math.floor((book.pageH - 70) / Math.max(1, root.chapters.length)) - 4))
                            radius: 0
                            color: root.withA(root.notes.paletteBg, isCur ? 0.95 : 0.7)
                            border.width: 1
                            border.color: isCur ? root.notes.paletteAccent
                                                : root.withA(root.notes.paletteFg, 0.4)
                            Text {
                                anchors.centerIn: parent
                                text: index === 0 ? "♪" : root.greekNum(index - 1)
                                font.family: index === 0 ? root.faceMusic : root.faceSerif
                                font.pixelSize: 11
                                color: parent.isCur ? root.notes.paletteAccent
                                                    : root.withA(root.notes.paletteFg, 0.6)
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.flipToChapter(index)
                            }
                        }
                    }
                }

                // dog-ear — turn forward
                Canvas {
                    id: earNext
                    x: book.pageW - 32; y: book.pageH - 32
                    width: 28; height: 28
                    visible: root.chapterIndex < root.chapters.length - 1
                    property color earC: root.notes.paletteAccent
                    onEarCChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset()
                        ctx.clearRect(0, 0, width, height)
                        ctx.fillStyle = root.withA(root.notes.paletteBg, 0.9)
                        ctx.strokeStyle = root.withA(earC, 0.85)
                        ctx.lineWidth = 1.4
                        ctx.beginPath()
                        ctx.moveTo(width - 2, height - 2)
                        ctx.lineTo(4, height - 2)
                        ctx.lineTo(width - 2, 4)
                        ctx.closePath()
                        ctx.fill(); ctx.stroke()
                    }
                    MouseArea {
                        anchors.fill: parent; anchors.margins: -6
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.nextChapter()
                    }
                }
            }

            // the shared leaf-turn driver — both covers' content rotations
            // read this one angle (the flip animations target it by id)
            Item {
                id: turn
                property real angle: 0
            }

            // whole-spread empty state — a book-level overlay (drawn over both
            // covers; text spanning the spine can't live inside one cover or
            // the other cover's glass would overpaint half of it)
            Text {
                x: book.pageW - width / 2
                y: book.pageH / 2 - height / 2
                visible: root.currentList.length === 0 && !root.freqSparse
                text: root.searching
                      ? "no app by that name ♪(´ε｀ )"
                      : "no apps here ♪(´ε｀ )"
                color: root.notes.paletteFg
                opacity: 0.5 * root.fold
                font.family: root.faceMono
                font.pixelSize: 13
            }

            // the ribbon — ON the hinge axis, so it holds still while the
            // covers swing around it; fades in as the book settles flat
            Rectangle {
                x: book.pageW - 4
                y: book.pageH - 6
                width: 8
                height: book.stackH + 44
                color: root.withA(root.notes.paletteAccent, 0.85)
                opacity: root.fold * root.fold
                Rectangle {   // tip shadow line — the ribbon's cut end
                    anchors.bottom: parent.bottom
                    width: parent.width; height: 2
                    color: root.withA(root.notes.paletteFg, 0.4)
                }
            }
        }
    }
}
