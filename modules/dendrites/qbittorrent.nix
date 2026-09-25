# modules/dendrites/qbittorrent.nix — qBittorrent desktop client.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.qbittorrent.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#
# What this dendrite does:
#   - Installs the qBittorrent desktop app (no headless service, no web UI,
#     no firewall port — the app's own settings own downloads and ports).
#   - Registers it as the handler for magnet: links and .torrent files.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.qbittorrent.enable = lib.mkEnableOption "the qBittorrent desktop client";

  config = lib.mkIf config.aoide.qbittorrent.enable {
    environment.systemPackages = [ pkgs.qbittorrent ];

    xdg.mime.defaultApplications = {
      "x-scheme-handler/magnet" = "org.qbittorrent.qBittorrent.desktop";
      "application/x-bittorrent" = "org.qbittorrent.qBittorrent.desktop";
    };
  };
}
