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
// then exits. Never touches the live `~/Aoide/run` tree or the live shell
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
        // etude/fugue/nocturne author none of sonata's sonata-only slots, so
        // every one of those slots must fall through to sonata for them,
        // exactly like today's baseline-fallback chain.
        var sonataOnlySlots = Object.keys(m.sonata).filter(function (slot) {
            return !m.etude[slot] && !m.fugue[slot] && !m.nocturne[slot]
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

        // ── all 18 slots across the 4 songs resolve as they do today ────────
        // Ground truth from the REAL BUILT manifest itself: every
        // (song, slot) pair it declares is self-provided (no borrow exists
        // yet), so resolveSong(song, slot) must return that song, and
        // source(song, slot) must point at that song's own copied file —
        // exactly today's pre-W3b behavior, just reached through the new
        // owner-map lookup instead of the old list scan.
        var songs = Object.keys(m)
        var slotCount = 0
        var allOk = true
        for (var s = 0; s < songs.length; s++) {
            var song = songs[s]
            var slots = Object.keys(m[song])
            for (var sl = 0; sl < slots.length; sl++) {
                slotCount++
                var sn = slots[sl]
                var resolvedSong = harness.engine.resolveSong(song, sn)
                if (resolvedSong !== song) {
                    allOk = false
                    console.log("  MISMATCH:", song, sn, "resolveSong ->", JSON.stringify(resolvedSong), "expected", JSON.stringify(song))
                }
                var src = harness.engine.source(song, sn)
                var expectedSrc = Qt.resolvedUrl("songs/" + song + "/" + sn + ".qml")
                if (src !== expectedSrc) {
                    allOk = false
                    console.log("  SRC MISMATCH:", song, sn, "->", src, "expected", expectedSrc)
                }
            }
        }
        harness.check("real built manifest declares exactly 18 slots across 4 songs",
            slotCount, 18)
        harness.check("all 18 declared slots resolve to their own song, source() pointing at their own file",
            allOk, true)

        // ── a borrowed slot resolves to its owner ───────────────────────────
        // No real borrow exists yet in the committed songbook (every owner
        // == its own song), so this synthesizes one on the LOADED manifest
        // to prove the owner-map is actually consulted for the lookup and
        // not just "song name doubles as owner" by coincidence: etude's
        // manifest gets a slot whose OWNER is sonata.
        var borrowed = JSON.parse(JSON.stringify(m))
        borrowed.etude.borrowedSlot = { owner: "sonata", file: "bar.qml" }
        harness.engine.manifest = borrowed

        harness.check("borrowed slot: has() true for the BORROWING song's key",
            harness.engine.has("etude", "borrowedSlot"), true)
        harness.check("borrowed slot: resolveSong() returns the borrowing song's key (etude), not the owner",
            harness.engine.resolveSong("etude", "borrowedSlot"), "etude")
        harness.check("borrowed slot: source() resolves through the OWNER's directory (sonata/bar.qml), not etude's",
            harness.engine.source("etude", "borrowedSlot"),
            Qt.resolvedUrl("songs/sonata/bar.qml"))

        harness.engine.manifest = m // restore the real manifest

        console.log("=== HARNESS RESULT:",
            harness.failures === 0 ? ("PASS (" + harness.total + " assertions)") : ("FAIL — " + harness.failures + "/" + harness.total + " assertions failed"),
            "===")
        Qt.quit()
    }
}
