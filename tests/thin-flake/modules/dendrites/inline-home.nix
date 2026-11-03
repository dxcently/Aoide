# The Home Manager module of `inline`, a record that names only `homeManager`: a module the
# user's Home Manager takes, and no NixOS module at all.
{
  home.file."thin-flake-inline".text = "inline";
}
