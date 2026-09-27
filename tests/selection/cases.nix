# Executable schema cases. Each attribute is one case, evaluated on its own by
# tests/selection/run.sh; negative cases are expected to throw with a message
# the runner greps for, so "clear error" is evidence rather than a claim.
{ lib }:
let
  composition = import ../../lib/composition.nix { inherit lib; };
  registry = import ./registry.nix;

  select =
    mod:
    composition.evalSelection {
      inherit registry;
      modules = [ mod ];
    };

  # Force the whole resolution: selection, inventory, and every lane module the
  # host would import. Without this a throw hiding in an unforced thunk passes.
  resolve =
    mod:
    let
      selection = select mod;
      inv = composition.inventoryOf {
        hostName = "fixture";
        inherit selection;
      };
      lanes = {
        system = composition.lanesFor {
          inherit (selection) catalogue;
          selected = selection.dendrites;
          lane = "nixos";
          scope = "for the system";
        };
        home = lib.mapAttrs (
          userName: u:
          composition.lanesFor {
            inherit (selection) catalogue;
            selected = u.dendrites;
            lane = "homeManager";
            scope = "by user '${userName}'";
          }
        ) selection.users;
      };
    in
    builtins.deepSeq (builtins.toJSON inv) (builtins.deepSeq lanes { inherit selection inv lanes; });

  # A module list's identity, element for element, short of the values it holds.
  # `==` is no instrument here: Nix counts two distinct function objects as
  # unequal, so a list carrying a lane function never compares equal to a copy
  # of itself. What is left is each element's type, and for an attrset its
  # attribute names.
  fingerprint =
    l:
    map (
      m:
      if builtins.isAttrs m then
        "${builtins.typeOf m}{${builtins.concatStringsSep "," (builtins.attrNames m)}}"
      else
        builtins.typeOf m
    ) l;

  # ── The constructor's two hooks ────────────────────────────────────────────
  # `mkNixosModules` is the platform pass as a function: the same module list
  # and specialArgs `mkNixosHost` hands `nixosSystem`, without a package set in
  # the way. Everything here drives it directly.
  mkModules =
    args:
    composition.mkNixosModules (
      {
        hostName = "fixture";
        inherit registry;
        hostModules = [ { } ];
        nucleus = { };
        homeManagerModule = { };
      }
      // args
    );

  # A field on the host record that the CONSTRUCTOR does not know about,
  # declared the way a capability that needs one supplies it. The song selection
  # is the first user of this shape, so the fixture keeps the constructor's
  # vocabulary out of it.
  declareTag = {
    options.hostRecord.tag = lib.mkOption {
      type = lib.types.str;
      default = "unset";
      description = "A host-record field declared outside the constructor.";
    };
  };

  selectionCases = rec {
    # ── A disabled implementation is never imported ────────────────────────────
    # landmine/default.nix throws on import; selecting everything around it and
    # forcing the resolution must still succeed.
    disabledIsInert = (resolve { dendrites.systemonly.enable = true; }).inv.host;

    # An enabled dendrite imports only the provider that was chosen. The
    # landmine provider sits beside dunst in the same registry.
    unselectedProviderIsInert =
      (resolve {
        dendrites.notifications = {
          enable = true;
          provider = "dunst";
        };
      }).inv.dendrites.notifications.provider;

    # aggregations/landmine/default.nix throws on import. It is discovered — it is
    # a directory with a default.nix right beside the ones that are selected — so
    # resolving a host that does not select it proves discovery never reads a body.
    unselectedAggregationIsInert =
      (resolve {
        aggregation.workstation.enable = true;
        users.alice = {
          definition = ./users/alice.nix;
          homeManager.enable = true;
          aggregation.desk.enable = true;
        };
      }).inv.host;

    # ── Provider diagnostics ───────────────────────────────────────────────────
    missingProvider = (resolve { dendrites.notifications.enable = true; }).inv.host;

    unknownProvider =
      (resolve {
        dendrites.notifications = {
          enable = true;
          provider = "nope";
        };
      }).inv.host;

    providerOnSingleImpl =
      (resolve {
        dendrites.systemonly = {
          enable = true;
          provider = "mako";
        };
      }).inv.host;

    # ── Lane diagnostics ───────────────────────────────────────────────────────
    # mako is home-only; selecting it at system scope must fail, not be skipped.
    systemScopeWantsHomeOnlyProvider =
      (resolve {
        dendrites.notifications = {
          enable = true;
          provider = "mako";
        };
      }).lanes.system;

    systemScopeWantsHomeOnlyDendrite = (resolve { dendrites.homeonly.enable = true; }).lanes.system;

    homeScopeWantsSystemOnlyDendrite =
      (resolve {
        users.alice = {
          definition = ./users/alice.nix;
          homeManager.enable = true;
          dendrites.systemonly.enable = true;
        };
      }).lanes.home;

    # ── Unknown names ──────────────────────────────────────────────────────────
    unknownDendrite = (resolve { dendrites.frobnicate.enable = true; }).inv.host;

    unknownAggregation = (resolve { aggregation.frobnicate.enable = true; }).inv.host;

    # A provider selector an aggregation does not own is a missing option, not a
    # silently ignored line. This is the case the gate step's freeform attrset
    # defers rather than swallows.
    unknownAggregationSelector =
      (resolve {
        aggregation.workstation = {
          enable = true;
          compositor.provider = "hyprland";
        };
      }).inv.host;

    # ── The host interface: provider choices under the aggregation ─────────────
    # The host names its implementation on the aggregation that owns the
    # dendrite, and needs no top-level dendrites override to do it.
    aggregationProviderSelector =
      (resolve {
        aggregation.workstation = {
          enable = true;
          notifications.provider = "herald";
        };
      }).inv.dendrites.notifications.provider;

    # A top-level selection still outranks the aggregation's mkDefault.
    hostOverridesAggregationProvider =
      (resolve {
        aggregation.workstation.enable = true;
        dendrites.notifications.provider = "herald";
      }).inv.dendrites.notifications.provider;

    # Two aggregations naming the same dendrite on the same terms merge into ONE
    # selection — they do not instantiate it twice.
    aggregationsMergeOnSharedDendrite =
      let
        r = resolve {
          aggregation.workstation.enable = true;
          aggregation.annex.enable = true;
        };
      in
      r.inv.dendrites.notifications.provider == "dunst" && builtins.length r.lanes.system == 2;

    # Two aggregations choosing different providers for it collide; import order
    # never picks a winner.
    conflictingAggregationProviders =
      (resolve {
        aggregation.workstation.enable = true;
        aggregation.kiosk.enable = true;
      }).inv.dendrites.notifications.provider;

    # false beats a default true.
    hostDisablesAggregationMember =
      (resolve {
        aggregation.workstation.enable = true;
        dendrites.systemonly.enable = false;
      }).inv.dendrites
        ? systemonly;

    # A preference that rides along reaches the platform pass as a module, not as
    # something the selection pass evaluated.
    aggregationRidesPlatformSettings =
      let
        r = resolve { aggregation.workstation.enable = true; };
      in
      (lib.evalModules {
        modules = [
          {
            options.networking.hostName = lib.mkOption {
              type = lib.types.str;
              default = "";
            };
          }
          r.selection.nixos
        ];
      }).config.networking.hostName;

    # ── A backend-specific aggregation is not dragged in by its sibling ───────
    # modules/aggregations/compositor/ holds the members only the hyprland
    # provider can run, precisely so a host that answers `compositor.provider`
    # with something else never evaluates them. `backend`'s only member throws
    # on import: selecting the provider-bearing aggregation beside it, and
    # forcing the whole resolution, must still succeed.
    backendAggregationIsInert =
      (resolve {
        users.alice = {
          definition = ./users/alice.nix;
          homeManager.enable = true;
          aggregation.desk = {
            enable = true;
            notifications.provider = "dunst";
          };
        };
      }).inv.host;

    # The landmine is real: selecting the backend aggregation does reach it.
    backendAggregationIsReachable =
      (resolve {
        users.alice = {
          definition = ./users/alice.nix;
          homeManager.enable = true;
          aggregation.backend.enable = true;
        };
      }).inv.host;

    # ── Scopes ─────────────────────────────────────────────────────────────────
    # The same aggregation one scope down: a user turning it on gets its home
    # membership, and the system scope stays empty — the two halves of one
    # aggregation reach different evaluators.
    userAggregationContributesHomeMembers =
      let
        r = resolve {
          users.alice = {
            definition = ./users/alice.nix;
            homeManager.enable = true;
            aggregation.desk.enable = true;
          };
        };
      in
      r.inv.users.alice.dendrites.notifications.provider == "dunst" && r.lanes.system == [ ];

    # A user outranks the aggregation that attached them, independently of the
    # system selection.
    userOverridesAggregationProvider =
      (resolve {
        users.alice = {
          definition = ./users/alice.nix;
          homeManager.enable = true;
          aggregation.desk = {
            enable = true;
            notifications.provider = "mako";
          };
        };
      }).inv.users.alice.dendrites.notifications.provider;

    # ── Users ──────────────────────────────────────────────────────────────────
    # Two users, same capability, different providers, each in its own scope.
    twoUserScopes =
      let
        r = resolve {
          users.alice = {
            definition = ./users/alice.nix;
            homeManager.enable = true;
            dendrites.notifications = {
              enable = true;
              provider = "mako";
            };
          };
          users.bob = {
            definition = ./users/bob.nix;
            homeManager.enable = true;
            dendrites.notifications = {
              enable = true;
              provider = "dunst";
            };
          };
        };
      in
      "${r.inv.users.alice.dendrites.notifications.provider}+${r.inv.users.bob.dendrites.notifications.provider}";

    # Selecting a capability for the system does not select it for any user.
    scopesDoNotLeak =
      let
        r = resolve {
          dendrites.notifications = {
            enable = true;
            provider = "dunst";
          };
          users.alice = {
            definition = ./users/alice.nix;
            homeManager.enable = true;
          };
        };
      in
      r.lanes.home.alice == [ ];

    # ── Home Manager absence ───────────────────────────────────────────────────
    # A home selection with the lane switched off is a configuration error.
    homeSelectionWithoutHomeManager = mkHost {
      users.alice = {
        definition = ./users/alice.nix;
        homeManager.enable = false;
        dendrites.notifications = {
          enable = true;
          provider = "mako";
        };
      };
    };

    # A host whose users all have it off resolves cleanly and attaches no home
    # lanes at all.
    homeManagerAbsent =
      let
        r = resolve {
          dendrites.systemonly.enable = true;
          users.bob = {
            definition = ./users/bob.nix;
            homeManager.enable = false;
          };
        };
      in
      !r.inv.users.bob.homeManager && r.lanes.home.bob == [ ];

    # ── Host assembly ──────────────────────────────────────────────────────────
    # Only the strandedHome guard is forced here; nixosSystem is not evaluated,
    # so these cases stay cheap.
    mkHost =
      mod:
      (composition.mkNixosHost {
        nixpkgs = {
          lib.nixosSystem = _: throw "nixosSystem must not be evaluated by a selection case";
        };
        hostName = "fixture";
        inherit registry;
        hostModules = [ mod ];
        nucleus = { };
        homeManagerModule = { };
      }).inventory.host;
  };

  # ── Override records ───────────────────────────────────────────────────────
  # A record is a capability-scoped fix. These cases drive `overridesFor`
  # directly: it is the whole of the matching contract, and the platform pass
  # only spends what it hands back.
  #
  # The fixture set is written out rather than discovered, so a case can swap in
  # one deliberately broken record; `modules/overrides/default.nix` is the real
  # discovery and tests/templates/run.sh exercises it.
  overrides = {
    allhosts = ./overrides/allhosts.nix;
    confined = ./overrides/confined.nix;
    homely = ./overrides/homely.nix;
    tripwire = ./overrides/tripwire.nix;
  };

  bad = name: { ${name} = ./badrecords + "/${name}.nix"; };

  # The constructor's registry plus one record that matches a selected target,
  # for the case that witnesses WHERE the hook's modules land: the record's
  # `nixos` half and the hook's module define the same list option, and a list
  # option's definitions merge in module-list order, so the merged order is the
  # position.
  registryWithOverrides = registry // {
    overrides = {
      allhosts = ./overrides/allhosts.nix;
    };
  };

  applyOverrides =
    {
      mod,
      host ? "alpha",
      records ? overrides,
    }:
    composition.overridesFor {
      inherit (registry) catalogue;
      overrides = records;
      knownHosts = [
        "alpha"
        "beta"
      ];
      hostName = host;
      selection = select mod;
    };

  matchedOn = args: builtins.concatStringsSep "," (applyOverrides args).matched;

  systemPair = {
    dendrites.systemonly.enable = true;
    dendrites.notifications = {
      enable = true;
      provider = "dunst";
    };
  };

  alicePicksNotifications = {
    users.alice = {
      definition = ./users/alice.nix;
      homeManager.enable = true;
      dendrites.notifications = {
        enable = true;
        provider = "mako";
      };
    };
  };
