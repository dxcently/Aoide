# tests/standalone-home/hosts/bad-enable.nix — writes the paint fact, which mkFact
# refuses: reading the fact must fail.
_: {
  strillix.enable = true;
}
