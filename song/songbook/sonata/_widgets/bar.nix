# sonata's `bar` slot — the top-edge widget, hosted by shell.qml's PanelWindow.
#
# `kind` is absent → null: `bar` is a facet-anchored slot (shell.qml embeds
# `WidgetSlot { slot: "bar" }` directly, slots.md's own wired-slot table),
# never a declared `arrangement.widgets` entry.
#
# `packages`: widgets/bar.qml shells out to `pactl` (`Quickshell.execDetached`,
# the card-profile switch) and `pavucontrol` (the mixer launch). Both are
# already carried by `modules/facets/quickshell/default.nix`'s
# `environment.systemPackages` today (`pkgs.pavucontrol`, `pkgs.pulseaudio #
# for pactl only`) — this record names the same two packages so a later
# consumer can derive the install list from the widget instead of a
# hand-synced comment.
#
# `dependsOn`: bar.qml embeds `WidgetSlot { slot: "calendar" }` itself
# (its clock popout), and declares `required property var powermenu` /
# `required property var dock`, both of which it calls `.toggle()` on
# (the clef and the agent-sessions cell).
#
# `helpers`: the uppercase types bar.qml actually instantiates —
# `AudioColonnade`, `BarPopout`, `SteleLayerPopout`, `StelePopout`.
# `GadgetFrame` is mentioned only in comments explaining bar does NOT use it
# (StelePopout hosts bare); it is not a real dependency.
_: {
  file = "bar.qml";

  packages = [
    "pavucontrol"
    "pulseaudio"
  ];

  dependsOn = [
    "calendar"
    "powermenu"
    "dock"
  ];

  helpers = [
    "AudioColonnade.qml"
    "BarPopout.qml"
    "SteleLayerPopout.qml"
    "StelePopout.qml"
  ];
}
