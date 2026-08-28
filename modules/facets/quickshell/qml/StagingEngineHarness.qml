// StagingEngineHarness.qml — offscreen assertion harness for the owner-map
// manifest (W3b/C3), the *Preview.qml precedent (DockPreview.qml et al.)
// adapted for assertions instead of visual review.
//
// Unlike the *Preview.qml files (which load a real widget body to eyeball on
// screen), this loads nothing but a real `StagingEngine` and drives its
// public functions (`has`/`source`/`resolveSong`) against whatever
// songs/manifest.json sits under `$HOME/Aoide/run/qml/`. Run it against a
// SCRATCH `$HOME` pointed at a real BUILT `aoide-quickshell-config`
// derivation's manifest.json (never a hand-written fixture — the standing
// lesson this harness exists to honour) with an offscreen QPA platform:
//
//   QT_QPA_PLATFORM=offscreen HOME=<scratch> \
//     qs -p modules/facets/quickshell/qml/StagingEngineHarness.qml
//
// Prints one PASS/FAIL line per assertion plus a final HARNESS RESULT line,
// then exits. Never touches the live `$AOIDE_ROOT/run` tree or the live shell
// instance — a distinct `-p` config path is a distinct quickshell instance,
// same posture AOIDE-DEV.md's own `qs -p shell.qml` QML-load check already
// takes.
import QtQuick
import Quickshell

