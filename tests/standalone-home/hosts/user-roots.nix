# tests/standalone-home/hosts/user-roots.nix — the user names its own core user and roots:
# the standalone defaults must yield to every one of them.
_: {
  aoide.user = "someone";
  aoide.root = "/srv/aoide-root";
  strillix.root = "/srv/strillix-root";
}
