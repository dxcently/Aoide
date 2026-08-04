# pkgs/kimi-code/default.nix — Kimi Code CLI, Moonshot AI's terminal coding
# agent (github.com/MoonshotAI/kimi-code). Sibling of claude-code (nixpkgs) in
# the devtools dendrite: another agentic CLI on the same PATH, not vendored.
#
# Upstream ships prebuilt, single-file platform binaries as GitHub Release
# assets (no npm/Node required to RUN it, only to build it) — a genuine public,
# unauthenticated download, unlike melete/mneme's out-of-band launcher-stub
# pattern (those exist only because their upstream fetch needs a bearer token
# Aoide has no sops stack to hold). So this is a real, reproducible,
# offline-buildable fetchurl derivation, same shape as pkgs/hyprglass.
#
# The Linux x64 asset is a dynamically-linked ELF (glibc, libstdc++) expecting
# /lib64/ld-linux-x86-64.so.2 — not NixOS-portable as-is, hence autoPatchelfHook
# to rewrite its interpreter/rpath against stdenv's glibc + libstdc++.
#
# Version bump: fetch the new release's linux-x64 asset digest from
#   https://api.github.com/repos/MoonshotAI/kimi-code/releases/latest
# (the "digest" field on the "kimi-code-linux-x64.zip" asset, GitHub-computed
# sha256 over the raw file — verified by hand against a local sha256sum before
# pinning, not taken on faith from any fetched page) and convert to SRI with
# `nix hash to-sri --type sha256 <hex>`.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  unzip,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "kimi-code";
  version = "0.31.1";

  src = fetchurl {
    url = "https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%40${finalAttrs.version}/kimi-code-linux-x64.zip";
    hash = "sha256-2njpSLACqNgH6uirfLVRg48zAyHRR/N2t8exh8DTHj8=";
  };

  nativeBuildInputs = [
    unzip
    autoPatchelfHook
  ];

  # Needs libstdc++/libgcc from the compiler runtime; libc/libm/libpthread/
  # libdl/ld-linux come from stdenv's default glibc, already on autoPatchelfHook's
  # search path.
  buildInputs = [ stdenv.cc.cc.lib ];

  # The zip contains exactly one file (`kimi`, no wrapping directory) —
  # stdenv's default unpackPhase requires an extracted directory to `cd` into
  # and errors ("produced no directories") on a bare-file archive, so unpack
  # by hand instead.
  unpackPhase = ''
    runHook preUnpack
    unzip -q "$src" -d .
    runHook postUnpack
  '';

  dontConfigure = true;
  dontBuild = true;

  # This is a Bun-compiled single-file executable: the JS bundle is raw data
  # APPENDED after the ELF body, read by offset at runtime. stdenv's default
  # fixupPhase strips the binary, which truncates/corrupts that trailing
  # payload — the symptom is SIGILL on launch, not a missing-symbol error.
  dontStrip = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 kimi "$out/bin/kimi"
    runHook postInstall
  '';

  meta = {
    description = "Kimi Code CLI — Moonshot AI's terminal coding agent";
    homepage = "https://github.com/MoonshotAI/kimi-code";
    license = lib.licenses.mit;
    mainProgram = "kimi";
    platforms = [ "x86_64-linux" ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
})
