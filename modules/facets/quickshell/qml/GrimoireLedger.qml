// GrimoireLedger.qml — the Grimoire's own usage ledger.
//
// A plain QtObject instantiated INSIDE AoideLauncher.qml (NOT a shell.qml
// singleton — nothing else needs to read launch-frequency data). Tracks how
// often each .desktop entry is launched so the Grimoire's frequency chapter
// ("most commonly opened") can rank real usage instead of guessing.
//
// ── The file (CONTRACTS.md §4) ──────────────────────────────────────────────
// `~/Aoide/song/stage/grimoire.json` — gitignored runtime, v0 schema:
//   { "schemaVersion": "0",
//     "launches": { "<desktop-entry-id>": { "count": N, "lastAt": "<iso8601>" } } }
// Written directly by this QML (`atomicWrites: true` on the FileView — a
// write-temp-then-rename, so a hot-reload or a crash mid-write never reads a
// torn file), mirroring the execute()-direct precedent AoideLauncher.qml's
// own header already flags: DesktopEntry.execute() is called straight from
// QML with no aoided verb in between, and this ledger follows the same
// no-new-verb idiom for its own side effect. Parsing follows LiveryState.qml's
// FileView idiom: a guarded try/catch degrades a missing/garbage file to an
// empty map rather than throwing.
//
// ── Pruning ──────────────────────────────────────────────────────────────
// Capped at the top 200 entries by count so a machine with years of uptime
// doesn't grow the file unbounded; pruning only ever drops the COLDEST tail,
// never the entries a chapter/search is about to render.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property string ledgerPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/grimoire.json"

    // { "<id>": { "count": N, "lastAt": "<iso8601>" } }
    property var launches: ({})

    readonly property int maxEntries: 200

    function count(id) {
        var e = root.launches[id]
        return e ? e.count : 0
    }

    // Ranked array of desktop-entry ids, count desc, lastAt desc tiebreak.
    function rankedIds() {
        var ids = Object.keys(root.launches)
        ids.sort(function (a, b) {
            var ea = root.launches[a], eb = root.launches[b]
            if (eb.count !== ea.count) return eb.count - ea.count
            var la = ea.lastAt || "", lb = eb.lastAt || ""
            return la < lb ? 1 : (la > lb ? -1 : 0)
        })
        return ids
    }

    // Increment + stamp, prune to maxEntries by count, persist.
    function record(entryId) {
        if (!entryId) return
        var m = root.launches
        var e = m[entryId] || { "count": 0, "lastAt": "" }
        e = { "count": e.count + 1, "lastAt": new Date().toISOString() }
        var next = {}
        for (var k in m) next[k] = m[k]
        next[entryId] = e

        var ids = Object.keys(next)
        if (ids.length > root.maxEntries) {
            ids.sort(function (a, b) {
                var ea = next[a], eb = next[b]
                if (eb.count !== ea.count) return eb.count - ea.count
                var la = ea.lastAt || "", lb = eb.lastAt || ""
                return la < lb ? 1 : (la > lb ? -1 : 0)
            })
            var kept = {}
            for (var i = 0; i < root.maxEntries; i++) kept[ids[i]] = next[ids[i]]
            next = kept
        }

        root.launches = next
        root.persist()
    }

    function persist() {
        ledgerFile.setText(JSON.stringify({
            "schemaVersion": "0",
            "launches": root.launches
        }, null, 2))
    }

    // printErrors: false — a fresh install has no grimoire.json yet (the
    // ledger is QML-created, not staged by aoided like the other stage
    // files), so the first-ever launch's "file not found" is an EXPECTED
    // cold-start condition, not a warning-worthy error. onTextChanged's
    // try/catch already degrades a missing/garbage read to an empty map.
    property FileView ledgerFile: FileView {
        id: ledgerFile
        path: root.ledgerPath
        atomicWrites: true
        watchChanges: true
        printErrors: false
        onFileChanged: ledgerFile.reload()
        onTextChanged: {
            try {
                var parsed = JSON.parse(ledgerFile.text())
                root.launches = (parsed && parsed.launches) ? parsed.launches : {}
            } catch (e) {
                root.launches = {}
            }
        }
        Component.onCompleted: ledgerFile.reload()
    }
}
