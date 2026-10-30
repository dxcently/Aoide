# tests/standalone-home/hosts/bad-runtime.nix — writes the runtime mark. The standalone
# home already defines it (false), so this is a second definition: readOnly refuses it.
_: {
  strillix.runtime.nixos = true;
}
