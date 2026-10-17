#!/usr/bin/env bash
# song-shape's livery check, against fixture songbooks (the real songbook is
# the flake's own `song-shape` check). A fixture song that sets a livery-owned
# key in its hyprland.conf must fail AND name the file; a clean one must pass.
#
#   ./tests/song-shape/run.sh
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)
rev=$(jq -r '.nodes.nixpkgs.locked.rev' "$root/flake.lock")
lib="(builtins.getFlake \"github:nixos/nixpkgs/$rev\").lib"

check() {
  local song=$1
  nix eval --impure --raw --expr "
    let
      lib = $lib;
      checks = import $root/lib/checks.nix { inherit lib; pkgs = { runCommand = _: _: _: \"ok\"; }; };
    in checks.songShape {
      songbook = $PWD/livery-leak;
      songNames = [ \"$song\" ];
      strayNixFiles = [ ];
      escapingNixFiles = [ ];
    }" 2>&1
}

fail=0
out=$(check leaky)
if [[ $out == *"livery-owned key"*"leaky"* ]]; then echo "ok    leaky: blur dotted key refused"; else echo "FAIL  leaky: $out"; fail=1; fi
out=$(check clean)
if [[ $out == *"ok"* && $out != *FAILED* ]]; then echo "ok    clean: passes"; else echo "FAIL  clean: $out"; fail=1; fi
exit $fail
