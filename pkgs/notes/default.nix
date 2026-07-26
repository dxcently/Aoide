# pkgs/notes/default.nix — the Aoide notes package (Node / Style Dictionary).
#
# Wave-1 (Agent A): replaces the Wave-0 placeholder IN PLACE. The `callPackage`
# signature is unchanged, so flake.nix never changes.
#
# `aoide-notes` wraps Style Dictionary (concepts/Notes — wrap, don't
# rewrite) and provides, over note schema v0 (CONTRACTS.md §1):
#   * resolver     — tiered palette→semantic→component reference resolution.
#   * `lint`       — the authoritative v0 schema validator (`rice lint` calls it).
#   * three emitters — stage/notes.json (Quickshell), hyprctl dispatch, OSC.
#
# Deps are vendored via buildNpmPackage's npmDepsHash, so the build is
# offline/pure. If package.json/package-lock.json change, refresh the hash with:
#   nix run nixpkgs#prefetch-npm-deps -- pkgs/notes/package-lock.json
{
  lib,
  buildNpmPackage,
  nodejs,
  makeWrapper,
}:
buildNpmPackage {
  pname = "aoide-notes";
  version = "0.0.0";

  src = lib.cleanSource ./.;

  inherit nodejs;

  # Vendored npm deps (Style Dictionary + its tree). Refresh with prefetch-npm-deps.
  npmDepsHash = "sha256-S+FOC8+RJHxV8zIFJmcpVWf/sUZuEN46RZzs1uDKbqY=";

  # Pure library/CLI: no compile/bundle step. We install src + node_modules and
  # wrap a launcher, so the `npm run build` default is a no-op here.
  dontNpmBuild = true;

  nativeBuildInputs = [ makeWrapper ];

  # Lay down the package (src + vendored node_modules) into libexec and expose a
  # wrapped `aoide-notes` on PATH. Wrapping (rather than the raw bin shebang)
  # pins the exact nodejs and keeps node_modules resolvable from any cwd.
  installPhase = ''
    runHook preInstall

    mkdir -p "$out/libexec/aoide-notes"
    cp -r src package.json node_modules "$out/libexec/aoide-notes/"

    mkdir -p "$out/bin"
    makeWrapper ${nodejs}/bin/node "$out/bin/aoide-notes" \
      --add-flags "$out/libexec/aoide-notes/src/cli.js"

    runHook postInstall
  '';

  # Smoke-test the wrapped binary against a v0 fixture during the build.
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    "$out/bin/aoide-notes" lint test/fixtures/valid.json
    "$out/bin/aoide-notes" resolve test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-notes" emit stage   test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-notes" emit hyprctl test/fixtures/valid.json > /dev/null
    "$out/bin/aoide-notes" emit osc     test/fixtures/valid.json > /dev/null
    runHook postInstallCheck
  '';

  # A later `aoide` CLI (Agent B) shells out to `${aoide-notes}/bin/aoide-notes`.
  passthru.mainProgram = "aoide-notes";
  meta = {
    description = "Aoide notes engine: resolver + v0 schema lint + live emitters (Style Dictionary).";
    mainProgram = "aoide-notes";
    license = lib.licenses.mit;
  };
}