ShellRoot {
    id: harness

    property StagingEngine engine: StagingEngine {}
    property int failures: 0
    property int total: 0

    function check(desc, actual, expected) {
        harness.total++
        if (actual === expected) {
            console.log("PASS:", desc)
        } else {
            console.log("FAIL:", desc, "— expected", JSON.stringify(expected), "got", JSON.stringify(actual))
            harness.failures++
        }
    }

    // Gives the manifest FileView's async load a moment to land before any
    // assertion reads `engine.manifest` — mirrors SurfaceSlot.qml's own
    // documented observation that Component.onCompleted can fire before the
    // FileView's reload has landed.
    Timer {
        interval: 500
        running: true
        repeat: false
        onTriggered: harness._run()
    }

    function _run() {
        var m = harness.engine.manifest
        console.log("loaded manifest songs:", JSON.stringify(Object.keys(m).sort()))

        // ── a slot absent everywhere resolves to "" ─────────────────────────
        harness.check("has() false for a slot no song provides",
            harness.engine.has("etude", "nonexistent-slot"), false)
        harness.check("resolveSong() \"\" for a slot absent from the active song AND the baseline",
            harness.engine.resolveSong("etude", "nonexistent-slot"), "")

        // ── a song whose map is fully inherited resolves identically to
        // today's baseline floor ────────────────────────────────────────────
        // A sonata-only slot is one no OTHER song OWNS — generalised (not
        // hardcoded to etude/fugue/nocturne) so the next song added to the
        // songbook cannot silently hollow this assertion out. This checks
        // OWNERSHIP, not mere key presence: quodlibet's manifest carries a
        // key for 12 of these 14 slots too, but as a BORROW whose `owner` is
        // still "sonata" — a key-presence-only test (`!m[s][slot]`) would
        // wrongly exclude every one of those 12 the moment quodlibet exists,
        // collapsing the set to empty. `bar`/`herald` correctly drop out
        // because fugue genuinely OWNS them.
        var sonataOnlySlots = Object.keys(m.sonata).filter(function (slot) {
            return Object.keys(m).filter(function (s) { return s !== "sonata" })
                .every(function (s) { return !m[s][slot] || m[s][slot].owner !== s })
        })
        harness.check("sonata-only slot set is non-empty (test is exercising something)",
            sonataOnlySlots.length > 0, true)
        var inheritedOk = true
        for (var i = 0; i < sonataOnlySlots.length; i++) {
            var slot = sonataOnlySlots[i]
            var resolved = harness.engine.resolveSong("etude", slot)
            if (resolved !== "sonata") {
                inheritedOk = false
                console.log("  MISMATCH:", slot, "resolved to", JSON.stringify(resolved), "expected \"sonata\"")
            }
        }
        harness.check("etude (fully inherited for these slots) resolves every sonata-only slot to the baseline",
            inheritedOk, true)

        // ── all 32 slots across the 5 songs resolve, source() honouring owner ──
        // Ground truth read from the REAL BUILT manifest's own `owner` field
        // per record, not assumed to equal the song name — quodlibet exists
        // in this songbook now, and 2 of its 14 entries name fugue. This is
        // what makes the loop pass for a genuine borrow AND stay honest for
        // a self-owned slot: resolveSong(song, slot) always returns the
        // BORROWER (song), but source(song, slot) must resolve under the
        // OWNER's directory and file, per the manifest record itself.
        var songs = Object.keys(m)
        var slotCount = 0
        var allOk = true
        for (var s = 0; s < songs.length; s++) {
            var song = songs[s]
            var slots = Object.keys(m[song])
            for (var sl = 0; sl < slots.length; sl++) {
                slotCount++
                var sn = slots[sl]
                var rec = m[song][sn]
                var resolvedSong = harness.engine.resolveSong(song, sn)
                if (resolvedSong !== song) {
                    allOk = false
                    console.log("  MISMATCH:", song, sn, "resolveSong ->", JSON.stringify(resolvedSong), "expected", JSON.stringify(song))
                }
                var src = harness.engine.source(song, sn)
                var expectedSrc = Qt.resolvedUrl("songs/" + rec.owner + "/" + rec.file)
                if (src !== expectedSrc) {
                    allOk = false
                    console.log("  SRC MISMATCH:", song, sn, "->", src, "expected", expectedSrc)
                }
            }
        }
        harness.check("real built manifest declares exactly 32 slots across 5 songs",
            slotCount, 32)
        harness.check("all 32 declared slots resolve to their own song (resolveSong) and their record's owner (source)",
            allOk, true)

        // ── the real borrow (Rice B) ─────────────────────────────────────────
        // quodlibet owns none of its 14 slots — 12 come from sonata, 2
        // (bar, herald) from fugue. No synthesis needed: this is the first
        // real cross-song borrow in the committed songbook.
        var q = m.quodlibet
        var qSlots = Object.keys(q)
        harness.check("quodlibet declares 14 slots", qSlots.length, 14)
        harness.check("quodlibet owns none of them",
            qSlots.filter(function (s) { return q[s].owner === "quodlibet" }).length, 0)
        harness.check("12 borrowed from sonata",
            qSlots.filter(function (s) { return q[s].owner === "sonata" }).length, 12)
        harness.check("2 borrowed from fugue",
            qSlots.filter(function (s) { return q[s].owner === "fugue" }).length, 2)

        // has() true for the BORROWER's key — this is what separates a real
        // borrow from "quodlibet declares nothing and every lookup falls
        // through to sonata".
        harness.check("has() true on the borrower for a fugue-owned slot",
            harness.engine.has("quodlibet", "bar"), true)
        harness.check("resolveSong stays on the borrower, never the owner",
            harness.engine.resolveSong("quodlibet", "bar"), "quodlibet")

        // THE assertion. If quodlibet had no entry, resolveSong would answer
        // "sonata" and source would answer songs/sonata/bar.qml —
        // indistinguishable from a silent no-op. Only a real borrow produces
        // songs/fugue/bar.qml.
        harness.check("source() crosses to fugue for the fugue-owned bar",
            harness.engine.source("quodlibet", "bar"),
            Qt.resolvedUrl("songs/fugue/bar.qml"))
        harness.check("source() crosses to fugue for the fugue-owned herald",
            harness.engine.source("quodlibet", "herald"),
            Qt.resolvedUrl("songs/fugue/herald.qml"))
        harness.check("source() crosses to sonata for a sonata-owned slot",
            harness.engine.source("quodlibet", "calendar"),
            Qt.resolvedUrl("songs/sonata/calendar.qml"))

        console.log("=== HARNESS RESULT:",
            harness.failures === 0 ? ("PASS (" + harness.total + " assertions)") : ("FAIL — " + harness.failures + "/" + harness.total + " assertions failed"),
            "===")
        Qt.quit()
    }
}
