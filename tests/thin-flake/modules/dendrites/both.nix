# Both modules of one dendrite in one file: the NixOS module writes /etc, the Home Manager
# module writes the user's home.
{
  nixos = {
    environment.etc."thin-flake/both".text = "system";
  };
  homeManager = {
    home.file."thin-flake-both".text = "home";
  };
}
