# modules/dendrites/audio.nix — PipeWire + WirePlumber (the audio backend).
#
# Dendrite shape (CONTRACTS.md §2): guarded on aoide.audio.enable, carries
# its own dependencies, reads no other module (only aoide.user, implicitly
# via the `audio` group already granted in the account (users/khoa.nix).
#
# Backs the bar's REAL volume control: AoideBar.qml already binds to
# Quickshell.Services.Pipewire (Pipewire.defaultAudioSink.audio.volume/
# .muted) — nothing was actually running the audio graph until this dendrite
# is enabled on a host. No hardware.pulseaudio to disable anywhere in this
# flake — it was never enabled.

{ config, lib, ... }:
{
  options.aoide.audio.enable = lib.mkEnableOption "PipeWire + WirePlumber audio backend";

  config = lib.mkMerge [
    { aoide.audio.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.audio.enable {
      security.rtkit.enable = true;
      services.pipewire = {
        enable = true;
        alsa.enable = true;
        alsa.support32Bit = true;
        pulse.enable = true;
        wireplumber.enable = true;
      };
      # Bluetooth audio is audio hardware, so it is enabled here beside PipeWire
      # rather than in the widget lane: bluez is what makes WirePlumber create
      # the bluez5 nodes the bar's colonnade reads (its BT bay renders a "no
      # adapter" state until this lands). The hardware is present and unblocked
      # on yomi-strix (`rfkill` shows hci0, neither soft- nor hard-blocked);
      # nothing was ever running the stack.
      hardware.bluetooth.enable = true;
    })
  ];
}
