# A value outside lib.mkDefault: a rice's default would outrank the host's.
{
  system = {
    providers.greeter = "vibeos";
    module = { lib, ... }: {
      aoide.livery.polarity = "dark";
    };
  };
  home = {
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
