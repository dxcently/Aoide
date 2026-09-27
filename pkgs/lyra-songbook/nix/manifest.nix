# manifest.nix — the songbook generator, as SHIPPED (S9, concepts/song/
# Self-Ricing.md "the runtime reads").
#
# This file is the entry `aoide-song::widgets` evaluates with a plain
# `nix-instantiate --eval --strict --json` over a FILE:
#
#   nix-instantiate --eval --strict --json --expr \
#     'import <templates>/../nix/manifest.nix { songbook = <AOIDE_ROOT>/song/songbook; }'
#
# No flake, no network, no checkout, no `--impure`. What a repo-less machine
# regenerating its OWN songbook needs, and all it needs.
#
# It sits beside its three dependencies, all copied in by
# `pkgs/lyra-songbook/default.nix`:
#
#   songbook.nix   `lib/songbook.nix`, verbatim — THE generator
#   song.nix       `lib/song.nix`, verbatim — `composeSong`, which the shelf
#                  records reach through
#   lib/           the locked nixpkgs `lib/`, tests dropped
#
# The copies are copies: `lib/songbook.nix` is the one generator, and every
# caller — this entry, the lyra lane's `quickshellConfig` derivation, the
# `songbookManifest` flake output — evaluates the same file, so a machine
# running this cannot drift from the build that deployed it.
#
# ── What it returns ────────────────────────────────────────────────────────
#   manifest   `<song>` → `{ <slot> = { owner; file; }; }`, songs with no slot
#              omitted (the same asymmetry `songbookManifest` carries)
#   registry   `<song>` → that song's widget-TYPE registry entry, EVERY song
#              including the empty ones
#   packages   `<song>` → the package NAMES that song needs installed, its
#              borrow-closure included
#
# `packages` is what `aoide rice stage` checks a machine-songbook song against
# `builtin.json.packages` before staging it: a song whose lender is not built
# in needs the lender's packages too, so the closure — not the song's own
# record alone — is what "this song's needs" means. Same list, same closure
# `lib/songbook.nix`'s `builtIn` gives a host, so the two sides of that
# comparison are the same arithmetic.
{ songbook }:
let
  lib = import ./lib;
  songbookData = import ./songbook.nix { inherit lib songbook; };
in
{
  manifest = songbookData.manifestAttrs;
  registry = songbookData.registryAttrs;
  packages = lib.mapAttrs (
    name: _:
    songbookData.packagesFor (
      songbookData.builtIn {
        declared = name;
        available = [ ];
      }
    )
  ) songbookData.songs;
}
