# modules/dendrites/networkmanager.nix — the NetworkManager applet package.
#
# Dendrite shape v0: guarded on aoide.networkmanager.enable, carries its own
# dependency, nothing more. Ships pkgs.networkmanagerapplet — which provides
# BOTH nm-connection-editor (the GUI network editor, the useful surface here)
# and nm-applet (the tray indicator). Note: the Quickshell shell has no system
# tray today, so nm-applet itself has nowhere to dock — this dendrite is for
# the editor; if a tray ever lands, autostarting nm-applet is a separate add.
#
# NetworkManager itself stays venue plumbing: the host owns
# `networking.networkmanager.enable` (yomi-strix and the _desktop/_laptop
# skeletons already set it). This dendrite only assumes it.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.networkmanager.enable = lib.mkEnableOption "the NetworkManager applet package (nm-connection-editor + nm-applet)";

  config = lib.mkIf config.aoide.networkmanager.enable {
    environment.systemPackages = [ pkgs.networkmanagerapplet ];
  };
}
