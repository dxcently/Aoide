# fugue's `bar` slot — the always-visible top-edge lattice, hosted by
# shell.qml's PanelWindow. `kind` is absent → null: `bar` is a shell-anchored
# slot (`WidgetSlot { slot: "bar" }`, slots.md's own wired-slot table), never
# a declared `arrangement.widgets` entry.
#
# `dependsOn`: `ricemode` ONLY, despite two more real runtime dependencies —
# read this before adding either.
#
# widgets/bar.qml embeds `WidgetSlot { slot: "ricemode" }` (the rice-mode
# control, first cell of the right row). fugue authors that slot itself
# (`ricemode.nix`), so it is a key of this composition and `composeSong`'s
# closure holds. At runtime the slot still resolves through `StagingEngine`,
# which is how quodlibet's borrowed bar reaches fugue's body.
#
# `powermenu` is the dependency the closure cannot hold. widgets/bar.qml
# declares `required property var powermenu` and calls
# `.toggle()` on it (bar.qml:228) — shell.qml:139 injects
# `powermenuSlot.item`, the live item of the real `SurfaceSlot { slot:
# "powermenu" }` anchor. By the rule this shelf's sibling songs use (a real
# wired anchor is a genuine sibling-slot address, an injected shell
# component is not), `powermenu` WOULD be a `dependsOn` entry — except
# `lib/song.nix`'s `composeSong` closure checks a dependency against
# `present = builtins.attrNames widgets`, i.e. the KEYS OF THIS SAME
# COMPOSITION CALL, not the full songbook. fugue's own `rice.nix` composes
# only this shelf's records `{ bar, herald, ricemode }` — fugue does not, and
# should not, own a `powermenu` record of its own — so `present` never
# contains `powermenu` and the closure throws unconditionally:
# `aoide composition: slot dependency not satisfied — \`bar\` (owner fugue)
# needs powermenu` (reproduced against this exact file via `composeSong`,
# in isolation, with no songbook.nix or quodlibet involved).
#
# sonata's `bar.nix` never hits this because sonata OWNS a `powermenu`
# record itself (`_widgets/powermenu.nix`), so `powermenu` is always a key
# in sonata's own composition. fugue has no such record and getting one
# would mean fugue hand-declaring a stub for a slot whose real body lives in
# another song and is reached only through `StagingEngine`'s baseline
# fallback at runtime (`resolveSong`) — a mechanism `composeSong`'s
# eval-time closure has no visibility into and was never designed to
# express. Widening that contract is a `lib/song.nix` design change, out of
# scope for a prerequisite commit. Left out here rather than invented
# around; the runtime dependency is real and stays true — it is
# `powermenuSlot.item`'s shell-level fallback that already makes it safe,
# same as it does today before this shelf existed.
#
# It ALSO declares `required property var dock` and calls `.toggle()` on it
# (bar.qml:213) — but shell.qml:140 injects `dock: aoidePanel`, the shell's
# own `AoidePanel` component, not a `SurfaceSlot { slot: "dock" }` anchor
# (there is none — sonata's own `dock.nix` concedes the same thing). Calling
# `.toggle()` on an injected shell object is not addressing a sibling slot
# either way, so `dock` would not belong here regardless of the closure
# issue above — do not copy it in for symmetry with sonata's `bar.nix`,
# which lists it in error (see that file's audit note in the commit that
# added this shelf).
#
# `shared`, `stagingEngine`: declared, unused (bar.qml's own comments say so).
#
# The one `WidgetSlot` it embeds is `ricemode` — fugue's bar has no popout of
# its own, so no `calendar` dependency either.
#
# `helpers`: bar.qml instantiates `Cell` (fugue's own uppercase helper, this
# song's `qmldir` proof — `Cell.qml`).
_: {
  file = "bar.qml";

  dependsOn = [ "ricemode" ];

  helpers = [ "Cell.qml" ];
}
