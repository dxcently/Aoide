# Every value under lib.mkDefault, rice.hyprland.* beside them; a package
# reference inside a list is never forced, so the stub package set is enough.
{
  system = {
    providers.greeter = "strillix";
    module = { lib, pkgs, ... }: {
      config = lib.mkDefault {
        aoide.livery.polarity = "dark";
        environment.systemPackages = [ pkgs.jq ];
      };
    };
  };
  home = {
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
