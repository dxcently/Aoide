# pkgs/eidolon/default.nix — the Eidolon coding harness, launcher form.
#
# Same adaptation as pkgs/melete (see its header for the fuller rationale):
# eidolon's Cargo dependency on harnox (github.com/noah427/harnox) is a
# PRIVATE repo, and this repo must build offline with no token (root
# AGENTS.md: core is cargo-buildable on any Linux, no nix shell-outs — the
# packages walker here inherits the same "no authenticated build-time fetch"
# constraint). So this derivation does not compile eidolon at all: it ships a
# launcher that execs the runtime-deployed binary. That binary is built
# elsewhere — `nix build` inside ~/eidolon itself, against a plain local
# ~/harnox checkout (offline, no token needed there either; see
# ~/eidolon/nix/eidolon.nix and its `[patch]` in Cargo.toml) — and copied to
# $EIDOLON_BIN (default ~/.local/bin/eidolon).
#
# Unlike melete, eidolon has no self-update: redeploy by re-running that
# build and re-copying the binary after every source change.
{
  lib,
  stdenvNoCC,
  writeShellScript,
}:

let
  launcher = writeShellScript "eidolon" ''
    bin="''${EIDOLON_BIN:-$HOME/.local/bin/eidolon}"
    if [ ! -x "$bin" ]; then
      echo "eidolon: runtime binary not found at $bin" >&2
      echo 'eidolon: build it (nix build in ~/eidolon against a local ~/harnox checkout) and copy it there, or set $EIDOLON_BIN.' >&2
      exit 127
    fi
    exec "$bin" "$@"
  '';
in
stdenvNoCC.mkDerivation {
  pname = "eidolon";
  version = "0.1.0";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 ${launcher} $out/bin/eidolon
    runHook postInstall
  '';

  meta = {
    description = "Eidolon coding harness — launcher for the runtime-deployed binary (see ~/eidolon/nix/eidolon.nix for the real build)";
    mainProgram = "eidolon";
    platforms = lib.platforms.linux;
  };
}
