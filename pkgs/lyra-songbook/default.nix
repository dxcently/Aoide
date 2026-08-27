# pkgs/lyra-songbook/default.nix — the shipped SCORE TEMPLATES (L-C3,
# lyra-carrier lane, task #107).
#
# `pkgs/aoide` is self-flaked, nixpkgs-only, and its own source tree does NOT
# contain `song/songbook/` (that lives in this outer repo) — see that
# package's own `flake.nix` header, "Topology (b) of AOIDE-DEV §7". So the
# committed songbook cannot ride the core package derivation; it lives here
# instead, in the outer repo, discovered by `lib/pkgs.nix` like any other
# `pkgs/<name>`.
#
# `$out/share/lyra/songbook/` = a verbatim copy of the committed
# `song/songbook/` tree (every song's `rice.nix`/`livery.json`/`design/`/
# `widgets/`) PLUS `manifest.json`/`registry.json` baked via `lib/songbook.nix`
# — the SAME generator `modules/facets/quickshell/default.nix`'s
# `quickshellConfig` derivation and the `songbookManifest` flake output both
# use, so this dir's baked files can never drift from what a checkout host's
# `nix eval` would produce for the same committed songs.
#
# Consumed at runtime by `aoide_storage::fs::song_templates_dir` /
# `AOIDE_SONG_TEMPLATES` — `lyra rice compose --from <song>` and
# `aoide-song::widgets`'s registry/manifest regeneration both fall back to it
# on a repo-less host (no `$AOIDE_FLAKE_ROOT` checkout there to have composed
# FROM or to shell `nix eval` against). Wired onto every unit/shell that
# already carries `AOIDE_ROOT`/`AOIDE_FLAKE_ROOT`
# (`modules/nucleus/{aoided,shellbridge,secrets,melete-adapter}.nix`) as
# `AOIDE_SONG_TEMPLATES=${pkgs.lyra-songbook}/share/lyra/songbook` — on a
# NixOS host this env tier always wins, so the core Rust's OWN
# sibling-of-binary fallback tier is never actually exercised there; that
# tier exists for a future non-nix tarball install (`bin/lyra` +
# `share/lyra/songbook/` shipped side by side).
{
  lib,
  runCommand,
  jq,
}:
let
  songbook = ../../song/songbook;
  songbookData = import ../../lib/songbook.nix { inherit lib songbook; };

  manifestJsonFile = builtins.toFile "lyra-songbook-manifest.json" (
    builtins.toJSON songbookData.manifestAttrs
  );
  registryJsonFile = builtins.toFile "lyra-songbook-registry.json" (
    builtins.toJSON songbookData.registryAttrs
  );
in
runCommand "lyra-songbook-templates" { nativeBuildInputs = [ jq ]; } ''
  out_dir="$out/share/lyra/songbook"
  mkdir -p "$out_dir"
  cp -r ${songbook}/. "$out_dir/"

  jq . ${manifestJsonFile} > "$out_dir/manifest.json"
  jq . ${registryJsonFile} > "$out_dir/registry.json"
''
