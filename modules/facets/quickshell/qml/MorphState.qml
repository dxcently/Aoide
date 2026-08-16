// MorphState.qml — the compact↔expanded MORPH, extracted as one shared object.
//
// A widget that has two sizes (calendar.qml's month↔year scroll, AudioColonnade's
// porch↔parthenon) needs the same four things every time: a latched `expanded`
// flag, a 0→1 `frac` animated on the house's morph tier, `lerp()` so every
// geometry constant can be written as a pair, and — the part that is easy to get
// wrong — an ORDER around the host surface's size step.
//
// This object owns all four. It does NOT own the window: the two live callers sit
// on different surface types and size themselves differently (calendar.qml assigns
// its own winW/winH on a wlr layer surface; AudioColonnade is auto-sized by the
// xdg_popup its StelePopout host wraps around it), so the size step is a HOOK —
// `stepOut()` before the paint grows, `stepIn()` after it has shrunk — and each
// host wires its own mechanism into it. One state machine, two window mechanisms.
//
// ── WHY THE STEP HOOKS EXIST (measured 2026-08-16, yomi-strix, Hyprland 0.56.0)
//
// NOT for a compositor remap. calendar.qml's 300ms `resizeGuard` was written
// against a recorded behaviour — "Hyprland re-maps an xdg_popup on the first
// resize of a visible popup and plays its popup animation over the remap (~0.25s
// vanish/fade)". That behaviour DOES NOT REPRODUCE on this rig any more. Both
// surfaces were driven live and burst-captured with `grim` at ~60ms/frame:
//
//   · LAYER SURFACE (calendar on SteleLayerPopout): across a full expand and a
//     full collapse the surface never left `hyprctl layers`, its x/y stayed at
//     31,26 through a 298→621 width step, and every captured frame was fully
//     painted. Removing the guard outright produced a clean morph — and a
//     ~300ms faster one, since the guard's whole cost was a dead interval where
//     the window had already grown and the sheet was still compact.
//   · XDG_POPUP (AudioColonnade on StelePopout): a probe resized the live popup
//     8 times over 120 frames (~7.2s). ZERO frames showed the popup absent —
//     the frame means clustered on exactly two values, the two widths, with one
//     transitional frame per resize where the content and the surface were a
//     frame out of step. The first resize after a fresh map was captured
//     separately and behaved identically. No fade, no vanish, no remap.
//
// So the guard is dead weight on BOTH surface types and this object does not
// carry one. What the capture DID show is why the step is still ordered:
// animating the surface size per frame (the same probe bound to a 340ms
// NumberAnimation) leaves the surface a frame behind the paint for the whole
// morph, so the trailing edge — border, cast shadow — shimmers in and out of
// clip for 340ms instead of for one frame. Stepping the surface to the larger
// size ONCE, before the paint grows, and back down only after the paint has
// finished shrinking, keeps the surface ≥ the content at every instant. That is
// the same "motion is paint, not geometry" discipline calendar.qml's header
// already states; this object is just where the ordering now lives.
//
// ── USE
//
//   MorphState {
//       id: mode
//       onStepOut: { root.winW = root.expandedWinW; root.winH = root.expandedWinH }
//       onStepIn:  { root.winW = root.compactWinW;  root.winH = root.compactWinH  }
//   }
//   readonly property int sheetW: mode.lerpInt(compactSheetW, expandedSheetW)
//   ...
//   MouseArea { onClicked: mode.toggle() }
//
// `busy` is the re-entrancy guard every caller needs anyway — a second toggle
// mid-flight is absorbed, and any motion that must not run across the morph
// (calendar.qml's page slide) tests it instead of naming an animation id.
//
// Reachable from a song widget: it is a facet-owned TOP-LEVEL component, so a
// widget under run/qml/songs/<song>/ reaches it with `import "../.."` — the same
// import bar.qml already uses for StelePopout/AudioColonnade. A helper in the
// song's OWN directory would not resolve (widget-structure.md §8).
//
// QtObject has no default property, so the animation is a NAMED property
// (hazards.md §3) — the LiveryState/SurfaceSlot idiom.

import QtQuick

QtObject {
    id: root

    // 0 compact … 1 expanded, animated. Every geometry constant in a caller is
    // written as a pair and read through lerp/lerpInt off this.
    property real frac: 0
    // Latched immediately on toggle, so text that names the DESTINATION
    // ("μήν ⌃" vs "ἔτος ⌄") flips at the press, not at the end of the motion.
    property bool expanded: false

    // House morph tier (widget-structure.md §9: morph/summon 300–340ms).
    property int duration: 340
    property int easingType: Easing.InOutCubic

    // Grow the host surface — emitted BEFORE the paint starts expanding.
    signal stepOut()
    // Shrink it — emitted AFTER the paint has finished collapsing.
    signal stepIn()

    readonly property bool busy: _anim.running

    property NumberAnimation _anim: NumberAnimation {
        target: root
        property: "frac"
        duration: root.duration
        easing.type: root.easingType
        onStopped: if (!root.expanded && root.frac === 0) root.stepIn()
    }

    function setExpanded(v) {
        if (_anim.running || v === root.expanded) return
        root.expanded = v
        _anim.to = v ? 1 : 0
        if (v) root.stepOut()          // surface out first…
        _anim.restart()                // …then the paint follows it
    }                                  // (…and stepIn lands in onStopped)

    function toggle() { root.setExpanded(!root.expanded) }

    function lerp(a, b) { return a + (b - a) * root.frac }
    function lerpInt(a, b) { return Math.round(a + (b - a) * root.frac) }
}
