# tests/portability.nix — content for the `portability` check (lib/checks.nix
# Check 8): proves a built package is genuinely free of nix, not merely built
# with the right cargo flags.
#
# Wired as thin invocation from lib/checks.nix, per this phase's split: lib/
# is gate wiring, tests/ is the check content itself. Takes the package to
# examine as an argument so the same logic runs against `aoide-static` (must
# pass) or `packages.default` (must fail — the negative proof this check's
# existence depends on; see the commit report, not repeated here).
#
# Fast and hermetic: no network, no compositor, no sockets.
{ lib, pkgs }:
pkg:
pkgs.runCommand "aoide-check-portability"
  {
    nativeBuildInputs = [
      pkgs.binutils
      pkgs.jq
    ];
  }
  ''
    set -euo pipefail
    status=0

    for bin in aoide aoided; do
      path="${pkg}/bin/$bin"

      # 1. No PT_INTERP segment — nothing here expects a dynamic loader.
      interp_count=$(readelf -l "$path" | grep -c INTERP || true)
      if [ "$interp_count" -ne 0 ]; then
        echo "portability: $bin carries $interp_count INTERP segment(s)" >&2
        status=1
      fi

      # 2. No genuine /nix/store path baked in. A bare `/nix/store` substring
      # is not itself a violation: crates/upkeep/src/scan.rs's soundcheck
      # `clutter` finding legitimately contains the diagnostic text "a
      # /nix/store symlink nix recreates on the next build", and cli depends
      # on upkeep — grepping the literal would false-positive on both
      # binaries every time. A real leaked store path always carries the
      # 32-char content hash; that diagnostic string never does.
      store_count=$(grep -aEc '/nix/store/[a-z0-9]{32}-' "$path" || true)
      if [ "$store_count" -ne 0 ]; then
        echo "portability: $bin bakes in $store_count real /nix/store path(s)" >&2
        status=1
      fi
    done

    # 3 + 4. Runs with no nix on PATH, no HOME but a scratch one — the shape
    # of a distrobox/alpine drop-in with no store to fall back on.
    schema_raw=$(env -i HOME="$TMPDIR" "${pkg}/bin/aoide" schema --json)
    cmd_count=$(printf '%s' "$schema_raw" | jq '.commands | length')
    if [ "$cmd_count" -le 0 ]; then
      echo "portability: schema --json reported zero commands" >&2
      status=1
    fi

    env -i HOME="$TMPDIR" "${pkg}/bin/aoide" guide > /dev/null

    [ "$status" -eq 0 ]
    printf 'aoide check portability: ok (%d commands, 0 INTERP, 0 baked-in store paths)\n' \
      "$cmd_count" > "$out"
  ''
