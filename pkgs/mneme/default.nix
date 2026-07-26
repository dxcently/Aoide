# pkgs/mneme/default.nix — the Mneme vault MCP server, Aoide packaging.
#
# In dxflake Mneme was NOT a separate packaged derivation: the mneme binary was
# deployed out-of-band to ~/.local/bin/mneme and modules/dendrites/mneme.nix
# only wired the systemd services (mneme + a dedicated userspace tailscaled
# node for its own :443 funnel). So there is no upstream `pkgs/mneme` to port.
#
# Aoide gives Mneme a package of the SAME launcher shape as pkgs/melete: a
# buildable wrapper providing a stable `mneme` entrypoint that execs the
# runtime-deployed binary (default ~/.local/bin/mneme, overridable via
# $MNEME_BIN). This keeps `nix build .#mneme` offline/secret-free while the
# real, out-of-band binary stays the one that runs — matching dxflake's reality.
#
# The mneme dendrite (modules/dendrites/mneme.nix) owns the deployment/runtime
# seam (binPath, vault path, env file).
{
  lib,
  stdenvNoCC,
  writeShellScript,
}:

let
  launcher = writeShellScript "mneme" ''
    # Mneme launcher (Aoide). Execs the runtime-deployed Mneme binary, deployed
    # out-of-band (see pkgs/mneme header). This store path is a stable entrypoint.
    bin="''${MNEME_BIN:-$HOME/.local/bin/mneme}"
    if [ ! -x "$bin" ]; then
      echo "mneme: runtime binary not found at $bin" >&2
      echo 'mneme: deploy it (or set $MNEME_BIN); see aoide.mneme.binPath.' >&2
      exit 127
    fi
    exec "$bin" "$@"
  '';
in
stdenvNoCC.mkDerivation {
  pname = "mneme";
  version = "0.0.0";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 ${launcher} $out/bin/mneme
    runHook postInstall
  '';

  meta = {
    description = "Mneme — Obsidian/plain-vault MCP server; launcher for the runtime-deployed binary";
    mainProgram = "mneme";
    platforms = lib.platforms.linux;
  };
}
