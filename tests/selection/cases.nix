# Executable cases for what is Aoide's own: the rices (lib/rices.nix), the
# packages walker (lib/pkgs.nix), and Aoide's real registry composed through
# habit. Composition itself is habit's and tested there. Each attribute is one
# case, evaluated on its own by tests/selection/run.sh; negative cases are
# expected to throw with a message the runner greps for, so "clear error" is
# evidence rather than a claim.
{ lib, habit }:
let
  composition = habit.lib.composition { inherit lib; };

  # ── Aoide's real registry through habit ────────────────────────────────────
  registry = import ../../modules;

  selectionOf =
    mod:
    composition.evalSelection {
      inherit registry;
      host = {
        habit = mod;
      };
    };

  desktopHost = {
    aggregation.nucleus.enable = true;
    aggregation.desktop.enable = true;
    aggregation.agents.enable = true;
    aggregation.strillix.enable = true;
  };
in
rec {
  # The strillix group resolves to its members, with the providers it names.
  # The paint is the rice's, so the floor names the compositor and the theme.
  realRegistryResolvesStrillix =
    let
      selection = selectionOf { aggregation.strillix.enable = true; };
    in
    "${
      builtins.concatStringsSep "," (
        builtins.attrNames (lib.filterAttrs (_: d: d.enable) selection.dendrites)
      )
    }:${selection.dendrites.compositor.provider}";

  # Every dendrite file the selection kept imports as a module.
  realRegistryDendritesImport =
    let
      selection = selectionOf desktopHost;
      impls = lib.mapAttrsToList (name: d: composition.implOf selection.catalogue name d.provider) (
        lib.filterAttrs (_: d: d.enable) selection.dendrites
      );
    in
    impls != [ ]
    && builtins.all (
      i:
      let
        m = import i.path;
      in
      builtins.isAttrs m || builtins.isFunction m
    ) impls;

  # The inventory a reference-shaped host reports.
  realRegistryInventory =
    let
      selection = selectionOf desktopHost;
      inv = composition.inventoryOf {
        hostName = "fixture";
        inherit selection;
      };
    in
    builtins.concatStringsSep "," inv.aggregation;

  # ── Rices: discovery and selection (lib/rices.nix's hook) ──────────────
  # The hook over the fixture rices, composed through habit exactly as
  # `lib/strillix.nix` composes the real one: a rice is an aggregation whose body
  # habit reads only when it is selected. Discovery names rices and nothing else;
  # a selection decides which bodies are imported; an illegal one fails naming
  # what it was given. `selectionOf` evaluates the host's `habit` the way the
  # constructor's gate pass does, and `extraModulesFor` is the constructor's
  # second answer, so each case runs the real checks, not a copy of them.
  hookOver =
    dir:
    (import ../../lib/rices.nix {
      inherit lib;
      ricesRoot = dir;
    }).hook
      {
        inherit registry;
      };

  rices = import ../../lib/rices.nix {
    inherit lib;
    ricesRoot = ./rices;
  };

  riceDiscovery = builtins.concatStringsSep " " rices.discoveredNames;

  # Selects `habit` over a hook's registry and returns the resolved selection.
  selectedOver =
    h: habit:
    composition.evalSelection {
      inherit (h) registry selectionModules;
      host = {
        inherit habit;
      };
    };

  # The landmine is discovered and NEVER imported: selecting `alpha` names one
  # body, and the landmine's file is not read.
  riceLandmineUnread =
    let
      h = hookOver ./rices;
      sel = selectedOver h {
        rice.declared = "alpha";
      };
    in
    "${builtins.toString (builtins.length (h.extraModulesFor sel))}:${lib.boolToString (builtins.elem "landmine" rices.discoveredNames)}";

  # …and the SAME file throws the moment it is selected. Paired with the case
  # above, this is "an unselected rice's file is never read" made executable
  # rather than asserted — root AGENTS.md house rule 7's removable-without-a-trace
  # claim, for rices.
  riceLandmineFires =
    let
      h = hookOver ./rices;
    in
    builtins.length (h.extraModulesFor (selectedOver h { rice.declared = "landmine"; }));

  riceUnknownName =
    let
      h = hookOver ./rices;
    in
    builtins.length (h.extraModulesFor (selectedOver h { rice.declared = "nope"; }));

  riceAvailableUnknown =
    let
      h = hookOver ./rices;
    in
    builtins.length (h.extraModulesFor (selectedOver h { rice.available = [ "nope2" ]; }));

  # A discovered folder with no `rice.nix` can never be selected: the manifest is
  # right to see a rice being written, a host is not allowed to perform one.
  riceNoRice =
    let
      h = hookOver ./rices;
    in
    builtins.length (h.extraModulesFor (selectedOver h { rice.declared = "noshelf"; }));

  # A second rice's aggregation enabled by hand is refused by name, not merged.
  riceStrayEnabled =
    let
      h = hookOver ./rices-borrow;
    in
    builtins.length (
      h.extraModulesFor (
        selectedOver h {
          rice.declared = "borrower";
          aggregation.lender.enable = true;
        }
      )
    );

  # A rice folder named like an aggregation is refused when the hook is read.
  riceNameClash = builtins.attrNames (hookOver ./rices-clash).registry.aggregations;

  # The positive reading: a declared rice with its home builds the one module
  # the hook owns (the `_module.args` and the facts), and no other.
  riceDeclaredBuilds =
    let
      h = hookOver ./rices;
      sel = selectedOver h {
        rice.declared = "alpha";
      };
    in
    builtins.length (h.extraModulesFor sel);

  # ── A rice names nothing outside its folder (CONTRACTS.md §5) ──────────────
  # Paired: the fixture rices the cases above use has no escaping `.nix`, and
  # the escaper fixture's `../` is named by the same scan `checks.rice-shape`
  # runs over the real rices.
  ricesEscape = import ../../lib/rices.nix {
    inherit lib;
    ricesRoot = ./rices-escape;
  };

  riceEscapeClean = builtins.concatStringsSep "," rices.escapingNixFiles;
  riceEscapeFound = builtins.concatStringsSep "," ricesEscape.escapingNixFiles;

  # ── A borrow joins the built-in set (CONTRACTS.md §5) ──────────────────────
  # `borrower` dresses every slot with `lender`'s body, so a host performing
  # `borrower` and nothing else still builds the lender in — otherwise the
  # deployed `rices/lender/slotA.qml` would not exist and the slot would render
  # nothing. Two readings: the set itself, and the property it exists for — every
  # owner a built-in rice's manifest names is itself in the set.
  ricesBorrow = import ../../lib/rices.nix {
    inherit lib;
    ricesRoot = ./rices-borrow;
  };

  riceBorrowClosure = builtins.concatStringsSep " " (
    ricesBorrow.builtIn {
      declared = "borrower";
      available = [ ];
    }
  );

  riceBorrowOwnersResolve =
    let
      built = ricesBorrow.builtIn {
        declared = "borrower";
        available = [ ];
      };
      owners = lib.unique (
        lib.concatMap (
          name:
          map (slot: ricesBorrow.riceMeta.${name}.manifest.${slot}.owner) (
            lib.attrNames ricesBorrow.riceMeta.${name}.manifest
          )
        ) built
      );
      unresolvable = lib.subtractLists built owners;
    in
    if unresolvable == [ ] then
      "all-resolve"
    else
      "unresolvable: ${builtins.concatStringsSep "," unresolvable}";

  # ── Every bar embeds the shared rice-mode control (lib/checks.nix) ─────────
  # `rices-ricemode/` holds sonata (a bar that embeds the `ricemode` slot,
  # and the floor record) and `mute` (a bar that does not). Paired so the check
  # cannot pass by never firing: it names the mute bar, it holds once that rice
  # is out of the manifest, and it names a rices with no floor. Only the
  # throwing branch needs no `pkgs`; the passing one gets a stand-in builder.
  checks = import ../../lib/checks.nix {
    inherit lib;
    pkgs.runCommand =
      name: _: _:
      "built ${name}";
  };

  ricemodeManifest =
    (import ../../lib/rices.nix {
      inherit lib;
      ricesRoot = ./rices-ricemode;
    }).manifestAttrs;

  barRicemodeOf =
    manifest:
    checks.barRicemode {
      ricesRoot = ./rices-ricemode;
      inherit manifest;
    };

  barRicemodeOmitted = barRicemodeOf ricemodeManifest;

  barRicemodeEmbedded = barRicemodeOf (removeAttrs ricemodeManifest [ "mute" ]);

  barRicemodeNoFloor = barRicemodeOf (
    removeAttrs ricemodeManifest [ "mute" ]
    // {
      sonata = removeAttrs ricemodeManifest.sonata [ "ricemode" ];
    }
  );

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
    ((walkerOverlay { stock = { }; }) { } { strillix-rices = "the lane's rices"; }).strillix-rices;

  walkerRefusesUnlistedOverride =
    ((walkerOverlay { stock = { }; }) { } { strillix-shell = "another overlay's shell"; })
    .strillix-shell;
}
