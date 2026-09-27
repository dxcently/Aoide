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
# `$out/share/lyra/` holds three things, one per consumer:
#
#   songbook/          the song folders this system ships (every discovered one
#                      by default; a host's built-in set when the lyra lane
#                      overrides it) PLUS `manifest.json`/`registry.json`/
#                      `builtin.json` baked via `lib/songbook.nix` — the SAME
#                      generator the lyra lane's `quickshellConfig` derivation
#                      and the `songbookManifest` flake output both call, so
#                      this dir's baked files can never drift from what the
#                      same songs evaluate to anywhere else.
#   nix/               the OFFLINE generator and its dependencies
#                      (`manifest.nix` + copies of `lib/{songbook,song}.nix` +
#                      the locked nixpkgs `lib/`) — what a machine with no
#                      Aoide checkout evaluates to regenerate the two files
#                      over its OWN songbook. See `nix/manifest.nix`.
#   aoide-options.json `lib/options.nix`'s output, so `lyra onboard` derives the
#                      `aoide.*` option set without a checkout and without nix.
#
# Consumed at runtime by `aoide_storage::fs::song_templates_dir` /
# `AOIDE_SONG_TEMPLATES` (songbook/) and by its parent (nix/,
# aoide-options.json) — `lyra rice compose --from <song>`,
# `aoide-song::widgets`'s registry/manifest regeneration, and `lyra onboard`
# all read it on a repo-less host (no `$AOIDE_FLAKE_ROOT` checkout there). ONLY
# `aoide-song`/`aoide-lyra` read this var, and
# only `lyra` links them — so it is wired onto exactly the units
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
  pkgs,
  # The `aoide.*` option doc list (`lib/options.nix`), shipped as
  # `share/lyra/aoide-options.json` for `lyra onboard`. `callPackage` cannot
  # reach it — it is derived from the flake's `inputs` — so it arrives by name
  # from the build context that has them (`lib/pkgs.nix`'s `extra`).
  aoideOptions,
  # The songs to ship, by name; `null` = every discovered song.
  songs ? null,
  # The host's selection as lyra records it: `{ declared, songs, packages }`.
  # `null` = none, and no `builtin.json` is written.
  builtin ? null,
  # The songbook those names live in. A consumer's songs are in the CONSUMER's
  # tree, so this is an argument and not a path literal into Aoide's own
  # (`lib/songbook.nix`'s `root` is what the lanes pass; the flake's own
  # `packages` output keeps its default).
  songbook ? ../../song/songbook,
}:
let
  songbookData = import ../../lib/songbook.nix { inherit lib songbook; };

  # ── The offline generator (§9) ──────────────────────────────────────────
  # `share/lyra/nix/` is the songbook generator, shipped so a machine with no
  # Aoide checkout can regenerate manifest.json/registry.json over its OWN
  # songbook with a plain `nix-instantiate --eval` on a FILE — no flake, no
  # network, no checkout, no `--impure`. `nix/manifest.nix` is the entry
  # (`aoide-song::widgets` names it); the two `.nix` beside it are COPIES of
  # `lib/songbook.nix` and `lib/song.nix`, and `lib/` is the locked nixpkgs
  # `lib/`. Copies, not a second implementation: one generator, shipped.
  #
  # The whole `lib/` rather than a subset of the functions used today (Q-3):
  # the generator calls 15 of them and a song's `_widgets/` shelf is handed
  # `lib` too (`hasSuffix` today), so a shim's boundary would be silent —
  # a future shelf calling one more function would fail only at RUNTIME, on
  # the machine, and never in this build. 2.4 MB is the price of that being
  # impossible.
  nixpkgsLib = builtins.path {
    path = pkgs.path + "/lib";
    name = "nixpkgs-lib";
    filter = path: _: builtins.baseNameOf path != "tests";
  };

  aoideOptionsJson = builtins.toFile "aoide-options.json" (builtins.toJSON aoideOptions);

  # Baked manifest/registry are over the songs this instance SHIPS, not over
  # every song the repo happens to hold — a host that does not build a song in
  # has no `<song>/widgets/` in its deployed tree for a slot record to resolve
  # against.
  keep =
    attrs: if songs == null then attrs else lib.filterAttrs (name: _: builtins.elem name songs) attrs;

  shipped = if songs == null then songbookData.songNames else songs;

  # ── What this derivation actually reads ─────────────────────────────────────
  # Each shipped song enters as ITS OWN store path (`builtins.path` copies the
  # folder), never as a subdirectory of the whole `song/songbook` tree. That is
  # not tidiness: a path literal naming the songbook DIRECTORY makes every song's
  # content part of this derivation — and the templates path is a string in a
  # host's session variables and activation, so a host's whole toplevel drvPath
  # moved whenever any song changed. A host that builds one song in reads that
  # one song's folder, and a song it does not build in cannot move its system.
  #
  # The manifest/registry below are the same story from the other side: they are
  # `builtins.toFile` over the SHIPPED set, so they do not carry the others
  # either. The unoverridden package (`songs = null`) is every discovered song,
  # each with its own folder path — so the flake's own `packages` output still
  # ships the whole songbook.
  songDir =
    name:
    builtins.path {
      path = songbook + "/${name}";
      name = "lyra-song-${name}";
    };

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
    cp -r ${songDir name} "$out_dir/${name}"
  '') shipped}

  jq . ${files.manifest} > "$out_dir/manifest.json"
  jq . ${files.registry} > "$out_dir/registry.json"
  ${lib.optionalString (builtin != null) ''
    jq . ${files.builtin} > "$out_dir/builtin.json"
  ''}

  # The offline generator, and the option doc list `lyra onboard` reads.
  mkdir -p "$out/share/lyra/nix"
  cp ${./nix/manifest.nix} "$out/share/lyra/nix/manifest.nix"
  cp ${../../lib/songbook.nix} "$out/share/lyra/nix/songbook.nix"
  cp ${../../lib/song.nix} "$out/share/lyra/nix/song.nix"
  cp -r ${nixpkgsLib} "$out/share/lyra/nix/lib"
  jq . ${aoideOptionsJson} > "$out/share/lyra/aoide-options.json"
''
