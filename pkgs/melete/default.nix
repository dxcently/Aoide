# pkgs/melete/default.nix — the Melete AI harness binary, Aoide packaging.
#
# Ported from dxflake pkgs/melete. dxflake fetched a PREBUILT release binary
# via an authenticated fixed-output derivation (curl with a Bearer token from
# `NIX_MELETE_READ_TOKEN`, reachable only through nix.conf's `impure-env`, which
# in dxflake was rendered by sops). See the dxflake header for the mechanics:
#
#   version = "canary-9d8de3c"  (github.com/noah427/melete)
#   url     = https://melete-distributor.rdct.dev/artifacts/<ver>/melete-x86_64-unknown-linux-musl
#   sha256  = M8AS75mX/+Beuve9UbAOXUueNjr9NetN2Ko8Kq99bNA=  (flat FOD, __impureEnvVars NIX_MELETE_READ_TOKEN)
#
# ── Secret/network adaptation (Aoide has no sops stack) ────────────────────────
# Aoide must build this package with `nix build .#melete` — offline, no token.
# An authenticated FOD fetch cannot satisfy that, so the build-time fetch is
# LIFTED OUT to a runtime seam, which also matches how the binary actually runs:
# even in dxflake the store binary only SEEDED ~/.local/bin/melete once, and
# Melete's own self-update owned the running binary from then on (the store path
# was never on the hot path). So this derivation ships a launcher wrapper that
# execs the runtime-deployed binary from a configurable location (default
# ~/.local/bin/melete, overridable via $MELETE_BIN). If no runtime binary is
# present it exits with a clear, actionable message rather than a broken symlink.
#
# The melete dendrite (modules/dendrites/melete.nix) owns the deployment seam:
# it exposes `aoide.melete.binPath` / `aoide.melete.envFile` options and seeds
# the runtime binary out-of-band. To restore the reproducible FOD on a host that
# HAS the token, replace the launcher with the dxflake fetch above and wire
# `impure-env = NIX_MELETE_READ_TOKEN=<token>` into that host's nix.conf.
{
  lib,
  stdenvNoCC,
  writeShellScript,
}:

let
  # Built as a proper shell script (correct shebang, no heredoc/indent hazards).
  # $MELETE_BIN / $HOME / "$@" expand at RUNTIME, not build time.
  launcher = writeShellScript "melete" ''
    # Melete launcher (Aoide). Execs the runtime-deployed Melete binary. The
    # real binary is deployed out-of-band (self-updating; see pkgs/melete
    # header) — this store path only provides a stable entrypoint + version pin.
    bin="''${MELETE_BIN:-$HOME/.local/bin/melete}"
    if [ ! -x "$bin" ]; then
      echo "melete: runtime binary not found at $bin" >&2
      echo 'melete: deploy it (or set $MELETE_BIN); see aoide.melete.binPath.' >&2
      exit 127
    fi
    exec "$bin" "$@"
  '';
in
stdenvNoCC.mkDerivation {
  pname = "melete";
  # Track the dxflake pin so `meletePkg.version` (read by the dendrite's seed
  # stamp) stays meaningful across canary bumps.
  version = "canary-9d8de3c";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 ${launcher} $out/bin/melete
    runHook postInstall
  '';

  meta = {
    description = "Melete AI harness — launcher for the runtime-deployed, self-updating binary (canary-9d8de3c pin)";
    mainProgram = "melete";
    platforms = lib.platforms.linux;
  };
}
