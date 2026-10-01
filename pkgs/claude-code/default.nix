# pkgs/claude-code/default.nix — the Claude Code CLI, pinned ahead of nixpkgs.
#
# nixpkgs' own recipe, fed a newer release manifest. Its package.nix takes the
# manifest as an argument, so the build (zstd unpack, autoPatchelf, the
# DISABLE_AUTOUPDATER wrapper, ripgrep/bubblewrap/socat on PATH) stays
# nixpkgs'; only the version this file pins is Aoide's.
#
# This deliberately shadows `pkgs.claude-code` (lib/pkgs.nix
# intentionalShadows): every consumer of the overlay, the claude-code dendrite
# and a host's spawnPath alike, gets this one build.
{
  lib,
  path,
  callPackage,
}:
callPackage (path + "/pkgs/by-name/cl/claude-code/package.nix") {
  manifest = lib.importJSON ./manifest.zst.json;
}
