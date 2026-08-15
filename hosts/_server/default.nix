# hosts/_server/default.nix — TEMPLATE: headless server skeleton.
#
# Shelved (the `_` prefix): not registered in flake.nix. Aoide-the-core only —
# aoided, the session graph, conduct, hooks — with NO desktop: no facets, no
# hyprland, nothing renders a song here. Every terminal is still a conducted,
# tracked session; the reaper's liveness is pid-based, so no compositor is
# needed for dead sessions to be swept. To adopt:
#   1. cp -r hosts/_server hosts/<your-hostname>
#   2. flake.nix: `nixosConfigurations.<your-hostname> = mkHost "<your-hostname>";`
#   3. set hostName/aoide.user; networking is yours to declare (networkmanager
#      is a desktop pick — most servers want static/systemd-networkd instead)
#   4. nixos-rebuild switch --flake .#<your-hostname>
{ lib, ... }:
{
  imports = [
    ../common
  ]
  ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "server"; # ← your hostname

  time.timeZone = "UTC";

  aoide.enable = true;
  aoide.user = "khoa"; # ← your user

  # hosts/common turns the baseline dendrites ON; shelve the GUI-leaning one —
  # a headless box has no use for a terminal emulator:
  aoide.kitty.enable = false;

  # No aoide.facets.* / aoide.hyprland here — that is the AoideOS desktop and
  # this box performs no song. The core (graph/conduct/aoided) is what you get.

  # Headless is where the integrated agent + knowledge server earn their keep.
  # Both services are guarded (ConditionPathExists on their runtime binaries),
  # so enabling them is safe before the binaries land — the units just stay
  # inactive. Uncomment when wanted:
  # aoide.melete.enable = true;
  # aoide.mneme.enable = true;
}
