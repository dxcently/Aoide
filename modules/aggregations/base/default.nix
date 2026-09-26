# base — the command line every Aoide host carries.
#
# An aggregation body is DATA: the members it selects, the provider choices it
# exposes, and any preference that rides along into the platform pass. It
# declares no options and carries no gate of its own; `lib/composition.nix`
# wraps it in one. Members are named by CATALOGUE NAME.
#
# `base` is a membership, not a brand: these are the capabilities every host
# gets, including the shelved skeletons, which is why it is the one aggregation
# every host selects. `melete` and `mneme` are deliberately NOT members — a host
# that wants one names it, and no host has to switch anything off.
{
  description = "The command line every Aoide host carries: shell, prompt, editor, fuzzy history, file and process browsers.";

  system = {
    members = [
      "bash"
      "btop"
      "cli"
      "devtools"
      "fastfetch"
      "fonts"
      "git"
      "kitty"
      "mcfly"
      "neovim"
      "nh"
      "starship"
      "yazi"
    ];

    # What `base` sets beyond its members, deferred until the platform pass.
    # The framework switch and the sane stateVersion used to live in
    # `hosts/common`; they are the same for every host, so they are the floor's
    # business and no host file restates them.
    nixos =
      { lib, ... }:
      {
        aoide.enable = lib.mkDefault true;
        aoide.mcp.enable = lib.mkDefault false;
        system.stateVersion = lib.mkDefault "25.11";
      };
  };
}
