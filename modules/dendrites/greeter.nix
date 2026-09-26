# modules/dendrites/greeter.nix — the display-manager greeter (ly).
#
# The login prompt: `ly` on tty1, authenticating through PAM and launching the
# Wayland session `programs.hyprland` registers (that half is the compositor
# lane's). Every colour — the animation's included — is an `aoide.livery` read,
# so a re-rice redresses the greeter with no edit here; ly takes colours as
# 32-bit `0xSSRRGGBB` (a styling byte ahead of the RGB: 00 plain, 01 bold) with
# `full_color` on by default, so the palette's own hex goes in unmapped — no
# eight-colour approximation and no second colour vocabulary. Box title and key
# hints stay at ly's defaults.
#
# It replaces greetd, which was wired as a stub for a Quickshell greeter that
# was never written — no `AoideGreeter.qml` ever existed. That stub put Hyprland
# in greetd's `default_session`, the GREETER slot rather than `initial_session`,
# so the desktop came up with no authentication step at all and logind classed
# the whole session `greeter`. ly restores the login prompt, and the `user`
# class follows from PAM registering a real login.
#
# Guarded on the FACT `aoide.greeter.enable` (declared once in
# modules/nucleus/options.nix), which the `nixos` half below sets
# `mkDefault true`. Reads only the dress (`aoide.livery`) plus that fact — root
# AGENTS.md house rule 5 — and a host that wants no greeter flips the fact off
# and boots to the tty it lands on.
let
  body =
    { config, lib, ... }:
    let
      # Read-side venue recolour (CONTRACTS.md §1, override tier): resolve
      # rewrites colours equal to an overridden anchor's authored value, in one
      # pass, with no option-system recursion — the option itself stays inert
      # either way.
      t = (import ../../lib/livery.nix { inherit lib; }).resolve config.aoide.livery;

      lyPlain = hex: "0x00${lib.removePrefix "#" hex}";
      lyBold = hex: "0x01${lib.removePrefix "#" hex}";
    in
    {
      config = lib.mkIf config.aoide.greeter.enable {
        services.displayManager.ly = {
          enable = true;
          settings = {
            # Behaviour lifted verbatim from dxflake's own ly dendrite
            # (modules/dendrites/displaymanager.nix on the reference rig) so
            # both rigs' greeters behave alike: the colour-mixing shader,
            # stopped after five minutes rather than painting an idle machine
            # forever, a locale clock, and a password field that clears on a
            # failed attempt.
            animation = "colormix";
            animation_timeout_sec = 300;
            clock = "%c";
            clear_password = true;

            # The dress is Aoide's, and every colour here is a palette read.
            bg = lyPlain t.palette.bg;
            fg = lyPlain t.palette.fg;
            border_fg = lyPlain t.palette.accent;
            # Bold, as ly's own default red is — an error stays emphatic.
            error_fg = lyBold t.palette.urgent;
            # The shader mixes the two chromatic anchors instead of ly's stock
            # red/blue. colormix_col3 keeps its default: its styling byte
            # (0x20) is undocumented upstream, and that is not a value to guess
            # at.
            colormix_col1 = lyPlain t.palette.accent;
            colormix_col2 = lyPlain t.palette.urgent;
          };
        };

        # Upstream gap, not a preference. nixpkgs' ly module builds
        # `systemd.services.display-manager` through the deprecated
        # `services.displayManager.generic` path, and neither that module nor
        # the shared display-manager module ever gives the unit a `wantedBy` —
        # so it is installed and never started, and the machine boots to a bare
        # tty1 with no greeter. greetd did not have the problem because its own
        # module carries `wantedBy = [ "graphical.target" ]` and aliases
        # display-manager.service onto it. Supply the missing link; drop this
        # the day the ly module carries it.
        systemd.services.display-manager.wantedBy = [ "graphical.target" ];
      };
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.greeter.enable = lib.mkDefault true;
    };
}
