# tests/consumer/aggregations/desktop/default.nix — the consumer's own
# aggregation.
#
# An aggregation is the CONSUMER's data: Strillix's own groupings (`strillix`,
# `nucleus`, `desktop`, `agents`) describe Strillix's machines, and a stranger's
# registry carries its own. This one names the paint the fixture's contract
# asks for: the renderer (`strillix`), the ten strillix components that paint the
# shell, and the theme underneath. No `quickshell` member: the strillix providers
# bring the runtime, and the runtime carries the Quickshell lane with it.
{
  description = "The consumer's desktop: renderer, ten strillix components, theme.";

  system = {
    members = [
      "strillix"
      "stylix"
      "bar"
      "launcher"
      "powermenu"
      "notifications"
      "wallpaper"
      "dock"
      "calendar"
      "switcher"
      "lock"
      "greeter"
    ];

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
  };
}
