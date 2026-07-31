# modules/dendrites/audio.nix — PipeWire + WirePlumber (the audio backend).
#
# Dendrite shape v0 (CONTRACTS.md §2): guarded on aoide.audio.enable, carries
# its own dependencies, reads no other module (only aoide.user, implicitly
# via the `audio` group already granted in hosts/common/default.nix).
#
# Backs the bar's REAL volume control: AoideBar.qml already binds to
# Quickshell.Services.Pipewire (Pipewire.defaultAudioSink.audio.volume/
# .muted) — nothing was actually running the audio graph until this dendrite
# is enabled on a host. No hardware.pulseaudio to disable anywhere in this
# flake — it was never enabled.
{ config, lib, ... }:
{
  options.aoide.audio.enable = lib.mkEnableOption "PipeWire + WirePlumber audio backend";

  config = lib.mkIf config.aoide.audio.enable {
    security.rtkit.enable = true;
    services.pipewire = {
      enable = true;
      alsa.enable = true;
      alsa.support32Bit = true;
      pulse.enable = true;
      wireplumber.enable = true;
    };
  };
}
