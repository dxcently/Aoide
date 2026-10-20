# pkgs/strillix-shell/default.nix — the strillix shell's SOURCE, shipped.
#
# What this IS: the QML tree, the resolved icon assets and the preview fixture
# sets a shell gets BUILT FROM. `${pkg}/share/strillix/{qml,icons,preview}` mirrors
# the checkout's own `pkgs/strillix-shell/{qml,icons,preview}` byte for byte, so a
# host can carry the shell's source into a store path instead of reaching back
# into a git checkout at build time.
#
# What this is NOT: a shell. Nothing runs from here, nothing is installed from
# here, and no config is named here — which directory quickshell is pointed at
# is `strillix.quickshell.config`, and what fills that directory (a rice's
# widgets, its manifest/registry/surfaces) is a host's own business. The
# copy-out is the consumer's step: the strillix lane's build copies `qml/`
# into its generated config, and the strillix dendrite does the same with the rices
# staged alongside.
#
# Discovered by `lib/pkgs.nix` like every other `pkgs/<name>` — no hand-list
# names it. The name is `strillix-shell`, NEVER `strillix`: nixpkgs already carries a
# `strillix` (an unrelated C++ parser) so the overlay's collision guard would
# throw, and `packages.strillix` is the CORE flake's own rice output
# (docs/architecture/PACKAGE-LAYOUT.md, "Two binaries") — a shell under that
# name would put a second meaning one attrpath away from the first.
{ runCommand }:
runCommand "strillix-shell" { } ''
  mkdir -p "$out/share/strillix"
  cp -r ${./qml} "$out/share/strillix/qml"
  cp -r ${./icons} "$out/share/strillix/icons"
  cp -r ${./preview} "$out/share/strillix/preview"
''
