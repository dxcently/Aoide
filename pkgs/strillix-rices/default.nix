# pkgs/strillix-rices/default.nix — the shipped SCORE TEMPLATES (L-C3,
# strillix-carrier lane, task #107).
#
# `pkgs/aoide` is self-flaked, nixpkgs-only, and its own source tree does NOT
# contain `rices/` (that lives in this outer repo) — see that
# package's own `flake.nix` header, "Topology (b)". So the
# committed rices cannot ride the core package derivation; it lives here
# instead, in the outer repo, discovered by `lib/pkgs.nix` like any other
# `pkgs/<name>`.
#
# `$out/share/strillix/` holds three things, one per consumer:
#
#   rices/          the rice folders this system ships (every discovered one
#                      by default; a host's built-in set when the strillix lane
#                      overrides it) PLUS `manifest.json`/`registry.json`/
#                      `builtin.json` baked via `lib/rices.nix` — the SAME
#                      generator the strillix lane's `quickshellConfig` derivation
#                      and the `ricesManifest` flake output both call, so
#                      this dir's baked files can never drift from what the
#                      same rices evaluate to anywhere else.
#   nix/               the OFFLINE generator and its dependencies
#                      (`manifest.nix` + copies of `lib/{rices,rice}.nix` +
#                      the locked nixpkgs `lib/`) — what a machine with no
#                      Aoide checkout evaluates to regenerate the two files
#                      over its OWN rices. See `nix/manifest.nix`.
#   aoide-options.json `lib/options.nix`'s output, so `strillix onboard` derives the
#                      `aoide.*` option set without a checkout and without nix.
#
# Consumed at runtime by `aoide_storage::fs::rice_templates_dir` /
# `STRILLIX_TEMPLATES` (rices/) and by its parent (nix/,
# aoide-options.json) — `strillix rice compose --from <rice>`,
# `strillix-rice::widgets`'s registry/manifest regeneration, and `strillix onboard`
# all read it on a repo-less host (no `$AOIDE_FLAKE_ROOT` checkout there).
#
# Only `strillix-rice` and `strillix` read any of it, so the path is named where
# strillix runs: the installed strillix (`modules/dendrites/strillix/package.nix`)
# defaults `STRILLIX_TEMPLATES` to `rices/`, and the strillix dendrite's units
# (`shellbridge`, `aoide-rice-reload`, `aoide-quickshell`) and its activation's
# `strillix rice refresh` spell the same path themselves. Core's `aoided` unit
# names none of it. On a NixOS host the installed strillix sets the variable by
# default, so the core Rust's own sibling-of-binary fallback is reached only by a
# binary run without that default; that tier exists for a future non-nix tarball
# install (`bin/strillix` + `share/strillix/rices/` shipped side by side).
#
# ── Two knobs, and they are what "built in" means ─────────────────────────
# `rices` and `builtin` default to the WHOLE committed rices and to no
# selection at all, which is what the flake's own `packages.strillix-rices`
# wants. The strillix lane overrides both, per host, with the rices that host
# builds in (`lib/rices.nix`'s `builtIn`): a `_server` that performs nothing
# ships an empty templates dir, and yomi ships sonata's folder and no other.
# This package is where "built in" stops being an attribute of the host and
# becomes bytes on disk — the baseline `aoide rice stage` falls back to, and the
# source `aoideSeedRices` seeds the machine's rices from.
{
  lib,
  runCommand,
  jq,
  pkgs,
  # The `aoide.*` option doc list (`lib/options.nix`), shipped as
  # `share/strillix/aoide-options.json` for `strillix onboard`. `callPackage` cannot
  # reach it — it is derived from the flake's `inputs` — so it arrives by name
  # from the build context that has them (`lib/pkgs.nix`'s `extra`).
  aoideOptions,
  # The rices to ship, by name; `null` = every discovered rice.
  rices ? null,
  # The host's selection as strillix records it: `{ declared, rices, packages }`.
  # `null` = none, and no `builtin.json` is written.
  builtin ? null,
  # The rices those names live in. A consumer's host may perform from its
  # own tree's rices, so this is an argument and not a path literal into
  # Aoide's own (the strillix lane passes the host's `_module.args.rices`; the
  # flake's own `packages` output keeps its default).
  ricesRoot ? ../../rices,
  # name -> rice directory, when the caller has already discovered them (the
  # merged rices's registry); null = discover `rices`. Reading `rices`
  # itself at eval would make a merged rices (a derivation) import-from-derivation.
  riceDirs ? null,
}:
let
  ricesData = import ../../lib/rices.nix {
    inherit lib ricesRoot;
    discovered = riceDirs;
  };

  # ── The offline generator (§9) ──────────────────────────────────────────
  # `share/strillix/nix/` is the rices generator, shipped so a machine with no
  # Aoide checkout can regenerate manifest.json/registry.json over its OWN
  # rices with a plain `nix-instantiate --eval` on a FILE — no flake, no
  # network, no checkout, no `--impure`. `nix/manifest.nix` is the entry
  # (`strillix-rice::widgets` names it); the two `.nix` beside it are COPIES of
  # `lib/rices.nix` and `lib/rice.nix`, and `lib/` is the locked nixpkgs
  # `lib/`. Copies, not a second implementation: one generator, shipped.
  #
  # The whole `lib/` rather than a subset of the functions used today (Q-3):
  # the generator calls 15 of them and a rice's `_widgets/` shelf is handed
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

  # Baked manifest/registry are over the rices this instance SHIPS, not over
  # every rice the repo happens to hold — a host that does not build a rice in
  # has no `<rice>/widgets/` in its deployed tree for a slot record to resolve
  # against.
  keep =
    attrs: if rices == null then attrs else lib.filterAttrs (name: _: builtins.elem name rices) attrs;

  shipped = if rices == null then ricesData.discoveredNames else rices;

  # ── What this derivation actually reads ─────────────────────────────────────
  # Each shipped rice enters as ITS OWN store path (`builtins.path` copies the
  # folder), never as a subdirectory of the whole `rices` tree. That is
  # not tidiness: a path literal naming the rices DIRECTORY makes every rice's
  # content part of this derivation — and the templates path is a string in a
  # host's session variables and activation, so a host's whole toplevel drvPath
  # moved whenever any rice changed. A host that builds one rice in reads that
  # one rice's folder, and a rice it does not build in cannot move its system.
  #
  # The manifest/registry below are the same story from the other side: they are
  # `builtins.toFile` over the SHIPPED set, so they do not carry the others
  # either. The unoverridden package (`rices = null`) is every discovered rice,
  # each with its own folder path — so the flake's own `packages` output still
  # ships the whole rices.
  riceDir =
    name:
    builtins.path {
      path = ricesData.riceDirs.${name};
      name = "strillix-rice-${name}";
    };

  files = {
    manifest = builtins.toFile "strillix-rices-manifest.json" (
      builtins.toJSON (keep ricesData.manifestAttrs)
    );
    registry = builtins.toFile "strillix-rices-registry.json" (
      builtins.toJSON (keep ricesData.registryAttrs)
    );
  }
  // lib.optionalAttrs (builtin != null) {
    builtin = builtins.toFile "strillix-rices-builtin.json" (builtins.toJSON builtin);
  };
in
runCommand "strillix-rices-templates" { nativeBuildInputs = [ jq ]; } ''
  out_dir="$out/share/strillix/rices"
  mkdir -p "$out_dir"

  ${lib.concatMapStrings (name: ''
    cp -r ${riceDir name} "$out_dir/${name}"
  '') shipped}

  jq . ${files.manifest} > "$out_dir/manifest.json"
  jq . ${files.registry} > "$out_dir/registry.json"
  ${lib.optionalString (builtin != null) ''
    jq . ${files.builtin} > "$out_dir/builtin.json"
  ''}

  # The offline generator, and the option doc list `strillix onboard` reads.
  mkdir -p "$out/share/strillix/nix"
  cp ${./nix/manifest.nix} "$out/share/strillix/nix/manifest.nix"
  cp ${../../lib/rices.nix} "$out/share/strillix/nix/rices.nix"
  cp ${../../lib/rice.nix} "$out/share/strillix/nix/rice.nix"
  cp -r ${nixpkgsLib} "$out/share/strillix/nix/lib"
  jq . ${aoideOptionsJson} > "$out/share/strillix/aoide-options.json"
''
