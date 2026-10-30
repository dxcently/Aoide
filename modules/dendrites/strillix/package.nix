# modules/dendrites/strillix/package.nix — the strillix binary this dendrite
# installs and runs: the aoide input's `strillix` output, with its shipped rices named by default.
#
# `makeWrapper --set-default` exports STRILLIX_TEMPLATES only while the variable
# is unset, so a value already in the environment still wins. `strx` links to
# the wrapper, the way core's own output links `strx` to `strillix`.
#
# A plain function imported by path (runtime.nix, runtime-home.nix and shellbridge.nix):
# not a module, and not a catalogue entry. The caller passes the core package and the
# shipped rices set, so a standalone home needs no overlay to name them.
{
  pkgs,
  aoide,
  strillixRices,
}:
pkgs.runCommand "strillix" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
  mkdir -p "$out/bin"
  makeWrapper ${aoide.strillix}/bin/strillix "$out/bin/strillix" \
    --set-default STRILLIX_TEMPLATES ${strillixRices}/share/strillix/rices
  ln -s strillix "$out/bin/strx"
''
