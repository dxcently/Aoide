# lib/strillix.nix — Strillix's constructor.
#
# `mkFlake` is the one entry a flake takes. Its `root` is the flake's own tree,
# and the constructor reads that tree by convention, each part optional: `hosts/`
# (a machine per directory holding a `default.nix`), `modules/default.nix` (a
# registry, merged beside Strillix's own catalogue) and `rices/` (a book, read
# beside Strillix's). What comes back is the machines the root holds, as
# `nixosConfigurations` and `inventory`. Strillix's own flake is one caller, with
# its tree as the root; a thin consumer's flake is another, with its own, and it
# names no host builder of its own.
#
# What it does NOT do: name a host. A host is DISCOVERED — every immediate child
# directory of the root's `hosts/` that holds a `default.nix`, minus the
# `_`-prefixed shelved ones — and it is the ONE site that reads `hosts/`. The
# outputs are built from that list and `knownHosts` takes the same list, so "which
# machines exist" can never come from two answers that disagree.
#
# Selection happens before this file is reached, inside `mkNixosHost`
# (habit's composition, `inputs.habit`): the host module's `habit.*` keys are read
# in an ordinary `evalModules` pass that knows nothing about NixOS, and the
# platform import list is assembled from the result. Nothing below imports a
# dendrite file a host did not select. A registry's `{ byClass = …; }` entries are
# rendered by lib/by-class.nix, so habit reads one only once it is selected.
#
# The rices are one hook, over one book or, when a root has rices of its own,
# several merged into one (lib/rices-merge.nix). `lib/rices.nix`'s `hook` answers
# habit's three rice hooks: each committed `rice.nix` is an aggregation body (the
# registry half), `habit.rice.*` is the selection (the selection-module half), and
# the checks plus the facts the paint lanes read are the platform half. Habit names
# no rice; this is the only site that wires the hook; `lib/rices.nix` is the only
# site that says what it means.
#
# A host's `nixpkgs` and `home-manager` are Strillix's own inputs: a consumer
# chooses which ones those are through its flake's `follows`.
{
  inputs,
  lib,
}:
let
  composition = inputs.habit.lib.composition { inherit lib; };
  catalogues = inputs.habit.lib.catalogues { inherit lib; };
  byClass = import ./by-class.nix { inherit lib; };

  # The committed rices this flake's hosts' rices live in. Named ONCE here,
  # as the caller's answer to `lib/rices.nix`'s own default (`ricesRoot ?
  # ../rices` — the same directory): the selection validates a host
  # against it, the hook below hands the same value to the lane that paints the
  # built-in rices, and the flake exports this same value as `ricesRoot`, so
  # a consumer performing Aoide's rices hands it to that same argument, where a
  # consumer with its own rices hands its own directory. `lib/rices.nix` itself is deliberately NOT touched for this:
  # it is COPIED into `pkgs/strillix-rices` (`share/strillix/nix/rices.nix`, the
  # shipped generator), so a single added line there moves every host's
  # templates path — and every session variable and unit `Environment` that
  # interpolates it.
  ricesRoot = ../rices;

  # The option files both levels declare, once: NixOS reaches them through modules/nucleus
  # and the core module, Home Manager through homeNucleus. The forward reads this list too.
  levelNeutral = [
    ../modules/nucleus/options.nix
    ../pkgs/aoide/module/options.nix
  ];

  # The nucleus module (the option contract), as a CONSUMER takes it — and the
  # ONE place Aoide's own
  # flake inputs reach a module evaluation.
  #
  # What a module reads is the module argument `aoideInputs`, and there are
  # exactly two doors for it, because the module system asks for arguments in
  # two different moments:
  #
  #   - INSIDE `config` (every module's reads except the imports below), through
  #     `_module.args`, which is `raw`: exactly ONE module defines it, and this
  #     is that module. Both the hosts `mkFlake` builds and the flake's
  #     `nixosModules.nucleus` output are that same value.
  #   - While an `imports` LIST is being resolved, through specialArgs — and
  #     only specialArgs. An `imports` list is what the module list is built
  #     from, and a name provided by `_module.args` is read out of `config`,
  #     which is computed FROM that module list. Nixpkgs names the wall itself
  #     when it happens ("argument `x' is not externally provided, so querying
  #     `_module.args` instead, requiring `config`" → infinite recursion). So
  #     the two upstream modules a module used to name in its own `imports` are
  #     imported HERE, by value, where `inputs` is a LEXICAL value of this file
  #     and costs no argument at all: the core's module (was
  #     `modules/nucleus/options.nix`) and stylix's (was
  #     `modules/dendrites/stylix.nix`).
  #   - The home half gets a third door for the same reason: home-manager
  #     evaluates its own module system, and `extraSpecialArgs` IS its
  #     specialArgs (external, available during its own module collection), so
  #     `aoideInputs.nvf` works in an HM `imports` list the way
  #     `aoideInputs.quickshell` works in a NixOS one. `sharedModules` would put
  #     the name back in `config` and hit the wall above. `ricesRoot` takes the
  #     same door: it is the value the system half reads (`config._module.args`),
  #     the merged book of a host whose root has rices of its own, and Strillix's
  #     own book otherwise (the closed-over value, which a host without the rices
  #     hook also gets). A home lane performs the rices its system half performs,
  #     and a home module that takes `ricesRoot` as a function argument gets this
  #     value: a specialArg outranks `_module.args` for a module's arguments.
  #
  # `options ? home-manager` is asked INSIDE this module's `config`, never at
  # module-application time: a guard forced in the top-level attrset asks the
  # same question the wrong way (the error above). A host that never imports
  # home-manager's NixOS module gets no option named that from here.
  # The option contract as Home Manager reads it: the same options.nix the system
  # half imports, and the core's `aoide.*` options. Declared once per evaluation;
  # a standalone home imports it (`homeModules.nucleus`), a NixOS host gets it
  # through `sharedModules` below.
  homeNucleus = {
    key = "strillix/homeNucleus";
    imports = levelNeutral;
    # The module arguments the home half reads, closed over here: a standalone
    # home has no NixOS nucleus to set them. A NixOS host passes its own `ricesRoot`
    # through extraSpecialArgs (nucleusModule), which outranks this one for a module's
    # function argument; `aoideInputs` is the same value either way.
    config._module.args = {
      aoideInputs = inputs;
      inherit ricesRoot;
    };
  };

  # A standalone home has no system half, so it alone defines the runtime's own mark:
  # false. Plain, so a host's write is a second definition, and `strillix.runtime.nixos`
  # (readOnly, nucleus/options.nix) refuses it by name. NixOS never imports this: its
  # forward carries the system half's value instead.
  standaloneNucleus = {
    key = "strillix/standaloneNucleus";
    imports = [ homeNucleus ];
    config.strillix.runtime.nixos = false;
  };

  nucleusModule =
    { options, config, ... }:
    let
      imports = [
        ../modules/nucleus
        inputs.aoide.nixosModules.default
      ]
      ++ lib.optional (inputs ? stylix) inputs.stylix.nixosModules.stylix;

      # What the home half reads from the system config, forwarded by value: every
      # option the two level-neutral files declare (modules/nucleus/options.nix and
      # the core's module/options.nix), minus facts. A fact is never forwarded:
      # mkFact refuses a forwarded write, and each owner sets its fact at both
      # levels. Derived from the declarations, so a new option needs no line here.
      sharedFiles = map toString levelNeutral;
      sharedPaths =
        prefix: tree:
        lib.concatLists (
          lib.mapAttrsToList (
            name: node:
            let
              path = prefix ++ [ name ];
            in
            if node ? _type then
              if
                !(lib.any (f: lib.elem f sharedFiles) (node.declarations or [ ])) || (node.type.isFact or false)
              then
                [ ]
              # An attrsOf option merges per key: a forwarded mkDefault of the whole
              # value is dropped beside a home definition, so each key goes alone.
              else if (node.type.name or "") == "attrsOf" then
                map (key: path ++ [ key ]) (lib.attrNames (lib.getAttrFromPath path config))
              else
                [ path ]
            else
              sharedPaths path node
          ) tree
        );
      forwarded =
        lib.recursiveUpdate
          (lib.foldl' lib.recursiveUpdate { } (
            map (path: lib.setAttrByPath path (lib.mkDefault (lib.getAttrFromPath path config))) (
              sharedPaths [ ] (lib.getAttrs [ "aoide" "strillix" ] options)
            )
          ))
          {
            # A module argument, not an option: the Home Manager half reads it (runtime-home.nix).
            _module.args.riceDirs = config._module.args.riceDirs;
          };
    in
    {
      inherit imports;
      config = {
        _module.args.aoideInputs = inputs;
        # The NixOS system half's value for the host file (the option has no default of its own).
        strillix.hostConfig = lib.mkDefault "${config.aoide.checkout}/hosts/${config.networking.hostName}/default.nix";
        # The system half's one definition of the runtime mark: true exactly when the runtime
        # is in the host (the fact runtime.nix sets). Always defined here, so a host without
        # the dendrite still has its one value; the home half receives it by the forward.
        strillix.runtime.nixos = config.strillix.enable;
      }
      // lib.optionalAttrs (options ? home-manager) {
        home-manager.extraSpecialArgs = {
          aoideInputs = inputs;
          ricesRoot = config._module.args.ricesRoot or ricesRoot;
        };
        home-manager.sharedModules = [
          homeNucleus
          { config = forwarded; }
        ];
      };
    };

  # The constructor a flake calls. `root` is the flake's own tree, and each part
  # the constructor reads from it is optional: `hosts/` (a machine per directory
  # holding a `default.nix`), `modules/default.nix` (a registry, merged after
  # Strillix's own catalogue) and `rices/` (a book, read after Strillix's). Every
  # host is built lazily, so one host's output never forces another.
  mkFlake =
    {
      # The flake's root, a path.
      root,
      # More registry sources, merged after the root's. A source is habit's
      # `{ name; catalogue; aggregations; overrides; rules; }`, and its name is what
      # a clash is blamed on. `strillix` is always taken as a source name, and `root`
      # when the root has a `modules/default.nix`: a source with either name is refused.
      registries ? [ ],
      # Modules added to every host after the nucleus module.
      extraModules ? [ ],
      # Package overlays, applied after Strillix's base overlay.
      overlays ? [ ],
      # Module arguments every host receives.
      specialArgs ? { },
      # The platform the base overlay and the rice books' package set are built for.
      system ? "x86_64-linux",
    }:
    let
      # Strillix's own flake passes its tree as the root. That tree is the base
      # already, so it is not added a second time.
      isOwnTree = root == ../.;

      # The default host config is the Aoide checkout's path, right only for Strillix's
      # own hosts. A host from another tree has none: this throw is read only when such
      # a host leaves `strillix.hostConfig` unset, and a host's own write beats it.
      hostConfigRequired = lib.optional (!isOwnTree) {
        strillix.hostConfig = lib.mkOverride 999 (
          throw "strillix.hostConfig is not set for this host. mkFlake built it from a tree that is not Strillix's own, so it has no default. Set it to the path of this host's file in the consumer's checkout, e.g. \"/home/<user>/<repo>/hosts/<name>/default.nix\". `strillix rice declare --host` rewrites that file's habit.rice.declared line."
        );
      };

      hostsDir = root + "/hosts";
      rootRegistry = root + "/modules/default.nix";
      rootRices = root + "/rices";

      # Every immediate child directory of `hosts/` holding a `default.nix`, minus
      # the `_`-prefixed shelved ones.
      knownHosts =
        if builtins.pathExists hostsDir then
          builtins.attrNames (
            lib.filterAttrs (
              name: type:
              type == "directory"
              && !lib.hasPrefix "_" name
              && builtins.pathExists (hostsDir + "/${name}/default.nix")
            ) (builtins.readDir hostsDir)
          )
        else
          [ ];

      # Habit merges the sources, and a name two of them define is its error. A
      # root's registry file that is a function of `{ }` is called here.
      registryOf =
        file:
        let
          value = import file;
        in
        if lib.isFunction value then value { } else value;
      merged = catalogues.mergeRegistries (
        [ ({ name = "strillix"; } // import ../modules) ]
        ++ lib.optional (!isOwnTree && builtins.pathExists rootRegistry) (
          { name = "root"; } // registryOf rootRegistry
        )
        ++ registries
      );

      # A `{ byClass = …; }` entry is rendered before habit reads the catalogue.
      registry = merged // {
        catalogue = byClass.renderCatalogue merged.catalogue;
      };

      # One book is the rice hook as it has always been. Several are merged first,
      # so a rice may borrow a slot from another book (lib/rices-merge.nix).
      books = [ ricesRoot ] ++ lib.optional (!isOwnTree && builtins.pathExists rootRices) rootRices;
      hook =
        (
          if builtins.length books == 1 then
            import ./rices.nix { inherit lib ricesRoot; }
          else
            import ./rices-merge.nix {
              inherit lib books;
              pkgs = inputs.nixpkgs.legacyPackages.${system};
            }
        ).hook
          { inherit registry; };

      # The flake's own packages (`pkgs/aoide`'s outputs and the walker's
      # `pkgs/<name>` dirs) reach a host's package set through this overlay — the
      # same source `flake.nix`'s `packages` output and the `pkg-<name>` checks
      # read, so there is one build and no second list. It is a BASE, handed to the
      # constructor as an `overlays` entry rather than as an extra module, because
      # the selected modules come after it and one may replace a name it provides.
      overlay = (import ./pkgs.nix { inherit lib; }).overlay {
        stock = inputs.nixpkgs.legacyPackages.${system};
      };

      hosts = lib.genAttrs knownHosts (
        name:
        composition.mkNixosHost {
          inherit (inputs) nixpkgs;
          inherit system;

          hostName = name;
          # The hosts this root holds, for the one thing the constructor needs the
          # list for: an override record confined to unknown host names is a typo,
          # and it fails by name instead of applying nowhere.
          inherit knownHosts;

          inherit (hook) registry selectionModules extraModulesFor;
          host = hostsDir + "/${name}";
          homeManagerModule = inputs.home-manager.nixosModules.home-manager;
          overlays = [ overlay ] ++ overlays;

          # The nucleus module: the option contract every host carries, selected by no
          # one. The services are the `nucleus` aggregation's, which a host selects.
          extraModules = [ nucleusModule ] ++ hostConfigRequired ++ extraModules;

          # `system` is threaded to the modules rather than to `nixosSystem`: a host
          # module states its own platform (`nixpkgs.hostPlatform`), because the
          # constructor assembles the module list and lets the platform come from
          # the modules it assembled. `host` rides along, and so does the caller's
          # `specialArgs` (a flake's `username`, say), because modules and test
          # fixtures have always had their arguments — and `inputs` does NOT: every
          # module reads Aoide's own inputs as `aoideInputs` (`nucleusModule` above),
          # so threading them through specialArgs would be a second, drifting answer
          # to a question already answered once.
          inherit specialArgs;
        }
      );
    in
    {
      nixosConfigurations = lib.mapAttrs (_: h: h.system) hosts;
      inventory = lib.mapAttrs (_: h: h.inventory) hosts;
    };
in
{
  inherit ricesRoot;

  # The nucleus module as a consumer takes it — the same module value
  # `nixosModules.nucleus` exports (see the let-block above).
  inherit
    nucleusModule
    homeNucleus
    standaloneNucleus
    mkFlake
    ;
}
