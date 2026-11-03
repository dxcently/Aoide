# The NixOS module of `sysonly`, a record that names only `nixos`: no Home Manager module, so
# the selection adds nothing to any user's home.
{
  environment.etc."thin-flake/sysonly".text = "sysonly";
}
