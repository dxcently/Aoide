# lib/checks.nix — the contractual coupling discipline, as flake checks.
#
# Two assertions ride as flake `checks` (see concepts/Governance and
# concepts/Notes in the wiki):
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
# Both are written so they PASS TRIVIALLY today (no facets declare owners yet)
# and become real as Wave-1 facets populate `aoide.surfaces`. Each check
# resolves to a trivial derivation: it either builds (assertion held) or the
# eval fails with a readable message (assertion broken).
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
    assertCheck "surface-ownership" (badOwner == [ ])
      "surfaces with no owner: ${builtins.toString badOwner}";

  # ── Check 2: no song/ RUNTIME read at build time ───────────────────────────
  # Asserts that no walked module path lives under a `song/` RUNTIME dir. The
  # ban is scoped to ephemeral runtime state (stage/ · auditions/ · catalog/ ·
  # index/) — `stage/` can never become load-bearing for the frozen half. It
  # deliberately does NOT list `song/songbook/`: committed songs there are
  # VERSIONED SCORE, legitimately walked at eval by lib/mkHost.nix (each song's
  # rice.nix self-gates on `aoide.song`). Walking the songbook therefore never
  # trips this check — only runtime infixes offend.
  noSongRead =
    modulePaths:
    let
      runtimeInfixes = [
        "/song/stage/"
        "/song/auditions/"
        "/song/catalog/"
        "/song/index/"
      ];
      offenders = builtins.filter (
        p: let s = toString p; in builtins.any (needle: lib.hasInfix needle s) runtimeInfixes
      ) modulePaths;
    in
    assertCheck "no-song-read" (offenders == [ ])
      "modules read song/ runtime paths at build time: ${builtins.toString offenders}";

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
  songShape =
    songbookPaths:
    let
      strays = builtins.filter (
        p: let s = toString p; in !lib.hasSuffix "/rice.nix" s
      ) songbookPaths;
    in
    assertCheck "song-shape" (strays == [ ])
      "songbook holds non-rice.nix modules (a song is rice.nix only): ${builtins.toString strays}";
in
{
  inherit
    assertCheck
    surfaceOwnership
    noSongRead
    songShape
    ;
}
