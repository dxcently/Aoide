# The NixOS module that `deep/default.nix` would name, were its marker rendered. It is
# never imported: no host gets far enough to read it.
{
  environment.etc."thin-flake/deep".text = "deep-one";
}