in
selectionCases
// rec {
  # A record with no `hosts` reaches every host that selected a target — and
  # nothing else: `confined` is beta-only, `homely` and `tripwire` target
  # capabilities this host did not select.
  overrideMatchesSelectedTarget = matchedOn {
    mod = {
      dendrites.systemonly.enable = true;
    };
  };

  # Same selection, the other host. The host filter admits beta, so `confined`
  # applies there; `homely` joins because notifications is selected too. The
  # order is record name, never the filesystem.
  overrideHostFilterAdmits = matchedOn {
    mod = systemPair;
    host = "beta";
  };

  # Alpha selects exactly the same things and still does not see `confined`.
  overrideHostFilterExcludes = matchedOn { mod = systemPair; };

  # `confined` names two dendrites and beta selected both. A record applies
  # once, not once per target it hit.
  overrideAppliesOnceForTwoTargets = builtins.length (
    builtins.filter (n: n == "confined")
      (applyOverrides {
        mod = systemPair;
        host = "beta";
      }).matched
  );

  # A record never selects anything. Targeting a capability nobody chose is a
  # record that does not apply, not a capability that gets installed.
  overrideNeedsSelectedTarget = builtins.length (applyOverrides { mod = { }; }).matched;

  # A matched `nixos` module reaches the platform pass as a module — the
  # constructor hands it over, it does not evaluate it.
  overrideNixosModuleApplies =
    let
      r = applyOverrides {
        mod = {
          dendrites.systemonly.enable = true;
        };
      };
    in
    builtins.head ((builtins.head r.nixos) { }).fixture.marks;

  # A capability only a USER selected still matches, and the overlay it carries
  # is host-scoped: `useGlobalPkgs` means the home lane draws from the host
  # package set, so there is no separate home one to patch.
  overrideHomeOnlySelectionIsHostScoped =
    let
      r = applyOverrides { mod = alicePicksNotifications; };
    in
    ((builtins.head r.overlays) { } { }).fixture-homely;

  # The homeManager half rides only the users whose own selection hit a target.
  # bob is on the same matched host and gets nothing.
  overrideHomeModuleTargetsSelectingUserOnly =
    let
      r = applyOverrides {
        mod = {
          users = alicePicksNotifications.users // {
            bob = {
              definition = ./users/bob.nix;
              homeManager.enable = true;
            };
          };
        };
      };
    in
    "${toString (builtins.length r.homeManager.alice)}:${toString (builtins.length r.homeManager.bob)}";

  # tripwire's overlay and nixos module both throw. Nothing here selects
  # `homeonly`, so forcing the whole result proves an unmatched record's
  # functions are never called — the metadata above them is read on every host,
  # and that is the whole of the boundary.
  overrideUnmatchedBodiesAreInert = builtins.deepSeq (applyOverrides { mod = systemPair; }) true;

  # The complement, so the case above is not passing for the wrong reason: when
  # a user does select `homeonly`, tripwire matches and its overlay really is
  # the throwing one.
  overrideMatchedBodyIsCallable =
    let
      r = applyOverrides {
        mod = {
          users.alice = {
            definition = ./users/alice.nix;
            homeManager.enable = true;
            dendrites.homeonly.enable = true;
          };
        };
      };
    in
    (builtins.head r.overlays) { } { };

  # ── Record schema diagnostics ──────────────────────────────────────────────
  # Validation is not conditional on matching: a typo fails on every host, so a
  # broken record cannot hide on the machines it would not have applied to.
  overrideUnknownField = matchedOn {
    mod = { };
    records = bad "unknownfield";
  };

  overrideNoTarget = matchedOn {
    mod = { };
    records = bad "notarget";
  };

  overrideStrayTarget = matchedOn {
    mod = { };
    records = bad "straytarget";
  };

  overrideStrayHost = matchedOn {
    mod = { };
    records = bad "strayhost";
  };

  overrideCarriesNothing = matchedOn {
    mod = { };
    records = bad "empty";
  };

  # ── The two constructor hooks ─────────────────────────────────────────────
  # A `selectionModules` module joins the host's own modules in BOTH selection
  # steps, so a field it declares is a field the host record can set — the
  # constructor never heard of it.
  selectionModuleFieldIsVisible =
    (mkModules {
      hostModules = [ { hostRecord.tag = "sonata"; } ];
      selectionModules = [ declareTag ];
    }).selection.hostRecord.tag;

  # The same host module without the hook: not an option, so the record cannot
  # carry the field. This is the complement — the hook is what declares it, and
  # a host is not silently allowed to invent one.
  selectionModuleFieldIsUnknownWithoutIt =
    (mkModules { hostModules = [ { hostRecord.tag = "sonata"; } ]; }).selection.hostRecord.tag;

  # The gate step is what decides which aggregation BODIES the select step
  # imports. A hook module that enables an aggregation is therefore observable
  # as work done in the gate pass only: the body's own MEMBERSHIP is what the
  # resolved inventory shows, and a body the gate pass never selected is never
  # imported, so a hook handed to the select pass alone contributes nothing
  # here. `notifications` and `systemonly` are exactly what
  # `aggregations/workstation` writes into the selection.
  selectionModuleDrivesTheGatePass =
    let
      r = mkModules { selectionModules = [ { aggregation.workstation.enable = true; } ]; };
    in
    "${builtins.concatStringsSep "," r.inventory.aggregation}:${builtins.concatStringsSep "," (builtins.attrNames r.inventory.dendrites)}";

  # `extraModulesFor` is handed the RESOLVED selection, and what it returns
  # lands in the platform pass — once, for the host that selected the capability
  # it asks about, and not for the one that did not.
  extraModulesForLandsOnlyWhenSelected =
    let
      marker = {
        fixtureMarker = "extraModulesFor";
      };
      wantsSystemonly = sel: lib.optional sel.dendrites.systemonly.enable marker;
      markerCount =
        args:
        builtins.length (
          builtins.filter (m: builtins.isAttrs m && m ? fixtureMarker) (mkModules args).modules
        );
      selected = markerCount {
        hostModules = [ { dendrites.systemonly.enable = true; } ];
        extraModulesFor = wantsSystemonly;
      };
      unselected = markerCount {
        hostModules = [ { } ];
        extraModulesFor = wantsSystemonly;
      };
    in
    "${toString selected}:${toString unselected}";

  # The hook's argument is the WHOLE resolved selection — `selection.catalogue`
  # included, and a catalogue path is a body the hook can `import` itself. That
  # is deliberate: narrowing it would take the resolved selectors out of a
  # caller's reach. The discipline is the caller's, so it is pinned here: an
  # unselected catalogue path imported from the hook throws in the pass that
  # hands the hook its argument, which is where a caller notices.
  extraModulesForCanReachTheCatalogue =
    (mkModules { extraModulesFor = sel: [ (import sel.catalogue.landmine) ]; }).modules;

  # Position, not just presence. The hook's modules sit with `extraModules` —
  # after the constructor's own imports, before the override records and before
  # the host's own module — so a record or the host can still outrank them. A
  # list option both the hook's module and a matched record's `nixos` half
  # define merges in module-list order (a later module's definition reads
  # first), which is what makes the merged order a witness of the position
  # rather than of the hook merely being present.
  extraModulesForKeepsItsPosition =
    let
      # The platform vocabulary the fixture lanes write into. A real caller's
      # own modules declare it; the constructor knows none of it.
      vocabulary = {
        options.fixture.marks = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [ ];
        };
        options.fixture.systemonly = lib.mkOption {
          type = lib.types.bool;
          default = false;
        };
        options.nixpkgs.overlays = lib.mkOption {
          type = lib.types.listOf lib.types.anything;
          default = [ ];
        };
      };
      hook = {
        config.fixture.marks = [ "hook" ];
      };
      r = mkModules {
        registry = registryWithOverrides;
        hostModules = [ { dendrites.systemonly.enable = true; } ];
        extraModules = [ vocabulary ];
        extraModulesFor = _: [ hook ];
      };
    in
    builtins.concatStringsSep "," (lib.evalModules { inherit (r) modules; }).config.fixture.marks;

  # With both hooks left out, the module list and the resolved inventory are the
  # ones the constructor assembled before the hooks existed — which is what
  # makes every expectation above dxflake's, unchanged. The module list is
  # compared element for element, not by count: a same-length list with a
  # different member is a change, and the fingerprint says which.
  hookDefaultsChangeNothing =
    let
      mod = {
        dendrites.systemonly.enable = true;
      };
      base = mkModules { hostModules = [ mod ]; };
      spelled = mkModules {
        hostModules = [ mod ];
        selectionModules = [ ];
        extraModulesFor = _: [ ];
      };
    in
    "${if fingerprint base.modules == fingerprint spelled.modules then "same" else "differ"}:${
      lib.boolToString (builtins.toJSON base.inventory == builtins.toJSON spelled.inventory)
    }";

  # `mkNixosHost` is `mkNixosModules` plus `nixosSystem` and nothing else: the
  # stub here hands back exactly what it was given, so the two paths can be
  # compared instead of assumed equal — the module list element for element (an
  # equal count would not notice a swap), and the specialArgs as JSON.
  mkHostPassesTheModulesThrough =
    let
      mod = {
        dendrites.systemonly.enable = true;
      };
      viaHost = composition.mkNixosHost {
        nixpkgs = {
          lib.nixosSystem = args: args;
        };
        hostName = "fixture";
        inherit registry;
        hostModules = [ mod ];
        nucleus = { };
        homeManagerModule = { };
      };
      direct = mkModules { hostModules = [ mod ]; };
    in
    "${if fingerprint viaHost.system.modules == fingerprint direct.modules then "same" else "differ"}:${
      lib.boolToString (builtins.toJSON viaHost.system.specialArgs == builtins.toJSON direct.specialArgs)
    }";

  # ── Songs: discovery and selection (lib/songbook.nix) ──────────────────────
  # The songbook the constructor validates a host record against, over a fixture
  # with the shapes the real one holds: a plain song, a song whose `rice.nix` is
  # a landmine, a folder with no `rice.nix`, a `_`-shelved folder, and a stray
  # non-`.nix` file. Discovery names songs and nothing else; a selection decides
  # which modules exist; an illegal one fails naming what it was given.
  songbook = import ../../lib/songbook.nix {
    inherit lib;
    songbook = ./songbook;
  };

  songDiscovery = builtins.concatStringsSep " " songbook.songNames;

  # The landmine is discovered and NEVER imported: the module list a selection
  # produces names one file, and it is not the landmine's.
  songLandmineUnread =
    let
      built = songbook.songModules {
        declared = "alpha";
        available = [ ];
      };
    in
    "${builtins.toString (builtins.length built)}:${lib.boolToString (builtins.elem "landmine" songbook.songNames)}:${
      lib.boolToString (builtins.all (m: lib.hasSuffix "/alpha/rice.nix" (toString m)) built)
    }";

  # …and the SAME file throws the moment it is selected. Paired with the case
  # above, this is "an unselected song's file is never read" made executable
  # rather than asserted — root AGENTS.md house rule 7's removable-without-a-trace
  # claim, for songs.
  songLandmineFires = import (
    builtins.head (
      songbook.songModules {
        declared = "landmine";
        available = [ ];
      }
    )
  );

  songUnknownName = songbook.check {
    song = {
      declared = "nope";
      available = [ ];
    };
    lyra = true;
  };

  songAvailableUnknown = songbook.check {
    song = {
      declared = null;
      available = [ "nope2" ];
    };
    lyra = true;
  };

  # A discovered folder with no `rice.nix` can never be selected: the manifest is
  # right to see a song being written, a host is not allowed to perform one.
  songNoRice = songbook.check {
    song = {
      declared = "noshelf";
      available = [ ];
    };
    lyra = true;
  };

  # A song with no performer, refused in the field the record actually set —
  # `declared` when it exists, `available` when it does not.
  songWithoutLyra = songbook.check {
    song = {
      declared = "alpha";
      available = [ ];
    };
    lyra = false;
  };

  songAvailableWithoutLyra = songbook.check {
    song = {
      declared = null;
      available = [ "alpha" ];
    };
    lyra = false;
  };

  # ── A song names nothing outside its folder (CONTRACTS.md §5) ──────────────
  # Paired: the fixture songbook the cases above use has no escaping `.nix`, and
  # the escaper fixture's `../` is named by the same scan `checks.song-shape`
  # runs over the real songbook.
  songbookEscape = import ../../lib/songbook.nix {
    inherit lib;
    songbook = ./songbook-escape;
  };

  songEscapeClean = builtins.concatStringsSep "," songbook.escapingNixFiles;
  songEscapeFound = builtins.concatStringsSep "," songbookEscape.escapingNixFiles;

  # ── A borrow joins the built-in set (CONTRACTS.md §5) ──────────────────────
  # `borrower` dresses every slot with `lender`'s body, so a host performing
  # `borrower` and nothing else still builds the lender in — otherwise the
  # deployed `songs/lender/slotA.qml` would not exist and the slot would render
  # nothing. Two readings: the set itself, and the property it exists for — every
  # owner a built-in song's manifest names is itself in the set.
  songbookBorrow = import ../../lib/songbook.nix {
    inherit lib;
    songbook = ./songbook-borrow;
  };

  songBorrowClosure = builtins.concatStringsSep " " (
    songbookBorrow.builtIn {
      declared = "borrower";
      available = [ ];
    }
  );

  songBorrowOwnersResolve =
    let
      built = songbookBorrow.builtIn {
        declared = "borrower";
        available = [ ];
      };
      owners = lib.unique (
        lib.concatMap (
          name:
          map (slot: songbookBorrow.songMeta.${name}.manifest.${slot}.owner) (
            lib.attrNames songbookBorrow.songMeta.${name}.manifest
          )
        ) built
      );
      unresolvable = lib.subtractLists built owners;
    in
    if unresolvable == [ ] then
      "all-resolve"
    else
      "unresolvable: ${builtins.concatStringsSep "," unresolvable}";

  # ── The packages walker's override rule (lib/pkgs.nix) ─────────────────────
  # The composition's other half: the base package set arrives through this
  # walker's overlay, and a lane's replacement of a walker name stands because
  # the walker steps aside — for a LISTED name only. Paired, so neither half
  # can rot alone: the listed name yields (the lane's value survives whichever
  # overlay was applied first, which is the whole point of the yield), and any
  # OTHER walker name another overlay provides is refused by name, so a
  # replacement nobody decided on cannot happen quietly.
  walkerOverlay = (import ../../lib/pkgs.nix { inherit lib; }).overlay;

  walkerYieldsListedOverride =
    ((walkerOverlay { stock = { }; }) { } { lyra-songbook = "the lane's songbook"; }).lyra-songbook;

  walkerRefusesUnlistedOverride =
    ((walkerOverlay { stock = { }; }) { } { lyra-shell = "another overlay's shell"; }).lyra-shell;
}
