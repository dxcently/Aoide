# tests/consumer/aggregations/desktop/default.nix — the consumer's own
# aggregation.
#
# An aggregation is the CONSUMER's data: AoideOS's own groupings (`aoideos`,
# `nucleus`, `desktop`, `agents`) describe AoideOS's machines, and a stranger's
# registry carries its own. This one names the paint the fixture's contract
# asks for: the renderer (`lyra`), the ten vibeos components that paint the
# shell, and the theme underneath. No `quickshell` member: the vibeos providers
# bring the runtime, and the runtime carries the Quickshell lane with it.
{
  description = "The consumer's desktop: renderer, ten vibeos components, theme.";

  system = {
    members = [
      "lyra"
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
      bar = "vibeos";
      launcher = "vibeos";
      powermenu = "vibeos";
      notifications = "vibeos";
      wallpaper = "vibeos";
      dock = "vibeos";
      calendar = "vibeos";
      switcher = "vibeos";
      lock = "vibeos";
      greeter = "vibeos";
    };
  };
}
