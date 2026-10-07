# desktop — a graphical session's platform, and nothing that paints it.
#
# The session's own surfaces are the `aoideos` aggregation, which is why a host
# can take this one and answer the shell question differently (`quickshell`
# selected with its own `aoide.quickshell.config`, or not at all). What is here
# is what a graphical session needs regardless of who draws it: sound, the
# clipboard, notifications, the network applet, a browser, and the two halves of
# screenshotting (hyprshot/satty for the human, grim/slurp for agents).
{
  description = "A graphical session: audio, clipboard, notifications, screenshots, network plumbing, a browser.";

  system = {
    members = [
      "audio"
      "clipboard"
      "dunst"
      "firefox"
      "networkmanager"
      "screenshot"
      "vision"
    ];

    # Graphics support is the platform's, not a package's: every host that takes
    # this aggregation is running a compositor or a shell, and mkDefault keeps a
    # headless host free to say otherwise. `enable32Bit` is NOT defaulted — a
    # host that needs 32-bit userspace says so itself (yomi-strix does).
    module =
      { lib, ... }:
      {
        hardware.graphics.enable = lib.mkDefault true;
      };
  };
}
