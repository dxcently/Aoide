# tests/standalone-home/hosts/host-config.nix — the one thing a standalone user names
# besides its identity: its host file.
_: {
  strillix.hostConfig = "/home/fixture/.config/strillix/host.nix";
}
