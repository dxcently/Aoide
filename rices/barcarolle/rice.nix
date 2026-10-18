# rices/barcarolle/rice.nix — Windows 7 Aero, rebuilt as an AoideOS rice.
#
# Glass superbar at the bottom: deep teal glass, white text, cyan glow.
# Workspace buttons as Hyprland workspaces, RICE cell, clock, and an orb.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.theme.
# All theme values are literal nix expressions (no song/ runtime reads).
{
  system = {
    providers = {
      bar = "strillix";
      launcher = "strillix";
      powermenu = "strillix";
      notifications = "strillix";
      wallpaper = "strillix";
      dock = "strillix";
      calendar = "strillix";
      switcher = "strillix";
      lock = "strillix";
      greeter = "strillix";
    };
    module = { pkgs, lib, ... }: {
      config = lib.mkDefault {
        # ── Expected paint ──────────────────────────────────────────────────────
        aoide.layout.surfaces = {
          bar.perMonitor = true;
          wallpaper.perMonitor = true;
        };

        # ── Palette tier ────────────────────────────────────────────────────────
        aoide.theme.palette = {
          bg = "#0c2130"; # deep glass, teal-dark
          fg = "#ffffff"; # white text on glass
          accent = "#77fffc"; # cyan glow, hot-track
          hot = "#3bdcac"; # working, Approve (bright teal)
          urgent = "#f47a72"; # Deny, needle tip (coral-red)
        };

        # ── Polarity ────────────────────────────────────────────────────────────
        aoide.theme.polarity = "dark";

        # ── Base16 tier — Aero teal + termui accents ───────────────────────────
        aoide.theme.base16 = {
          base00 = "#0c2130"; # deep glass (bg)
          base01 = "#1a3d4d"; # pane fill — slightly raised
          base02 = "#2a5a6d"; # selection / focused row
          base03 = "#6a8f9c"; # dim phosphor — rules at rest
          base04 = "#8db0ba"; # mid phosphor — secondary text
          base05 = "#ffffff"; # white text (fg)
          base06 = "#ffffff"; # bright white
          base07 = "#f5fbfb"; # white-hot — plates (notif.bg)
          base08 = "#f47a72"; # red — urgent
          base09 = "#f2a65e"; # orange — warning
          base0A = "#e8cf6a"; # yellow — awaiting
          base0B = "#3bdcac"; # teal — working/hot
          base0C = "#4fd8e6"; # cyan — information (orb flag)
          base0D = "#25bbcf"; # blue — accent
          base0E = "#a99cf6"; # magenta — person tile
          base0F = "#a68861"; # warm beige — contrast
        };

        # ── Component tier ──────────────────────────────────────────────────────
        aoide.theme.bar = {
          bg = "#0c2130"; # deep glass
          fg = "#ffffff"; # white text
          accent = "#77fffc"; # cyan glow
        };
        aoide.theme.notif = {
          bg = "#f5fbfb"; # white plates
          fg = "#0c2130"; # ink on plates
          urgent = "#f47a72";
        };
        aoide.theme.window = {
          border = "#2aa3b8"; # colorization: rims, frames
          borderInactive = "#1a4a5a"; # dimmer teal when not focused
        };

        # ── Geometry tier ───────────────────────────────────────────────────────
        # Win7 is rounded; barcarolle has user exception to radius-0 house style.
        aoide.theme.geometry = {
          gapsOut = 8;
          gapsIn = 4;
          borderSize = 1;
          rounding = 10;
          blurEnabled = true;
          blurSize = 4;
          blurPasses = 1;
          terminalOpacity = 0.95;
        };

        # ── Font tier ───────────────────────────────────────────────────────────
        # Segoe UI unavailable; fall back to Noto Sans (bar uses Noto Sans per intent).
        aoide.theme.fonts.monospace = {
          name = "Noto Sans Mono";
          package = pkgs.noto-fonts-cjk-sans;
        };

        # ── Cover art ───────────────────────────────────────────────────────────
        # The desktop picture (cover/barcarolle-desktop.jpg). Stylix bakes it,
        # the seed publishes it as the declared cover, and a staged rice finds
        # the same file in the song's own cover/ folder (CONTRACTS.md §4).
        aoide.theme.wallpaper = ./cover/barcarolle-desktop.jpg;
      };
    };
  };
  home = {
    members = [ "strillix" ];
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
