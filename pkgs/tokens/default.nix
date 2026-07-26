# pkgs/tokens/default.nix — the Aoide design-token package (Node / Style Dictionary).
#
# Wave-1 (Agent A): replaces the Wave-0 placeholder IN PLACE. The `callPackage`
# signature is unchanged, so flake.nix never changes.
#
# `aoide-tokens` wraps Style Dictionary (concepts/Design-Tokens — wrap, don't
# rewrite) and provides, over token schema v0 (CONTRACTS.md §1):
#   * resolver     — tiered palette→semantic→component reference resolution.
#   * `lint`       — the authoritative v0 schema validator (`rice lint` calls it).
#   * three emitters — stage/tokens.json (Quickshell), hyprctl dispatch, OSC.
#
# Deps are vendored via buildNpmPackage's npmDepsHash, so the build is
# offline/pure. If package.json/package-lock.json change, refresh the hash with:
#   nix run nixpkgs#prefetch-npm-deps -- pkgs/tokens/package-lock.json
{
  lib,
  buildNpmPackage,
  nodejs,
  makeWrapper,
}:
buildNpmPackage {
  pname = "aoide-tokens";
  version = "0.0.0";

  src = lib.cleanSource ./.;

  inherit nodejs;

  # Vendored npm deps (Style Dictionary + its tree). Refresh with prefetch-npm-deps.
  npmDepsHash = "sha256-mo5+kT4j28KUmh/nygWFCqqbVATXDc+tySO0GDUV110=";

  # Pure library/CLI: no compile/bundle step. We install src + node_modules and
  # wrap a launcher, so the `npm run build` default is a no-op here.
  dontNpmBuild = true;

  nativeBuildInputs = [ makeWrapper ];

  # Lay down the package (src + vendored node_modules) into libexec and expose a
  # wrapped `aoide-tokens` on PATH. Wrapping (rather than the raw bin shebang)
  # pins the exact nodejs and keeps node_modules resolvable from any cwd.
  installPhase = ''
    runHook preInstall

    mkdir -p "$out/libexec/aoide-tokens"
    cp -r src package.json node_modules "$out/libexec/aoide-tokens/"

    mkdir -p "$out/bin"
    makeWrapper ${nodejs}/bin/node "$out/bin/aoide-tokens" \
      --add-flags "$out/libexec/aoide-tokens/src/cli.js"

    runHook postInstall
  '';

  # Smoke-test the wrapped binary against a v0 fixture during the build.
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    "$out/bin/aoide-tokens" lint test/fixtures/valid.json
    "$out/bin/aoide-tokens" resolve test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-tokens" emit stage   test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-tokens" emit hyprctl test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-tokens" emit osc     test/fixtures/valid.json > /dev/null
    runHook postInstallCheck
  '';

  # A later `aoide` CLI (Agent B) shells out to `${aoide-tokens}/bin/aoide-tokens`.
  passthru.mainProgram = "aoide-tokens";
  meta = {
    description = "Aoide design-token engine: resolver + v0 schema lint + live emitters (Style Dictionary).";
    mainProgram = "aoide-tokens";
    license = lib.licenses.mit;
  };
}
