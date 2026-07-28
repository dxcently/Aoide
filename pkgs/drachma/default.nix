# pkgs/drachma/default.nix — the Aoide note engine (Node / Style Dictionary).
#
# drachma takes the Greek coin's name: notes are the design-token values, and
# drachma is the mint that resolves, validates, and emits them (on-theme with
# the Aoide/Melete/Mneme muse naming — concepts/Lexicon.md).
#
# drachma wraps Style Dictionary (concepts/Notes — wrap, don't rewrite) and
# provides, over note schema v0 (CONTRACTS.md §1):
#   * resolver     — tiered palette→semantic→component reference resolution.
#   * `lint`       — the authoritative v0 schema validator (`rice lint` calls it).
#   * three emitters — stage/drachma.json (Quickshell), hyprctl dispatch, OSC.
#
# Deps are vendored via buildNpmPackage's npmDepsHash, so the build is
# offline/pure. If package.json/package-lock.json change, refresh the hash with:
#   nix run nixpkgs#prefetch-npm-deps -- pkgs/drachma/package-lock.json
{
  lib,
  buildNpmPackage,
  nodejs,
  makeWrapper,
}:
buildNpmPackage {
  pname = "aoide-drachma";
  version = "0.0.0";

  src = lib.cleanSource ./.;

  inherit nodejs;

  # Vendored npm deps (Style Dictionary + its tree). Refresh with prefetch-npm-deps.
  npmDepsHash = "sha256-5fMHPqU3yVikfEP+ylwEYftYIR/v0LJe2y9KTVG4KVw=";

  # Pure library/CLI: no compile/bundle step. We install src + node_modules and
  # wrap a launcher, so the `npm run build` default is a no-op here.
  dontNpmBuild = true;

  nativeBuildInputs = [ makeWrapper ];

  # Lay down the package (src + vendored node_modules) into libexec and expose a
  # wrapped `drachma` on PATH. Wrapping (rather than the raw bin shebang)
  # pins the exact nodejs and keeps node_modules resolvable from any cwd.
  installPhase = ''
    runHook preInstall

    mkdir -p "$out/libexec/drachma"
    cp -r src package.json node_modules "$out/libexec/drachma/"

    mkdir -p "$out/bin"
    makeWrapper ${nodejs}/bin/node "$out/bin/drachma" \
      --add-flags "$out/libexec/drachma/src/cli.js"

    runHook postInstall
  '';

  # Smoke-test the wrapped binary against a v0 fixture during the build.
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    "$out/bin/drachma" lint test/fixtures/valid.json
    "$out/bin/drachma" resolve test/fixtures/valid.json > /dev/null
    "$out/bin/drachma" emit stage   test/fixtures/valid.json > /dev/null
    "$out/bin/drachma" emit hyprctl test/fixtures/valid.json > /dev/null
    "$out/bin/drachma" emit osc     test/fixtures/valid.json > /dev/null

    # Optional palette.hot (the one-neon trace colour): accepted when present,
    # a bad hex rejected, and it flows through resolve → stage untouched.
    "$out/bin/drachma" lint test/fixtures/valid-hot.json
    "$out/bin/drachma" resolve test/fixtures/valid-hot.json | grep -q '"hot"'
    "$out/bin/drachma" emit stage test/fixtures/valid-hot.json | grep -q '"hot"'
    if "$out/bin/drachma" lint test/fixtures/invalid-hot.json; then
      echo "drachma: invalid-hot.json should have failed lint" >&2
      exit 1
    fi

    # Optional base16 tier (the full sixteen-slot terminal scheme): accepted when
    # complete, flows through resolve → stage untouched, and is rejected when a
    # slot is missing or malformed.
    "$out/bin/drachma" lint test/fixtures/valid-base16.json
    "$out/bin/drachma" resolve test/fixtures/valid-base16.json | grep -q '"base0C"'
    "$out/bin/drachma" emit stage test/fixtures/valid-base16.json | grep -q '"base0C"'
    if "$out/bin/drachma" lint test/fixtures/invalid-base16-missing.json; then
      echo "drachma: invalid-base16-missing.json should have failed lint" >&2
      exit 1
    fi
    if "$out/bin/drachma" lint test/fixtures/invalid-base16-hex.json; then
      echo "drachma: invalid-base16-hex.json should have failed lint" >&2
      exit 1
    fi

    # The assert-driven schema tests (hot-accepted / hot-bad-hex-rejected).
    ${nodejs}/bin/node test/run.js
    runHook postInstallCheck
  '';

  # The `aoide` CLI shells out to `${drachma}/bin/drachma` for `rice lint`.
  passthru.mainProgram = "drachma";
  meta = {
    description = "Aoide drachma: the note engine — resolver + v0 schema lint + live emitters (Style Dictionary).";
    mainProgram = "drachma";
    license = lib.licenses.mit;
  };
}
