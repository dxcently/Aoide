# modules/dendrites/aoide-packages.nix — the aoide CLI on every PATH.
#
# The control plane's binaries (`aoide`, `aoided` — one derivation as of P-A7
# of the binary-split workstream) are injected into pkgs by mkHost's overlay;
# this module puts them on the SYSTEM profile so agent sessions and the user at
# a terminal can invoke them by name. `strillix` is a separate output and the
# strillix dendrite installs it, so this module reads no strillix fact. The
# systemd units don't need this (they ExecStart absolute store paths) — the
# interactive session does.
#
# Found live on first switch: the vm-boot test node added these to
# systemPackages itself, masking their absence from the `nucleus` aggregation. The test node
# now relies on this module instead (tests/vm-boot.nix keeps only jq).
{ pkgs, ... }:
{
  config = {
    aoide.enable = true;
    environment.systemPackages = [
      pkgs.aoide
      # git is load-bearing, not dev comfort: nix flake operations on the
      # user's Aoide clone require it (found live on first switch — dxflake's
      # nucleus had carried it, and the "essentials only" port cut it).
      pkgs.git
    ];
  };
}
