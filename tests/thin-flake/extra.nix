# tests/thin-flake/extra.nix — the module the `extra` registry source names. A plain module: the
# thin host selects it, and no other host does.
{
  environment.etc."thin-flake/extra".text = "registries";
}
