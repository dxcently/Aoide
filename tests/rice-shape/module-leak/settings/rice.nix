# The rice's hyprland keys are rice.hyprland.*, never the base's settings.
{
  home = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        wayland.windowManager.hyprland.settings.bind = [ "SUPER, Q, exec, foot" ];
      };
    };
  };
}
