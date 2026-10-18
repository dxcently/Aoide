# sonata's `bar` slot — the top-edge widget, hosted by shell.qml's PanelWindow.
#
# `kind` is absent → null: `bar` is a shell-anchored slot (shell.qml embeds
# `WidgetSlot { slot: "bar" }` directly, slots.md's own wired-slot table),
# never a declared `layout.widgets` entry.
#
# `packages`: widgets/bar.qml shells out to `pactl` (`Quickshell.execDetached`,
# the card-profile switch) and `pavucontrol` (the mixer launch). Both are
# already carried by the strillix lane's own
# `environment.systemPackages` (`modules/dendrites/strillix/default.nix`, `pkgs.pavucontrol`, `pkgs.pulseaudio #
# for pactl only`) — this record names the same two packages so a later
# consumer can derive the install list from the widget instead of a
# hand-synced comment.
#
# `dependsOn`: `ricemode` ONLY. bar.qml embeds `WidgetSlot { slot: "ricemode" }`
# (the rice-mode control leading its right stave). The clef (powermenu), the
# agent-sessions cell (dock) and the clock (calendar) are not closure members:
# bar.qml asks the bridge for each (`bridge.component(...)`), and the bridge runs
# whichever component fills the slot, so nothing here is a QML dependency.
#
# `helpers`: the uppercase types bar.qml actually instantiates —
# `AudioColonnade`, `BarPopout`, `StelePopout`. `GadgetFrame` is mentioned only
# in comments explaining bar does NOT use it (StelePopout hosts bare); it is not
# a real dependency.
_: {
  file = "bar.qml";

  packages = [
    "pavucontrol"
    "pulseaudio"
  ];

  dependsOn = [
    "ricemode"
  ];

  helpers = [
    "AudioColonnade.qml"
    "BarPopout.qml"
    "StelePopout.qml"
  ];
}
