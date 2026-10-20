# pkgs/aoide/module/default.nix — composes the core deployment bundle,
# exported as `nixosModules.default` (pkgs/aoide/flake.nix).
{ self }:
{
  imports = [
    ./options.nix
    ./aoided.nix
    ./reap.nix
  ];
  nixpkgs.overlays = [ self.overlays.default ];
}
