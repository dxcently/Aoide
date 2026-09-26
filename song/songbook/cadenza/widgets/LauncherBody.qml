// LauncherBody.qml — everything the cadenza launcher draws (intent §3.5).
//
//     ┌─ RUN ─────────────────────────────────── apps │ clip │ ledger ┐
//     │ > fire_                                                  2/27 │
//     │ ───────────────────────────────────────────────────────────── │
//     │ [1] Firefox                 Web Browser                       │   the selected row: select band,
//     │ [2] qBittorrent             BitTorrent Client                 │   its [n] amber, its text match
//     │                                                               │
//     │ ───────────────────────────────────────────────────────────── │
//     │ ⏎ launch · tab mode · ↑↓ move · esc close                     │
//     └───────────────────────────────────────────────────────────────┘
//
// A HELPER (uppercase — never a slot). `launcher.qml` is the thin
// PanelWindow shell (namespace, layer, focus, toggle, shortcuts) and loads
// this body BY URL (design/kit.md §1); the preview canvas loads it directly,
// since the canvas refuses a PanelWindow root.
//
// ── MODES — the tabs cut into the top rule ───────────────────────────────
//   apps    every installed desktop entry (Quickshell's DesktopEntries),
//           alphabetical at rest, fuzzy-ranked under a query.
//   clip    clipboard history — the facet's `AoideClipboard` extra
//           (`parsedEntries`, `refresh()`, `copyById(id)`), text only: an
//           image entry shows its metadata line, never a thumbnail.
//   ledger  the facet's `GrimoireLedger` extra (`song/stage/grimoire.json`,
//           CONTRACTS §4): the most-launched apps, count and age.
// A mode whose extra is absent (the preview canvas passes both as null)
// says so on its first row; nothing is faked.
//
// ── THE WIRE — the same mechanism as sonata's launcher ───────────────────
//   launch  `systemd-run --user --scope --quiet --collect
//            [--working-directory=…] -- <entry.command…>` through
//           `Quickshell.execDetached` (scope-before-exec), then
//           `ledger.record(entry.id)`.
//   copy    `clipboard.copyById(id)` — a validated numeric cliphist id only;
//           clipboard contents never become a command.
// Clipboard previews and app names are Text.PlainText, never linkified,
// never used to pre-fill the prompt (house rule 4).
//
// ── KEYS ──────────────────────────────────────────────────────────────────
// Typing filters. ↑/↓, Ctrl+J/K move; PgUp/PgDn move a page; ⏎ fires the
// selected row; Tab / Shift+Tab (and Ctrl+L/H) cycle the mode; Esc clears
// the query first, then asks the shell to close (`dismissed`). Hover
// selects, click fires; a click on the scrim closes.
//
// ── COLOUR ────────────────────────────────────────────────────────────────
// One amber thing: the selected row's [n]. Indices at rest are dim; the
// query's hits in a name are `match` (white-hot); the text cursor is a
// static ink underline, so the index keeps amber to itself. Counts are
// `number`, glosses and ages `dim`, a failed clipboard read `urgent`.
import QtQuick
import Quickshell
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    property var bridge: null
    property var clipboard: null        // AoideClipboard (the slot extra)
    property var ledger: null           // GrimoireLedger (the slot extra)

    // The shell drives these; the canvas leaves the defaults (drawn open).
    property bool shown: true
    property string mode: "apps"        // "apps" | "clip" | "ledger"
    property string query: ""
    property int sel: 0
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

    // ── geometry, in cells ────────────────────────────────────────────────
    readonly property int innerCols: 68
    readonly property int listRows: 13
    readonly property int nameCells: 26
    readonly property int innerRows: 1 + 1 + listRows + 1 + 1   // prompt, rule, list, rule, keys

    implicitWidth: kit.cells(innerCols + 2) + kit.cells(12)
    implicitHeight: kit.lines(innerRows + 1.5) + kit.lines(6)

    readonly property var modes: ["apps", "clip", "ledger"]

    // ── shell API ─────────────────────────────────────────────────────────
    property Item _input: null
    function focusInput() { if (root._input) root._input.forceActiveFocus() }
    // Clean prompt in `m`; the shell calls this on every show.
    function reset(m, q) {
        root.setMode(m || "apps", true)
        if (root._input) root._input.text = q || ""
        else root.query = q || ""
        root.sel = 0
    }
    function setMode(m, force) {
        if (root.modes.indexOf(m) < 0) m = "apps"
        var changed = m !== root.mode
        root.mode = m
        root.sel = 0
        if (m === "clip" && (changed || force) && root.clipboard && !root.clipboard.loading)
            root.clipboard.refresh()
    }
    function cycleMode(step) {
        var i = root.modes.indexOf(root.mode)
        var n = root.modes.length
        root.setMode(root.modes[(i + step + n) % n], false)
    }
    function close() { root.dismissed() }

    // ── fuzzy ranking ─────────────────────────────────────────────────────
    // rank 0 prefix · 1 word-start substring · 2 substring · 3 subsequence
    // (the fuzzy tier) · 4 keyword · 5 generic name / comment. `hits` are
    // the matched character indices in `name` (tiers 0–3), for the highlight.
    function fuzzy(name, q, extra) {
        var n = name.toLowerCase()
        var at = n.indexOf(q)
        var hits = [], i
        if (at >= 0) {
            for (i = 0; i < q.length; i++) hits.push(at + i)
            var wordStart = at === 0 || " -_.".indexOf(n.charAt(at - 1)) >= 0
            return { rank: at === 0 ? 0 : (wordStart ? 1 : 2), hits: hits }
        }
        var j = 0
        for (i = 0; i < n.length && j < q.length; i++)
            if (n.charAt(i) === q.charAt(j)) { hits.push(i); j++ }
        if (j === q.length) return { rank: 3, hits: hits }
        if (extra) {
            var kws = extra.keywords || []
            for (i = 0; i < kws.length; i++)
                if (("" + kws[i]).toLowerCase().indexOf(q) >= 0) return { rank: 4, hits: [] }
            if (("" + (extra.genericName || "")).toLowerCase().indexOf(q) >= 0
                    || ("" + (extra.comment || "")).toLowerCase().indexOf(q) >= 0)
                return { rank: 5, hits: [] }
        }
        return null
    }

    // The name cut to the name column, as runs of hit / not-hit text.
    function segments(name, hits, cells) {
        var s = "" + name
        var cut = s.length > cells
        if (cut) s = s.slice(0, Math.max(0, cells - 1))
        var on = {}
        for (var i = 0; i < (hits || []).length; i++) on[hits[i]] = true
        var out = [], cur = null
        for (var c = 0; c < s.length; c++) {
            var h = !!on[c]
            if (!cur || cur.hit !== h) { cur = { t: "", hit: h }; out.push(cur) }
            cur.t += s.charAt(c)
        }
        if (cut) out.push({ t: "…", hit: false })
        return out
    }

    function glossOf(e) { return "" + (e.genericName || e.comment || "") }

    function ageOf(iso) {
        var t = Date.parse(iso || "")
        if (isNaN(t)) return ""
        var s = Math.max(0, (Date.now() - t) / 1000)
        if (s < 60) return Math.floor(s) + "s"
        if (s < 3600) return Math.floor(s / 60) + "m"
        if (s < 86400) return Math.floor(s / 3600) + "h"
        return Math.floor(s / 86400) + "d"
    }

    // ── data ──────────────────────────────────────────────────────────────
    readonly property string q: ("" + root.query).trim().toLowerCase()
    readonly property bool searching: root.q.length > 0

    readonly property var apps: {
        var m = DesktopEntries.applications
        var vs = (m && m.values) ? m.values : []
        var out = []
        for (var i = 0; i < vs.length; i++) if (vs[i] && !vs[i].noDisplay) out.push(vs[i])
        return out
    }
    readonly property var appsById: {
        var m = {}
        for (var i = 0; i < root.apps.length; i++) m[root.apps[i].id] = root.apps[i]
        return m
    }
    function countOf(id) { return root.ledger ? root.ledger.count(id) : 0 }

    readonly property var appsList: {
        var out = [], i, e
        if (!root.searching) {
            for (i = 0; i < root.apps.length; i++) {
                e = root.apps[i]
                out.push({ entry: e, name: "" + (e.name || e.id), hits: [] })
            }
            out.sort(function (a, b) {
                var an = a.name.toLowerCase(), bn = b.name.toLowerCase()
                return an < bn ? -1 : (an > bn ? 1 : 0)
            })
            return out
        }
        for (i = 0; i < root.apps.length; i++) {
            e = root.apps[i]
            var name = "" + (e.name || e.id)
            var f = root.fuzzy(name, root.q, e)
            if (f) out.push({ entry: e, name: name, hits: f.hits, rank: f.rank, count: root.countOf(e.id) })
        }
        out.sort(function (a, b) {
            if (a.rank !== b.rank) return a.rank - b.rank
            if (b.count !== a.count) return b.count - a.count
            var an = a.name.toLowerCase(), bn = b.name.toLowerCase()
            return an < bn ? -1 : (an > bn ? 1 : 0)
        })
        return out
    }

    readonly property var ledgerList: {
        if (!root.ledger) return []
        var ids = root.ledger.rankedIds()
        var launches = root.ledger.launches || {}
        var out = []
        for (var i = 0; i < ids.length; i++) {
            var e = root.appsById[ids[i]]
            if (!e) continue                     // uninstalled since: not launchable
            var name = "" + (e.name || e.id)
            var f = root.searching ? root.fuzzy(name, root.q, e) : { hits: [] }
            if (!f) continue
            var rec = launches[ids[i]] || {}
            out.push({ entry: e, name: name, hits: f.hits,
                       count: rec.count || 0, lastAt: rec.lastAt || "" })
        }
        return out
    }

    readonly property var clipList: {
        if (!root.clipboard) return []
        var all = root.clipboard.parsedEntries || []
        var out = []
        for (var i = 0; i < all.length; i++) {
            var it = all[i]
            if (!it) continue
            var p = "" + (it.preview || "")
            var at = root.searching ? p.toLowerCase().indexOf(root.q) : -1
            if (root.searching && at < 0) continue
            var hits = []
            for (var k = 0; at >= 0 && k < root.q.length; k++) hits.push(at + k)
            out.push({ id: "" + it.id, image: it.kind === "image", name: p, hits: hits })
        }
        return out
    }

    readonly property var totalCount: root.mode === "clip"
            ? (root.clipboard ? (root.clipboard.parsedEntries || []).length : 0)
            : root.mode === "ledger" ? (root.ledger ? root.ledger.rankedIds().length : 0)
            : root.apps.length
    readonly property var list: root.mode === "clip" ? root.clipList
                              : root.mode === "ledger" ? root.ledgerList
                              : root.appsList
    onListChanged: if (root.sel >= root.list.length) root.sel = Math.max(0, root.list.length - 1)
    onQueryChanged: root.sel = 0

    // What the list says when it has no rows (never a fake row).
    readonly property var emptyLine: {
        if (root.list.length > 0) return null
        var none = { t: "no match for “" + ("" + root.query).trim() + "”", c: root.kit.dim }
        if (root.mode === "clip") {
            if (!root.clipboard) return { t: "no clipboard — not wired here", c: root.kit.dim }
            if (root.clipboard.loading) return { t: "reading clipboard history…", c: root.kit.dim }
            if (root.clipboard.loadError)
                return { t: "clipboard history unavailable — " + (root.clipboard.error || "cliphist list failed"),
                         c: root.kit.urgent }
            return root.searching ? none : { t: "no clipboard history yet", c: root.kit.dim }
        }
        if (root.mode === "ledger") {
            if (!root.ledger) return { t: "no ledger — not wired here", c: root.kit.dim }
            return root.searching ? none
                 : { t: "no launches recorded yet — the ledger fills as you launch", c: root.kit.dim }
        }
        return root.searching ? none : { t: "no desktop entries found", c: root.kit.dim }
    }

    // ── actions ───────────────────────────────────────────────────────────
    function move(d) {
        var n = root.list.length
        if (n === 0) return
        root.sel = Math.max(0, Math.min(n - 1, root.sel + d))
    }
    function fire(i) {
        var it = root.list[i]
        if (!it) return
        root.sel = i
        if (root.mode === "clip") {
            if (root.clipboard && root.clipboard.copyById(it.id)) root.close()
            return
        }
        var e = it.entry
        if (e && e.command && e.command.length > 0) {
            var argv = ["systemd-run", "--user", "--scope", "--quiet", "--collect"]
            if (e.workingDirectory) argv.push("--working-directory=" + e.workingDirectory)
            argv.push("--")
            for (var k = 0; k < e.command.length; k++) argv.push(e.command[k])
            Quickshell.execDetached(argv)
            if (root.ledger) root.ledger.record(e.id)
        }
        root.close()
    }
    function handleKey(event) {
        var ctrl = (event.modifiers & Qt.ControlModifier)
        var k = event.key
        if (k === Qt.Key_Down || (ctrl && k === Qt.Key_J)) root.move(1)
        else if (k === Qt.Key_Up || (ctrl && k === Qt.Key_K)) root.move(-1)
        else if (k === Qt.Key_PageDown) root.move(root.listRows)
        else if (k === Qt.Key_PageUp) root.move(-root.listRows)
        else if (k === Qt.Key_Return || k === Qt.Key_Enter) root.fire(root.sel)
        else if (k === Qt.Key_Backtab || (ctrl && k === Qt.Key_H)) root.cycleMode(-1)
        else if (k === Qt.Key_Tab || (ctrl && k === Qt.Key_L)) root.cycleMode(1)
        else if (k === Qt.Key_Escape) {
            if (root._input && root._input.text.length > 0) root._input.text = ""
            else root.close()
        } else return
        event.accepted = true
    }

    // ── scrim: CRT black, no blur (intent §5: no glass) ──────────────────
    Rectangle {
        anchors.fill: parent
        color: root.kit.withA(root.kit.ground, 0.82)
        opacity: root.shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: root.shown ? 160 : 120 } }
    }
    MouseArea { anchors.fill: parent; onClicked: root.close() }

    // ── the pane; the mode tabs ride its stat slot ───────────────────────
    // The stat is the whole tab strip in `dim` with the current mode's word
    // blanked; the current word is laid over that gap in `match`, bold. The
    // pane cuts the rule around the stat itself, so no chrome is redrawn.
    readonly property string tabSep: " │ "
    function tabPrefix(m) {
        var s = ""
        for (var i = 0; i < root.modes.length && root.modes[i] !== m; i++) s += root.modes[i] + root.tabSep
        return s
    }
    readonly property string tabStat: {
        var parts = []
        for (var i = 0; i < root.modes.length; i++)
            parts.push(root.modes[i] === root.mode ? root.kit.rep(" ", root.modes[i].length) : root.modes[i])
        return parts.join(root.tabSep)
    }

    Use {
        id: paneUse
        kit: root.kit
        helper: "Pane"
        anchors.centerIn: parent
        props: ({
            title: "run",
            stat: Qt.binding(() => root.tabStat),
            statColor: Qt.binding(() => root.kit.dim),
            focused: true,
            animateOnCreate: true,
            open: Qt.binding(() => root.shown),
            cols: root.innerCols,
            rows: root.innerRows,
            content: launcherBody })
        MouseArea { anchors.fill: parent; z: -1 }   // clicks in the pane never reach the scrim

        // the tabs, over the stat's own cut
        Repeater {
            model: root.modes
            delegate: Text {
                id: tab
                required property string modelData
                readonly property bool current: modelData === root.mode
                readonly property real statX: paneUse.item ? paneUse.item._statX : 0
                x: statX + fm.advanceWidth(root.tabPrefix(modelData))
                y: 0
                z: 1                      // over the pane, which the Loader makes last
                visible: paneUse.item !== null && paneUse.item.fillA > 0
                text: modelData
                font: tab.current ? root.kit.titleFont : root.kit.font
                textFormat: Text.PlainText
                // the current word is drawn here; the others show the stat's
                // own dim text beneath and only take the click
                color: tab.current ? root.kit.match : "transparent"
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -2
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { root.setMode(tab.modelData, false); root.focusInput() }
                }
            }
        }
    }

    // A hairline across the content, on the middle of its line.
    component Hair: Item {
        width: root.kit.cells(root.innerCols)
        height: root.kit.cellH
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width; height: 1
            color: root.kit.dim
        }
    }

    Component {
        id: launcherBody
        Column {
            // ── the prompt ────────────────────────────────────────────────
            Item {
                width: root.kit.cells(root.innerCols)
                height: root.kit.cellH
                Text {
                    id: promptGlyph
                    text: "> "; font: root.kit.titleFont; textFormat: Text.PlainText
                    color: root.kit.title
                }
                Text {
                    id: countText
                    anchors.right: parent.right
                    font: root.kit.font; textFormat: Text.PlainText
                    color: root.kit.number
                    text: root.list.length + "/" + root.totalCount
                }
                TextInput {
                    id: input
                    x: promptGlyph.implicitWidth
                    width: parent.width - x - countText.implicitWidth - root.kit.cellW
                    height: root.kit.cellH
                    font: root.kit.font
                    color: root.kit.ink
                    selectionColor: root.kit.select
                    selectedTextColor: root.kit.match
                    selectByMouse: true
                    clip: true
                    // a static ink underline: no blink, never amber
                    cursorDelegate: Item {
                        width: root.kit.cellW; height: root.kit.cellH
                        Rectangle {
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                            width: parent.width; height: 2
                            color: root.kit.ink
                        }
                    }
                    onTextChanged: root.query = text
                    Keys.onPressed: function (event) { root.handleKey(event) }
                    Component.onCompleted: { root._input = input; if (root.query.length) text = root.query }
                    Component.onDestruction: if (root._input === input) root._input = null

                    Text {
                        visible: input.text.length === 0
                        text: root.mode === "clip" ? "filter clipboard history"
                            : root.mode === "ledger" ? "filter the most launched"
                            : "type to filter apps"
                        font: root.kit.font; textFormat: Text.PlainText
                        color: root.kit.dim
                    }
                }
            }
            Hair {}

            // ── the list ─────────────────────────────────────────────────
            Item {
                width: root.kit.cells(root.innerCols)
                height: root.kit.lines(root.listRows)

                Text {
                    visible: root.emptyLine !== null
                    text: root.emptyLine ? root.emptyLine.t : ""
                    color: root.emptyLine ? root.emptyLine.c : root.kit.dim
                    font: root.kit.font; textFormat: Text.PlainText
                    width: parent.width; elide: Text.ElideRight
                }

                ListView {
                    id: lv
                    anchors.fill: parent
                    clip: true
                    model: root.list
                    boundsBehavior: Flickable.StopAtBounds
                    Connections {
                        target: root
                        function onSelChanged() {
                            if (root.sel >= 0 && root.sel < lv.count) lv.positionViewAtIndex(root.sel, ListView.Contain)
                        }
                    }
                    delegate: Item {
                        id: row
                        required property var modelData
                        required property int index
                        readonly property bool lit: root.sel === row.index
                        readonly property int digits: String(root.list.length).length
                        width: lv.width
                        height: root.kit.cellH

                        Rectangle { anchors.fill: parent; color: root.kit.select; visible: row.lit }

                        Row {
                            // [n]
                            Text {
                                text: "["; font: root.kit.font; textFormat: Text.PlainText
                                color: row.lit ? root.kit.match : root.kit.dim
                            }
                            Text {
                                text: root.kit.padL(row.index + 1, row.digits)
                                font: row.lit ? root.kit.titleFont : root.kit.font
                                textFormat: Text.PlainText
                                color: row.lit ? root.kit.hot : root.kit.dim
                            }
                            Text {
                                text: "] "; font: root.kit.font; textFormat: Text.PlainText
                                color: row.lit ? root.kit.match : root.kit.dim
                            }
                            // the name (or the clipboard preview), hits white-hot
                            Item {
                                id: nameBox
                                readonly property int cells: root.mode === "clip"
                                        ? root.innerCols - row.digits - 3 - 8
                                        : root.nameCells
                                width: root.kit.cells(cells)
                                height: root.kit.cellH
                                clip: true
                                Row {
                                    Repeater {
                                        model: root.segments(row.modelData.name, row.modelData.hits, nameBox.cells)
                                        delegate: Text {
                                            required property var modelData
                                            text: modelData.t
                                            textFormat: Text.PlainText
                                            font: modelData.hit ? root.kit.titleFont : root.kit.font
                                            color: modelData.hit ? root.kit.match
                                                 : row.lit ? root.kit.bright
                                                 : (root.mode === "clip" && row.modelData.image) ? root.kit.mid
                                                 : root.kit.ink
                                        }
                                    }
                                }
                            }
                            // apps: the gloss · ledger: count + age · clip: the id
                            Text {
                                visible: root.mode === "apps"
                                width: root.kit.cells(root.innerCols - row.digits - 3 - root.nameCells)
                                text: row.modelData.entry ? " " + root.glossOf(row.modelData.entry) : ""
                                font: root.kit.font; textFormat: Text.PlainText
                                color: row.lit ? root.kit.mid : root.kit.dim
                                elide: Text.ElideRight
                            }
                            Text {
                                visible: root.mode === "ledger"
                                text: root.kit.padL("×" + (row.modelData.count || 0), 7)
                                font: root.kit.font; textFormat: Text.PlainText
                                color: root.kit.number
                            }
                            Text {
                                visible: root.mode === "ledger"
                                text: root.kit.padL(root.ageOf(row.modelData.lastAt), 6) + " ago"
                                font: root.kit.font; textFormat: Text.PlainText
                                color: row.lit ? root.kit.mid : root.kit.dim
                            }
                            Text {
                                visible: root.mode === "clip"
                                text: root.kit.padL(row.modelData.id || "", 8)
                                font: root.kit.font; textFormat: Text.PlainText
                                color: row.lit ? root.kit.mid : root.kit.dim
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            hoverEnabled: true
                            onEntered: root.sel = row.index
                            onClicked: root.fire(row.index)
                        }
                    }
                }
            }
            Hair {}

            // ── the keys ─────────────────────────────────────────────────
            Text {
                text: (root.mode === "clip" ? "⏎ copy" : "⏎ launch")
                      + " · tab mode · ↑↓ move · esc "
                      + (root.query.length ? "clear" : "close")
                font: root.kit.font; textFormat: Text.PlainText
                color: root.kit.dim
            }
        }
    }
}
