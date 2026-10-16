# modules/dendrites/git.nix — git + gh for the aoide user.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Selected by the nucleus aggregation (modules/aggregations/nucleus/default.nix), or one line in hosts/.
#
# What this dendrite does (ported from dxflake modules/dendrites/git.nix):
#   - programs.git with LFS, a default branch of `main`, an identity, and
#     safe.directory entries for the system config + the Aoide checkout.
#   - programs.gh with the git credential helper wired for github + gists.
#
# Adapted vs dxflake: the safe.directory entry /home/khoa/dxflake is rehomed to
# /home/khoa/Aoide. Identity kept as the dxflake author for continuity — retune
# per box if desired.

{ lib, ... }:
{

  config = {
    habit.home = _: {
      programs = {
        git = {
          enable = true;
          lfs.enable = true;
          signing.format = null;
          settings = {
            user.name = "dxcently";
            user.email = "dxcently@gmail.com";
            init.defaultBranch = "main";
            core.editor = "nvim";
            safe.directory = [
              "/etc/nixos"
              "/home/khoa/Aoide"
            ];
          };
        };
        gh = {
          enable = true;
          gitCredentialHelper = {
            enable = true;
            hosts = [
              "https://github.com"
              "https://gist.github.com"
            ];
          };
        };
      };
    };
  };
}
