#!/usr/bin/env bash
# rice-shape's fixture checks, against fixture rices (the real rices is
# the flake's own `rice-shape` check). Each fixture rice must pass or fail as
# named, and a failure must name the rice and the rule it broke.
#
#   theme-leak/  a hyprland.conf that sets a theme-owned key fails; a clean one passes.
#   module-leak/  a rice module value outside lib.mkDefault fails, and so does a
#                 write to any wayland.windowManager.hyprland.* key, even under
#                 lib.mkDefault; a rice that wraps its values, and keeps
#                 rice.hyprland beside them, passes.
#
#   wallpaper-name/  a declared picture under wallpaper/<rice>-desktop.<ext> passes;
#                    any other name fails (the seed and `rice stage` must agree).
#
#   ./tests/rice-shape/run.sh
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)
rev=$(jq -r '.nodes.nixpkgs.locked.rev' "$root/flake.lock")
lib="(builtins.getFlake \"github:nixos/nixpkgs/$rev\").lib"

check() {
  local dir=$1 rice=$2
  nix eval --impure --raw --expr "
    let
      lib = $lib;
      checks = import $root/lib/checks.nix { inherit lib; pkgs = { runCommand = _: _: _: \"ok\"; }; };
    in checks.riceShape {
      ricesRoot = $PWD/$dir;
      discoveredNames = [ \"$rice\" ];
      strayNixFiles = [ ];
      escapingNixFiles = [ ];
      borrow = _: throw \"the fixtures borrow nothing\";
    }" 2>&1
}

fail=0
out=$(check theme-leak leaky)
if [[ $out == *"theme-owned key"*"leaky"* ]]; then echo "ok    leaky: blur dotted key refused"; else echo "FAIL  leaky: $out"; fail=1; fi
out=$(check theme-leak clean)
if [[ $out == *"ok"* && $out != *FAILED* ]]; then echo "ok    clean: passes"; else echo "FAIL  clean: $out"; fail=1; fi

out=$(check module-leak plain)
if [[ $out == *"rice plain's system.module sets strillix.theme.polarity outside lib.mkDefault"* ]]; then echo "ok    plain: unwrapped value refused"; else echo "FAIL  plain: $out"; fail=1; fi
out=$(check module-leak settings)
if [[ $out == *"rice settings's home.module writes wayland.windowManager.hyprland.settings"* ]]; then echo "ok    settings: base settings write refused"; else echo "FAIL  settings: $out"; fail=1; fi
out=$(check module-leak extra)
if [[ $out == *"rice extra's home.module writes wayland.windowManager.hyprland.extraConfig"* ]]; then echo "ok    extra: base extraConfig write refused under mkDefault"; else echo "FAIL  extra: $out"; fail=1; fi
out=$(check module-leak wrapped)
if [[ $out == *"ok"* && $out != *FAILED* ]]; then echo "ok    wrapped: mkDefault values and rice.hyprland pass"; else echo "FAIL  wrapped: $out"; fail=1; fi
out=$(check wallpaper-name named)
if [[ $out == *"ok"* && $out != *FAILED* ]]; then echo "ok    named: wallpaper/<rice>-desktop picture passes"; else echo "FAIL  named: $out"; fail=1; fi
out=$(check wallpaper-name stray)
if [[ $out == *"sets strillix.theme.wallpaper to"*"dusk.jpg"*"wallpaper/stray-desktop"* ]]; then echo "ok    stray: other-named picture refused"; else echo "FAIL  stray: $out"; fail=1; fi
exit $fail
