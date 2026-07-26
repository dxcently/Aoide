# lib/checks.nix — the contractual coupling discipline, as flake checks.
#
# Two assertions ride as flake `checks` (see concepts/Governance and
# concepts/Design-Tokens in the wiki):
#
#   1. surface-ownership — no render surface may have two owners. The
#      Quickshell facet declares `aoide.surfaces.<name>.owner`; Stylix reads
#      the same registry and disables derivation for owned surfaces. Two
#      modules claiming the same surface is a build-time error.
#
#   2. no-song-read — no module may make the nix build depend on a `song/`
#      runtime path (`stage/`, `backstage/`, `auditions/`, …). `stage/` can
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

  # ── Check 2: no song/ read at build time ──────────────────────────────────
  # Asserts that no walked module path lives under a `song/` runtime dir. This
  # is the structural guard: modules live in `modules/`, never in `song/`, so
  # the nix build cannot come to depend on ephemeral runtime state.
  noSongRead =
    modulePaths:
    let
      runtimeInfixes = [
        "/song/stage/"
        "/song/backstage/"
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
in
{
  inherit assertCheck surfaceOwnership noSongRead;
}
