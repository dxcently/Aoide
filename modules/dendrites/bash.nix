# modules/dendrites/bash.nix — the interactive bash environment.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.bash.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported from dxflake modules/dendrites/bash.nix):
#   - Enables programs.bash + completion for the aoide user (home-manager).
#   - profileExtra: auto-exec Hyprland on tty1 login (the bare-login → desktop
#     hand-off). Only fires when Hyprland is on PATH, so a headless box is safe.
#   - initExtra: a fastfetch greeting on every interactive shell.
#   - bashrcExtra: mcfly history init + the yazi `y` cd-wrapper (chdir to the
#     dir yazi last browsed on exit).
#   - lsd aliases (ls/ll/la/lt), power aliases (reboot/shutdown/sleep/…),
#     editor aliases (v/nv → nvim), lg → lazygit, crc → claude --rc.
#
# ALIASES — the Aoide `ad*` nh family (replaces dxflake's `dx*`), all pointed at
# the Aoide flake dir. `ad` alone cd's there.
#
# Omitted vs dxflake: the `khoa = "true"` sessionVariable (nothing depends on
# it). The other dendrites this one leans on (fastfetch, mcfly, yazi, nh) are
# their own files — enable them alongside bash for the full experience.
{ config, lib, ... }:
let
  flakeDir = "/home/khoa/Aoide";
in
{
  options.aoide.bash.enable = lib.mkEnableOption "the interactive bash environment (aliases, greeting, tty1 → Hyprland)";

  config = lib.mkIf config.aoide.bash.enable {
    home-manager.users.${config.aoide.user} =
      { pkgs, ... }:
      {
        home.packages = with pkgs; [ lsd ];

        programs.bash = {
          enable = true;
          enableCompletion = true;

          # tty1 login → launch the desktop (guarded so a headless login is safe).
          profileExtra = ''
            if [ "$(tty)" = "/dev/tty1" ] && command -v Hyprland >/dev/null; then
              exec Hyprland &> /dev/null
            fi
          '';

          # Greeting on every interactive shell.
          initExtra = ''
            fastfetch
          '';

          # mcfly history search + the yazi `y` cd-wrapper.
          bashrcExtra = ''
            command -v mcfly >/dev/null && eval "$(mcfly init bash)"

            function y() {
            local tmp="$(mktemp -t "yazi-cwd.XXXXXX")" cwd
            yazi "$@" --cwd-file="$tmp"
            if cwd="$(command cat -- "$tmp")" && [ -n "$cwd" ] && [ "$cwd" != "$PWD" ]; then
              builtin cd -- "$cwd"
            fi
            rm -f -- "$tmp"
               }
          '';

          shellAliases = {
            # editor
            v = "nvim";
            nv = "nvim";

            # Aoide `ad*` nh family (rehomed from dxflake's `dx*`).
            ad = "cd ${flakeDir}";
            adrebuild = "nh os switch ${flakeDir}/";
            adupdate = "nh os switch ${flakeDir}/ --update";
            # Re-lock just the first-party corner and switch. `adupdate` moves
            # every input including nixpkgs; this one moves only what we own,
            # so a muse bump never drags a world rebuild behind it.
            adbump = "nix flake update --flake ${flakeDir} aoide && nh os switch ${flakeDir}/";
            adboot = "nh os boot ${flakeDir}/";
            adtest = "nh os test ${flakeDir}/";
            adbuild = "nh os build ${flakeDir}/";
            adrollback = "nh os rollback";
            adcheck = "nix flake check ${flakeDir}/";
            adgens = "nh os info";
            adclean = "nh clean all";

            # navigation / power
            ".." = "cd ..";
            reboot = "systemctl reboot";
            shutdown = "systemctl poweroff";
            poweroff = "systemctl poweroff";
            sleep = "systemctl suspend";
            hibernate = "systemctl hibernate";
            lock = "hyprlock";

            # lsd
            ls = "lsd";
            ll = "lsd -l";
            la = "lsd -la";
            lt = "lsd --tree";

            # tools
            lg = "lazygit";
            crc = "claude --rc";
          };
        };
      };
  };
}
