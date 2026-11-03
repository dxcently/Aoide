# A plain module, not a byClass record: it has no `nixos` or `homeManager` key, so mkFlake must
# refuse it by name when the `bad` host selects it.
{
  environment.etc."thin-flake/bad".text = "bad";
}
