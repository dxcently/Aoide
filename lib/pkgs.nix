# lib/pkgs.nix — the packages walker.
#
# Sibling to lib/walk.nix: where walk.nix hands every `.nix` under a dir to the
# module system, this hands every package dir under `../pkgs` to `callPackage`,
# so a package placed under `pkgs/<name>/` self-registers with no hand-list to
# maintain (flake `packages` output, host + vm overlays, and a `pkg-<name>`
# check all read this one source).
#
# Discovery rule (same shelving convention as walk.nix): read `../pkgs`, keep
# entries that are DIRECTORIES whose name does NOT start with `_` (the shelving
# opt-out: prefix a package dir with `_` — e.g. `_wip/` — to hide it from
# discovery without deleting it) and that contain a `default.nix`. Each kept
# `<name>` maps to `pkgs.callPackage ../pkgs/<name> { }`.
#
# Self-flaked rule: a package dir that ALSO carries its own `flake.nix` is a
# self-flaked package consumed as a flake input, not a callPackage target —
# excluded here (topology (b), AOIDE-DEV §7; see pkgs/aoide/flake.nix). The
# root flake consumes it as the `aoide` path input and re-sources the same
# derivation in `packages`, `pkg-aoide`, and the overlays, so there is still
# exactly one build. The `flake.nix` marker keeps the exclusion structural —
# no hand-list to forget when a package graduates to its own flake.
#
# Naming rule: a package name must NOT shadow an existing nixpkgs attribute.
# Because these packages are also injected via a nixpkgs overlay
# (lib/mkHost.nix, tests/vm-boot.nix), a name that collides with a stock attribute
# would silently mask it. The `overlay` collision guard `throw`s a legible
# error when a discovered name already exists in `prev` (the overlay is the
# only path with the previous attrset in scope; the flake `packages` output has
# no `prev`, so it just maps — the overlay is where the guard bites).
#
# Intentional shadows — `intentionalShadows`: a package MAY deliberately shadow
# an unrelated nixpkgs attribute (Aoide's `melete` AI harness shadows nixpkgs'
# `melete` font — the muse-named package is the one the modules mean by
# `pkgs.melete`). Listing a name here exempts it from the guard: the shadow is a
# reviewed, documented decision, not the silent accident the guard exists to
# catch. Add a name here ONLY with that intent; the default for a new package is
# to pick a non-colliding name.
#
# Special-args escape hatch: `callPackage` auto-fills standard nixpkgs args. A
# package needing a non-standard arg (e.g. another flake package, or an input)
# overrides it with an explicit `//` after the call — edit THIS file's mapper
# for that one name, e.g. in `discover`:
#
#   discover = pkgs: (lib.genAttrs packageNames …) // {
#     foo = pkgs.callPackage (pkgsDir + "/foo") { extraArg = pkgs.aoide; };
#   };
#
# Keeping the override here (not in flake.nix) preserves the single source: the
# flake output and both overlays still read the same set.
#
# Usage:
#   discovered = (import ./lib/pkgs.nix { inherit lib; }).discover pkgs;
#   # overlay form (collision guard active):
#   nixpkgs.overlays = [ (import ./lib/pkgs.nix { inherit lib; }).overlay ];
{ lib }:
let
  pkgsDir = ../pkgs;

  # Names allowed to shadow an unrelated nixpkgs attribute (see header).
  intentionalShadows = [
    "melete" # Aoide's AI harness vs nixpkgs' `melete` headline font — unrelated.
    "eidolon" # Aoide's coding harness vs nixpkgs' `eidolon` — a dead alias, throws "removed as unmaintained upstream".
  ];

  # Directory entries under ../pkgs that are packages: type == "directory",
  # name not `_`-prefixed (shelving), containing a default.nix, and NOT
  # carrying its own flake.nix (self-flaked packages are consumed as flake
  # inputs, not callPackage targets — see header).
  packageNames =
    let
      entries = builtins.readDir pkgsDir;
      isPackageDir =
        name:
        entries.${name} == "directory"
        && !(lib.hasPrefix "_" name)
        && builtins.pathExists (pkgsDir + "/${name}/default.nix")
        && !(builtins.pathExists (pkgsDir + "/${name}/flake.nix"));
    in
    builtins.filter isPackageDir (builtins.attrNames entries);

  # Map every discovered name to its callPackage. `pkgs` is the package set that
  # supplies callPackage (nixpkgs legacyPackages in the flake output, `final`
  # in the overlay).
  discover = pkgs: lib.genAttrs packageNames (name: pkgs.callPackage (pkgsDir + "/${name}") { });

  # Entries under pkgsDir that are none of the three admitted shapes above
  # (shelved `_`-dir, callPackage target, self-flaked): a half-created
  # `pkgs/<name>/` that discovery silently drops today, with nothing flagging
  # it — plus any non-directory stray. Same predicate as `isPackageDir` above,
  # inverted and without the `_`-exemption, so the rule stays written ONCE,
  # here, next to `packageNames`. Consumed by lib/checks.nix's `discovery`
  # flake check (`nix flake check`, the COMMITTED-tree half — see the
  # mechanical-integrity design in the wiki); a self-flaked dir like
  # `pkgs/aoide` (default.nix AND flake.nix) is correctly never a stray.
  strayEntries =
    let
      entries = builtins.readDir pkgsDir;
      isStray =
        name:
        entries.${name} != "directory"
        || (
          !(lib.hasPrefix "_" name)
          && !(builtins.pathExists (pkgsDir + "/${name}/default.nix"))
          && !(builtins.pathExists (pkgsDir + "/${name}/flake.nix"))
        );
    in
    builtins.filter isStray (builtins.attrNames entries);
in
{
  inherit packageNames discover strayEntries;

  # Overlay form for nixpkgs.overlays: injects every discovered package and
  # guards each name against ACCIDENTALLY masking a stock nixpkgs attribute
  # (intentional shadows are exempted — see header).
  overlay =
    final: prev:
    lib.genAttrs packageNames (
      name:
      if prev ? ${name} && !(builtins.elem name intentionalShadows) then
        throw "pkgs/${name} collides with a nixpkgs attribute — rename it (or add it to intentionalShadows in lib/pkgs.nix if the shadow is deliberate)"
      else
        final.callPackage (pkgsDir + "/${name}") { }
    );
}
