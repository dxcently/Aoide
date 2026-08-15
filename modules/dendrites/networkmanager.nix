# modules/dendrites/networkmanager.nix — the NetworkManager applet package.
#
# Dendrite shape v0: guarded on aoide.networkmanager.enable, carries its own
# dependency. Ships pkgs.networkmanagerapplet — which provides BOTH
# nm-connection-editor (the GUI network editor) and nm-applet (the tray
# indicator, a StatusNotifierItem client). The Quickshell shell now hosts a
# real SNI tray (song/songbook/sonata/widgets/bar.qml's fermata-toggle
# BarPopout, Quickshell.Services.SystemTray) — nm-applet has somewhere to
# dock, so it autostarts as a systemd user service (same graphical-session
# shape clipboard.nix's watcher uses) rather than sitting unused.
# nm-connection-editor stays launch-on-demand only (no reason to keep a GUI
# editor window resident).
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

    systemd.user.services.aoide-nm-applet = {
      description = "NetworkManager tray applet (docks in the Aoide bar's SNI tray)";
      wantedBy = [ "graphical-session.target" ];
      after = [ "graphical-session.target" ];
      partOf = [ "graphical-session.target" ];
      unitConfig.ConditionEnvironment = "WAYLAND_DISPLAY";

      serviceConfig = {
        # No CLI flags: this nixpkgs build links libayatana-appindicator
        # (confirmed via its wrapper's GI_TYPELIB_PATH), so it registers as a
        # StatusNotifierItem automatically — the shape Quickshell's
        # SystemTray service reads. nm-applet does not parse a --help/
        # --indicator flag; it is not meant for CLI interaction at all.
        ExecStart = "${pkgs.networkmanagerapplet}/bin/nm-applet";
        Restart = "on-failure";
        RestartSec = "3s";
        NoNewPrivileges = true;
      };
    };
  };
}
