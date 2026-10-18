// StagingEngineHarness.qml — offscreen assertion harness for the owner-map
// manifest (W3b/C3), the *Preview.qml precedent (DockPreview.qml et al.)
// adapted for assertions instead of visual review.
//
// Unlike the *Preview.qml files (which load a real widget body to eyeball on
// screen), this loads nothing but a real `StagingEngine` and drives its
// public functions (`has`/`source`/`resolveRice`) against whatever
// rices/manifest.json sits under `$HOME/Aoide/run/qml/`. Run it against a
// SCRATCH `$HOME` pointed at a real BUILT `aoide-quickshell-config`
// derivation's manifest.json (never a hand-written fixture — the standing
// lesson this harness exists to honour) with an offscreen QPA platform:
//
//   QT_QPA_PLATFORM=offscreen HOME=<scratch> \
//     qs -p pkgs/strillix-shell/qml/StagingEngineHarness.qml
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
        console.log("loaded manifest rices:", JSON.stringify(Object.keys(m).sort()))

        // ── a slot absent everywhere resolves to "" ─────────────────────────
        harness.check("has() false for a slot no rice provides",
            harness.engine.has("nocturne", "nonexistent-slot"), false)
        harness.check("resolveRice() \"\" for a slot absent from the active rice AND the baseline",
            harness.engine.resolveRice("nocturne", "nonexistent-slot"), "")

        // ── a rice whose map is fully inherited resolves identically to
        // today's baseline floor ────────────────────────────────────────────
        // A sonata-only slot is one no OTHER rice OWNS — generalised (not
        // hardcoded to nocturne) so the next rice added to the rices cannot
        // silently hollow this assertion out. This checks OWNERSHIP, not mere
        // key presence: a record a rice borrows carries its lender's owner, so
        // a key-presence-only test would wrongly exclude it.
        var sonataOnlySlots = Object.keys(m.sonata).filter(function (slot) {
            return Object.keys(m).filter(function (s) { return s !== "sonata" })
                .every(function (s) { return !m[s][slot] || m[s][slot].owner !== s })
        })
        harness.check("sonata-only slot set is non-empty (test is exercising something)",
            sonataOnlySlots.length > 0, true)
        var inheritedOk = true
        for (var i = 0; i < sonataOnlySlots.length; i++) {
            var slot = sonataOnlySlots[i]
            var resolved = harness.engine.resolveRice("nocturne", slot)
            if (resolved !== "sonata") {
                inheritedOk = false
                console.log("  MISMATCH:", slot, "resolved to", JSON.stringify(resolved), "expected \"sonata\"")
            }
        }
        harness.check("nocturne (fully inherited for these slots) resolves every sonata-only slot to the baseline",
            inheritedOk, true)

        // ── every declared slot resolves, source() honouring owner ───────────
        // Ground truth read from the REAL BUILT manifest's own `owner` field
        // per record, not assumed to equal the rice name. resolveRice(rice, slot)
        // always returns the BORROWER (rice), but source(rice, slot) must
        // resolve under the OWNER's directory and file, per the manifest record.
        var rices = Object.keys(m)
        var slotCount = 0
        var allOk = true
        for (var s = 0; s < rices.length; s++) {
            var rice = rices[s]
            var slots = Object.keys(m[rice])
            for (var sl = 0; sl < slots.length; sl++) {
                slotCount++
                var sn = slots[sl]
                var rec = m[rice][sn]
                var resolvedRice = harness.engine.resolveRice(rice, sn)
                if (resolvedRice !== rice) {
                    allOk = false
                    console.log("  MISMATCH:", rice, sn, "resolveRice ->", JSON.stringify(resolvedRice), "expected", JSON.stringify(rice))
                }
                var src = harness.engine.source(rice, sn)
                var expectedSrc = Qt.resolvedUrl("rices/" + rec.owner + "/" + rec.file)
                if (src !== expectedSrc) {
                    allOk = false
                    console.log("  SRC MISMATCH:", rice, sn, "->", src, "expected", expectedSrc)
                }
            }
        }
        harness.check("real built manifest declares exactly 36 slots across 4 rices",
            slotCount, 36)
        harness.check("all 36 declared slots resolve to their own rice (resolveRice) and their record's owner (source)",
            allOk, true)

        // The cross-rice borrow is not exercised here: no committed rice borrows
        // a slot (the manifest above has every owner equal to its rice). The
        // borrow path is proved by tests/selection/rices-borrow and by the
        // consumer's merged host in tests/consumer, whose manifest names Aoide's
        // sonata as the owner of the borrowed slots.

        console.log("=== HARNESS RESULT:",
            harness.failures === 0 ? ("PASS (" + harness.total + " assertions)") : ("FAIL — " + harness.failures + "/" + harness.total + " assertions failed"),
            "===")
        Qt.quit()
    }
}
