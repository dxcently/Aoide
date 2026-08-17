# modules/dendrites/vision.nix — AGENT screen capture primitives (grim/slurp).
#
# Dendrite shape v0 (CONTRACTS.md §2): guarded on aoide.vision.enable,
# carries its own dependencies, reads no other module.
#
# "Vision" for agents working the rice: grim captures a Wayland screenshot
# non-interactively — no freeze overlay, no notification, no picker — which
# is what an agent needs to LOOK at what it just changed:
#
#   grim shot.png                    # full screen
#   grim -g "0,0 1920x28" bar.png    # exact region (geometry from
#                                    #   hyprctl layers / hyprctl clients)
#
# slurp rides along as the region-select primitive for semi-attended flows
# (a human drags a region, the agent consumes the geometry):
#
#   grim -g "$(slurp)" pick.png
#
# The HUMAN capture path is a separate dendrite (screenshot.nix — hyprshot +
# satty + binds). hyprshot wraps grim internally, but the two callers ship as
# two modules so either can be enabled alone.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.vision.enable = lib.mkEnableOption "agent screen-capture primitives (grim + slurp on PATH)";

  config = lib.mkIf config.aoide.vision.enable {
    environment.systemPackages = with pkgs; [
      grim # non-interactive capture: full screen or exact geometry
      slurp # region-select primitive (emits geometry on stdout)
      # Pointer synthesis for `aoide screen point` (screen phase 2): real
      # wl_pointer.motion/button/axis events via zwlr_virtual_pointer_manager_v1,
      # never a cursor warp (hyprctl dispatch movecursor warps — no hover
      # states fire, an agent could never verify one). The ONLY tool
      # `screen/point.rs`'s `run_wlrctl_pointer` shells out to.
      wlrctl
      # OCR engine for `aoide screen ocr` (screen phase 3), eng-only
      # trained data: `pkgs.tesseract` unwrapped pulls EVERY language's
      # tessdata (nixpkgs' own `languages.all`, ~1GB unpacked, measured
      # 2026-08-16); `enableLanguages` is the wrapper's own override point
      # (pkgs/applications/graphics/tesseract/wrapper.nix, confirmed against
      # this flake's pinned nixpkgs rev 279b4a8275f032c566576b3f181fa0f27197f588)
      # for cutting that down to one language's data — the exact pattern
      # nixpkgs itself already uses for its nixos-test-driver
      # (`tesseract4.override { enableLanguages = [ "eng" ]; }`,
      # pkgs/top-level/all-packages.nix). Measured eng-only closure: ~117MB.
      (tesseract.override { enableLanguages = [ "eng" ]; })
    ];
  };
}
