# modules/dendrites/qbittorrent.nix — qBittorrent desktop client.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#
# What this dendrite does:
#   - Installs the qBittorrent desktop app (no headless service, no web UI,
#     no firewall port — the app's own settings own downloads and ports).
#   - Registers it as the handler for magnet: links and .torrent files.

{
  lib,
  pkgs,
  ...
}:
{

  config = {
    environment.systemPackages = [ pkgs.qbittorrent ];

    xdg.mime.defaultApplications = {
      "x-scheme-handler/magnet" = "org.qbittorrent.qBittorrent.desktop";
      "application/x-bittorrent" = "org.qbittorrent.qBittorrent.desktop";
    };
  };
}
