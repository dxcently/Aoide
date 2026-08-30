# pkgs/lyra-songbook/default.nix — the shipped SCORE TEMPLATES (L-C3,
# lyra-carrier lane, task #107).
#
# `pkgs/aoide` is self-flaked, nixpkgs-only, and its own source tree does NOT
# contain `song/songbook/` (that lives in this outer repo) — see that
# package's own `flake.nix` header, "Topology (b)". So the
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
# FROM or to shell `nix eval` against). ONLY `aoide-song` reads this var, and
# only `lyra` links `aoide-song` — so it is wired onto exactly the units
# whose PROCESS actually execs `lyra`, not every unit that happens to carry
# `AOIDE_ROOT`/`AOIDE_FLAKE_ROOT` (review finding, task #107: paint data on a
# headless-core unit only drags this package's closure onto it for nothing).
# That is `modules/nucleus/shellbridge.nix`'s main `shellbridge` service
# (execs `lyra shellbridge --run`) plus `modules/nucleus/aoided.nix`'s
# `environment.sessionVariables`, itself gated on `aoide.lyra.enable` — the
# option that actually controls whether `lyra` is installed on this host —
# for an operator's own interactive `lyra rice compose`. On a NixOS host this
# env tier always wins, so the core Rust's OWN sibling-of-binary fallback
# tier is never actually exercised there; that tier exists for a future
# non-nix tarball install (`bin/lyra` + `share/lyra/songbook/` shipped side
# by side).
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
