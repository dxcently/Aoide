# pkgs/codex/default.nix — OpenAI's Codex CLI, from the official release bundle.
#
# nixpkgs builds codex from source (a long Rust + V8 build) and trails upstream
# releases; the release's `codex-package` tarball is what upstream's own
# installers ship: static-musl `codex` and `codex-code-mode-host`, plus the
# `codex-resources/` (bwrap, zsh, the voice host) and `codex-path/` (rg) it
# resolves relative to its own binary. The tree is installed as-is under
# lib/codex so those relative lookups hold.
#
# The voice host and the bundled zsh are dynamically linked; zsh takes
# libtinfo from ncurses, and the voice host carries its own GStreamer/GLib
# libraries in codex-resources/voice/lib, which autoPatchelf searches first.
#
# This deliberately shadows `pkgs.codex` (lib/pkgs.nix intentionalShadows).
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  installShellFiles,
  ncurses,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "codex";
  version = "0.160.0";

  src = fetchurl {
    url = "https://github.com/openai/codex/releases/download/rust-v${finalAttrs.version}/codex-package-x86_64-unknown-linux-musl.tar.gz";
    hash = "sha256-T8xHq1f1L/dTY5Uah2EUbNEMgoi9hv7UVIfbsgSha3E=";
  };

  sourceRoot = ".";

  nativeBuildInputs = [
    autoPatchelfHook
    installShellFiles
  ];
  buildInputs = [
    stdenv.cc.cc.lib
    ncurses # libtinfo, for the bundled zsh
  ];

  dontBuild = true;
  dontStrip = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/codex $out/bin
    cp -r bin codex-package.json codex-path codex-resources $out/lib/codex/
    ln -s $out/lib/codex/bin/codex $out/bin/codex
    runHook postInstall
  '';

  preFixup = ''
    addAutoPatchelfSearchPath $out/lib/codex/codex-resources/voice/lib
  '';

  postInstall = ''
    installShellCompletion --cmd codex \
      --bash <($out/bin/codex completion bash) \
      --fish <($out/bin/codex completion fish) \
      --zsh <($out/bin/codex completion zsh)
  '';

  meta = {
    description = "Lightweight coding agent that runs in your terminal";
    homepage = "https://github.com/openai/codex";
    changelog = "https://github.com/openai/codex/releases/tag/rust-v${finalAttrs.version}";
    license = lib.licenses.asl20;
    sourceProvenance = with lib.sourceTypes; [ binaryNativeCode ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "codex";
  };
})
