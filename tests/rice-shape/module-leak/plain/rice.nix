# A value outside lib.mkDefault: a rice's default would outrank the host's.
{
  system = {
    providers.greeter = "strillix";
    module = { lib, ... }: {
      strillix.theme.polarity = "dark";
    };
  };
  home = {
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
