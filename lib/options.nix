# lib/options.nix — the aoide.* option-derivation output (P-I3, onboarding
# lane, docs/architecture/ONBOARD.md "The vars-file generator"). Feeds `lyra
# onboard`'s `aoide.nix` generator: `nix eval --json <checkout>#aoideOptions`
# returns every VISIBLE, non-internal `aoide.*` option declared across
# modules/nucleus and the dendrites the registry catalogues,
# narrowed to exactly what the generator needs to render one commented line —
# name, description, and the default already rendered as nix SOURCE TEXT
# (nixpkgs' own doc renderer does the quoting/escaping; the generator pastes
# it verbatim, never re-serializes a value itself). DERIVED from the option
# declarations via `lib.evalModules`
# + `lib.optionAttrSetToDocList` — no hand-list. The emitted JSON deliberately
# carries only {name, description, default} — no `type` field, since nothing
# downstream consumes one; a future consumer adds it then, not speculatively.
#
# Ladder rung (a) HELD (docs/architecture/ONBOARD.md "The vars-file
# generator" / P-I3 brief): a bare `lib.evalModules` over the module tree
# evaluates cleanly with no fight from `pkgs`/`config`-referencing
# option constructions (ten `default` fields reference `config` — the core
# half's `aoide.root`/`checkout`/`auditLog`, with `"/home/${config.aoide.
# user}/…"` and `${config.aoide.root}` paths, plus the `melete`/`mneme`
# dendrite path defaults — and none references `pkgs` or a store path;
# verified by grepping the raw doc-list output for `config\.`/`pkgs\.`/
# `/nix/store` before committing to this rung). The heavier
# `nixosSystem` fallback (rung (b)) was never needed. `pkgs` is the one thing
# a real `nixosSystem` supplies as a module arg automatically that a bare
# `evalModules` does not — supplied here via `specialArgs`, the flake's own
# `nixpkgs.legacyPackages.<system>` (same source `checks`/`packages` already
# use), per the brief's "try the standard `pkgs = import nixpkgs {…}` first"
# guidance.
#
# `_module.check = false`: modules under `modules/` set plenty of config
# OUTSIDE the `aoide.*` tree (`habit.home`, `systemd.*`, …) that a
# real host declares via the NixOS/home-manager module sets this bare eval
# never loads. Those thunks are never forced (this file only reads
# `.options.aoide`, never `.config`), but disabling the check is the
# defensive, documented reason nothing here depends on loading those module
# sets just to read option DECLARATIONS.
{
  lib,
  pkgs,
  aoideInputs,
}:
let
  registry = import ../modules;

  # A dendrite file is a plain module (CONTRACTS.md §2), so evaluating the
  # catalogue is evaluating its files as they are: the option declarations and
  # the guard are the module's own, and `habit.home` is a key no option here
  # declares, which `_module.check = false` below leaves unread, as it does the
  # rest of what a module sets outside `aoide.*`.
  #
  # A capability with alternatives is a provider registry instead
  # (`{ providers.<p> = <path>; }`, e.g. `compositor`), and each provider is a
  # plain module of its own; the doc list needs every alternative's options, so
  # every provider file is read. A catalogue entry that is a module and not a
  # registry is told by the same test habit's `implOf` applies: an attrset
  # with `providers`.
  modulesOf =
    path:
    let
      entry = import path;
    in
    if lib.isAttrs entry && entry ? providers then lib.attrValues entry.providers else [ path ];

  dendrites = lib.concatMap modulesOf (lib.attrValues registry.catalogue);

  evaled = lib.evalModules {
    modules = dendrites ++ [
      ../modules/nucleus
      # The core's own module, straight from this flake's input — the same
      # value `lib/aoideos.nix`'s nucleus module imports into a host. It comes
      # from HERE rather than through `../modules/nucleus` because the
      # `aoide.*` option contract is part of the doc list this file renders,
      # and a bare `evalModules` has no constructor to close over the inputs for it.
      aoideInputs.aoide.nixosModules.default
      { config._module.check = false; }
    ];
    specialArgs = {
      inherit aoideInputs pkgs;
      inherit (pkgs) system;
      username = "khoa";
      host = "aoide-options-eval";
    };
  };

  docList = lib.optionAttrSetToDocList evaled.options;

  aoideOptions = builtins.filter (
    o: o.visible == true && o.internal == false && lib.hasPrefix "aoide." o.name
  ) docList;
in
map (o: {
  inherit (o) name description;
  default = o.default.text or null;
}) aoideOptions
