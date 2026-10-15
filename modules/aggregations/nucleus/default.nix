# nucleus — what every Aoide host carries beside the contract.
#
# An aggregation body is DATA: the members it selects, the provider choices it
# exposes, and any preference that rides along into the platform pass. It
# declares no options and carries no gate of its own; habit's composition
# wraps it in one. Members are named by CATALOGUE NAME.
#
# `nucleus` is a membership, not a brand: the daemon and its units, the nix
# settings, the binaries on the PATH and the command line,
# including the shelved skeletons, which is why it is the one aggregation every
# host selects. A host can still drop a member
# (`habit.dendrites.kitty.enable = false`). What no host can drop is
# the option contract in `modules/nucleus`, which is not a member: it is the
# module every host imports as an extra module. `melete` and `mneme` are
# deliberately NOT members — a host that wants one names it, and no host has to
# switch anything off.
{
  description = "What every Aoide host carries: the daemon and its units, nix settings, the aoide binaries, and the command line — shell, prompt, editor, fuzzy history, file and process browsers.";

  system = {
    members = [
      "aoide-nix"
      "aoide-packages"
      "aoided"
      "bash"
      "btop"
      "cli"
      "devtools"
      "fastfetch"
      "fonts"
      "git"
      "kitty"
      "mcfly"
      "melete-adapter"
      "neovim"
      "nh"
      "starship"
      "yazi"
    ];

    # What `nucleus` sets beyond its members, deferred until the platform pass.
    # The framework switch and the sane stateVersion used to live in
    # `hosts/common`; they are the same for every host, so they are the floor's
    # business and no host file restates them.
    module =
      { lib, ... }:
      {
        system.stateVersion = lib.mkDefault "25.11";
      };
  };
}
