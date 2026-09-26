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
# `$out/share/lyra/songbook/` = the song folders this system ships (every
# discovered one by default; a host's built-in set when the lyra lane overrides
# it) PLUS `manifest.json`/`registry.json` baked via `lib/songbook.nix` — the
# SAME generator the lyra lane's `quickshellConfig` derivation
# and the `songbookManifest` flake output both call, so this dir's baked files
# can never drift from what a checkout host's `nix eval` would produce for the
# same songs.
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
# That is `modules/dendrites/quickshell.nix`'s main `shellbridge` service
# (execs `lyra shellbridge --run`) plus `modules/nucleus/aoided.nix`'s
# `environment.sessionVariables`, itself gated on `aoide.lyra.enable` — the
# option that actually controls whether `lyra` is installed on this host —
# for an operator's own interactive `lyra rice compose`. On a NixOS host this
# env tier always wins, so the core Rust's OWN sibling-of-binary fallback
# tier is never actually exercised there; that tier exists for a future
# non-nix tarball install (`bin/lyra` + `share/lyra/songbook/` shipped side
# by side).
#
# ── Two knobs, and they are what "built in" means ─────────────────────────
# `songs` and `builtin` default to the WHOLE committed songbook and to no
# selection at all, which is what the flake's own `packages.lyra-songbook`
# wants. The lyra lane overrides both, per host, with the songs that host
# builds in (`lib/songbook.nix`'s `builtIn`): a `_server` that performs nothing
# ships an empty templates dir, and yomi ships sonata's folder and no other.
# This package is where "built in" stops being an attribute of the host and
# becomes bytes on disk — the baseline `aoide rice stage` falls back to, and the
# source `aoideSeedSongbook` seeds the machine's songbook from.
{
  lib,
  runCommand,
  jq,
  # The songs to ship, by name; `null` = every discovered song.
  songs ? null,
  # The host's selection as lyra records it: `{ declared, songs, packages }`.
  # `null` = none, and no `builtin.json` is written.
  builtin ? null,
}:
let
  songbook = ../../song/songbook;
  songbookData = import ../../lib/songbook.nix { inherit lib songbook; };

  # Baked manifest/registry are over the songs this instance SHIPS, not over
  # every song the repo happens to hold — a host that does not build a song in
  # has no `<song>/widgets/` in its deployed tree for a slot record to resolve
  # against.
  keep =
    attrs: if songs == null then attrs else lib.filterAttrs (name: _: builtins.elem name songs) attrs;

  shipped = if songs == null then songbookData.songNames else songs;

  files = {
    manifest = builtins.toFile "lyra-songbook-manifest.json" (
      builtins.toJSON (keep songbookData.manifestAttrs)
    );
    registry = builtins.toFile "lyra-songbook-registry.json" (
      builtins.toJSON (keep songbookData.registryAttrs)
    );
  }
  // lib.optionalAttrs (builtin != null) {
    builtin = builtins.toFile "lyra-songbook-builtin.json" (builtins.toJSON builtin);
  };
in
runCommand "lyra-songbook-templates" { nativeBuildInputs = [ jq ]; } ''
  out_dir="$out/share/lyra/songbook"
  mkdir -p "$out_dir"

  ${lib.concatMapStrings (name: ''
    cp -r ${songbook}/${name} "$out_dir/${name}"
  '') shipped}

  jq . ${files.manifest} > "$out_dir/manifest.json"
  jq . ${files.registry} > "$out_dir/registry.json"
  ${lib.optionalString (builtin != null) ''
    jq . ${files.builtin} > "$out_dir/builtin.json"
  ''}
''
