# lib/checks.nix — the contractual coupling discipline, as flake checks.
#
# Five checks ride as flake `checks` (see concepts/Governance and
# concepts/Notes in the wiki, and the mechanical-integrity design for fmt +
# discovery specifically):
#
#   1. surface-ownership — no render surface may have two owners. The
#      Quickshell facet declares `aoide.surfaces.<name>.owner`; Stylix reads
#      the same registry and disables derivation for owned surfaces. Two
#      modules claiming the same surface is a build-time error.
#
#   2. no-song-read — no module may make the nix build depend on a `song/`
#      runtime path (`stage/`, `auditions/`, …). `stage/` can
#      never become load-bearing for the frozen half. Enforced structurally:
#      an evaluated nixos config never imports/reads those paths, so we assert
#      over the module tree's source strings.
#
#   3. song-shape — a committed song under song/songbook/<name>/ is exactly
#      one rice.nix, never a stray extra module riding along.
#
#   4. fmt — `nixfmt --check` over every `.nix` file in the flake's own
#      COMMITTED source (flake.nix's declared `formatter`, run by nothing
#      until this check existed).
#
#   5. discovery — every `pkgs/<name>/` directory is a recognised shape
#      (shelved, callPackage target, or self-flaked) per lib/pkgs.nix's own
#      discovery rule; a fourth shape is a half-created package that vanishes
#      from `packages`/`pkg-<name>` silently today.
#
# 1–3 and 5 are written so they PASS TRIVIALLY where nothing populates the
# registry they inspect yet (1) and become real as Wave-1 facets/packages
# land. Each resolves to a trivial derivation: it either builds (assertion
# held) or the eval fails with a readable message (assertion broken). 4 is a
# real `runCommand` — it has to actually run a binary, so it can only fail at
# build time, not eval time.
{ lib, pkgs }:
let
  # A check that succeeds as a buildable derivation, or throws at eval time
  # with `msg` when `cond` is false. Throwing at eval (not build) is deliberate:
  # `nix flake check` should fail fast and legibly on a contract violation.
  assertCheck =
    name: cond: msg:
    if cond then
      pkgs.runCommand "aoide-check-${name}" { } ''
        printf 'aoide check %s: ok\n' "${name}" > "$out"
      ''
    else
      throw "aoide check ${name} FAILED: ${msg}";

  # ── Check 1: surface ownership ────────────────────────────────────────────
  # Consumes the evaluated `aoide.surfaces` registry. Duplicate detection is a
  # no-op today because attrsets cannot hold duplicate keys; the real teeth
  # arrive when a facet asserts, in its own module, that it is the sole owner
  # of a surface (Wave-1 wires the mkMerge/last-wins guard). Here we assert the
  # registry is well-formed: every declared surface names a non-empty owner.
  surfaceOwnership =
    surfaces:
    let
      names = builtins.attrNames surfaces;
      badOwner = builtins.filter (n: (surfaces.${n}.owner or "") == "") names;
    in
    assertCheck "surface-ownership" (
      badOwner == [ ]
    ) "surfaces with no owner: ${builtins.toString badOwner}";

  # ── Check 2: no song/ RUNTIME read at build time ───────────────────────────
  # Asserts that no walked module path lives under a `song/` RUNTIME dir. The
  # ban is scoped to ephemeral runtime state (stage/ · auditions/ · catalog/ ·
  # index/) — `stage/` can never become load-bearing for the frozen half. It
  # deliberately does NOT list `song/songbook/` wholesale: committed songs
  # there are VERSIONED SCORE, legitimately walked at eval by lib/mkHost.nix
  # (each song's rice.nix self-gates on `aoide.song`). Walking the songbook
  # therefore never trips this check on its own — EXCEPT the `drafts/`
  # subfolder nested inside each song (`song/songbook/<name>/drafts/`,
  # `rice draft save`'s scratch tree): that one runtime dir sits INSIDE an
  # otherwise-legitimate songbook path, so a flat infix can't name it (the
  # song name varies) — matched by regex instead, scoped tightly to just the
  # `drafts/` subfolder, never the songbook entry itself.
  noSongRead =
    modulePaths:
    let
      runtimeInfixes = [
        "/song/stage/"
        "/song/auditions/"
        "/song/catalog/"
        "/song/index/"
      ];
      isSongDraft = s: builtins.match ".*/song/songbook/[^/]+/drafts/.*" s != null;
      offenders = builtins.filter (
        p:
        let
          s = toString p;
        in
        builtins.any (needle: lib.hasInfix needle s) runtimeInfixes || isSongDraft s
      ) modulePaths;
    in
    assertCheck "no-song-read" (
      offenders == [ ]
    ) "modules read song/ runtime paths at build time: ${builtins.toString offenders}";

  # ── Check 3: song shape (host-agnostic discipline) ─────────────────────────
  # A committed song under song/songbook/<name>/ carries ONLY notes: its
  # rice.nix sets aoide.notes (palette + component tiers) and — later —
  # cover/chime references inside song/. It must NEVER set host options
  # (monitors, hardware, services) or enable facets/dendrites: the VENUE (host)
  # decides its instruments, the SONG carries only the notes (CONTRACTS.md §5).
  #
  # A cheap STRUCTURAL slice of that discipline is enforced here: every walked
  # songbook path is a file named `rice.nix` (the song's module entry). This
  # catches stray `.nix` in a song folder that would silently join the module
  # merge and could set arbitrary host options.
  #
  # TODO(song-shape v1): the full "only defines aoide.notes" invariant needs
  # per-module isolated eval + option-definition diffing — disproportionate for
  # v0. Until then the invariant is a DOCUMENTED CONVENTION (CONTRACTS.md §5 /
  # docs/BUILD.md), backed by this structural rice.nix-only gate and code review.
  # Joins that convention: a song must never set `aoide.livery.override.*`
  # (CONTRACTS.md §5) — the override tier is HOST-set only.
  songShape =
    songbookPaths:
    let
      strays = builtins.filter (
        p:
        let
          s = toString p;
        in
        !lib.hasSuffix "/rice.nix" s
      ) songbookPaths;
    in
    assertCheck "song-shape" (
      strays == [ ]
    ) "songbook holds non-rice.nix modules (a song is rice.nix only): ${builtins.toString strays}";

  # ── Check 4: nixfmt --check over the committed source ──────────────────────
  # `src` is the flake's own store copy (flake.nix passes `self`), which nix
  # already git-filters — this scopes the check to the COMMITTED tree, never
  # the working tree. Uncommitted drift is `aoide soundcheck`'s job, not
  # this one's; the two are disjoint by construction (`nix flake check`
  # cannot see gitignored/uncommitted files, full stop). A real `runCommand`
  # because it has to run the `nixfmt` binary — no purely-eval way to check
  # formatting.
  fmt =
    src:
    pkgs.runCommand "aoide-check-fmt" { nativeBuildInputs = [ pkgs.nixfmt ]; } ''
      set -euo pipefail
      cd ${src}
      nixfmt --check $(find . -name '*.nix' -type f)
      touch "$out"
    '';

  # ── Check 5: package discovery completeness ─────────────────────────────────
  # `strays` is lib/pkgs.nix's own `strayEntries` — every `pkgs/<name>/`
  # directory that is neither shelved (`_`-prefixed), a callPackage target
  # (carries default.nix), nor self-flaked (carries flake.nix, e.g.
  # pkgs/aoide). The predicate lives in lib/pkgs.nix, next to the discovery
  # rule it enforces; this check only asserts the list it returns is empty —
  # it invents no second copy of the rule.
  discovery =
    strays:
    assertCheck "discovery" (
      strays == [ ]
    ) "pkgs/ entries neither a package, shelved, nor self-flaked: ${builtins.toString strays}";
in
{
  inherit
    assertCheck
    surfaceOwnership
    noSongRead
    songShape
    fmt
    discovery
    ;
}
