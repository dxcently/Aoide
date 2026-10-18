# A base hyprland key under lib.mkDefault still belongs to the base: the rice's
# hyprland keys are rice.hyprland.*, whatever the priority.
{
  home = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        wayland.windowManager.hyprland.extraConfig = "bind = SUPER, Q, exec, foot";
      };
    };
  };
}
