# modules/dendrites/dunst.nix — the dunst notification daemon.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - one enable toggle: aoide.dunst.enable (off by default)
#   - carries its own dependencies: the dunst package (dunstctl/dunstify ride
#     along for the CLI and the herald center's `dunstctl history` poll) and
#     the daemon unit, via Home Manager's services.dunst module
#   - reads no other module — with ONE deliberate exception: config.aoide.livery,
#     the house design-token seam. Popups are a surface, and every surface's
#     dress flows from livery (the stylix facet reads the same option for the
#     same reason). The notif tier is nullOr-with-palette-fallback
#     (modules/nucleus/options.nix), resolved here, once.
#
# Why dunst owns org.freedesktop.Notifications now: the Quickshell
# NotificationServer (AoideNotifications.qml) is retired; dunst draws the
# themed popups, and the herald lives on as a dock notification CENTER that
# polls `dunstctl history` (song slot `herald-center`). The surfaces registry
# records this as notifications.owner = "dunst" (modules/facets/quickshell/
# default.nix), which also keeps Stylix's dunst/mako targets stood down so the
# dunstrc below is the only theming applied.
#
# Theming lands the way every Stylix-target app's does: baked at rebuild (the
# user-gated gate), not live from stage/livery.json. If live popup recoloring
# is ever wanted, `aoide livery emit file` is the ready seam.
#
# Enable with one line in hosts/yomi-strix/default.nix:
#   aoide.dunst.enable = true;
#
# hosts/ knows dendrites; dendrites never know hosts.
{ config, lib, pkgs, ... }:

let
  lv = config.aoide.livery;

  # livery's hexColor tolerates a missing leading '#'; dunstrc requires it.
  hex = c: if lib.hasPrefix "#" c then c else "#" + c;

  # notif tier: nullOr + palette fallback (options.nix's notifType); palette
  # tier fields always carry a value (closed tier, non-null defaults).
  notifBg = hex (if lv.notif.bg != null then lv.notif.bg else lv.palette.bg);
  notifFg = hex (if lv.notif.fg != null then lv.notif.fg else lv.palette.fg);
  notifUrgent = hex (if lv.notif.urgent != null then lv.notif.urgent else lv.palette.urgent);
  accent = hex lv.palette.accent;
in
{
  options.aoide.dunst.enable =
    lib.mkEnableOption "dunst notification daemon (the herald's delivery backend)";

  config = lib.mkIf config.aoide.dunst.enable {
    environment.systemPackages = [ pkgs.dunst ];

    # HM's module owns the whole daemon lifecycle: dunstrc generation from
    # `settings`, a Type=dbus unit holding BusName=org.freedesktop.Notifications
    # bound to the graphical session, D-Bus activation file, and a dunstrc
    # reload trigger. Hand-rolling that (clipboard.nix style) would buy nothing.
    home-manager.users.${config.aoide.user}.services.dunst = {
      enable = true;
      settings = {
        global = {
          # House chrome is square and serif; 360px bottom-right is exactly
          # where/what the retired Quickshell popup surface drew.
          corner_radius = 0;
          width = 360;
          origin = "bottom-right";
          font = "Noto Serif 11";
          frame_color = accent;
          separator_color = "frame";
        };
        urgency_low = {
          background = notifBg;
          foreground = notifFg;
        };
        urgency_normal = {
          background = notifBg;
          foreground = notifFg;
        };
        urgency_critical = {
          background = notifBg;
          foreground = notifFg;
          frame_color = notifUrgent;
        };
      };
    };
  };
}
