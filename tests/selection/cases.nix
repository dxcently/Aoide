# Executable cases for what is Aoide's own: the songbook (lib/songbook.nix), the
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
    aggregation.aoideos.enable = true;
  };
in
rec {
  # The aoideos group resolves to its members, with the providers it names.
  # The paint is the song's, so the floor names the compositor and the theme.
  realRegistryResolvesAoideos =
    let
      selection = selectionOf { aggregation.aoideos.enable = true; };
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

  # ── Songs: discovery and selection (lib/songbook.nix's hook) ──────────────
  # The hook over the fixture songbook, composed through habit exactly as
  # `lib/aoideos.nix` composes the real one: a song is an aggregation whose body
  # habit reads only when it is selected. Discovery names songs and nothing else;
  # a selection decides which bodies are imported; an illegal one fails naming
  # what it was given. `selectionOf` evaluates the host's `habit` the way the
  # constructor's gate pass does, and `extraModulesFor` is the constructor's
  # second answer, so each case runs the real checks, not a copy of them.
  hookOver =
    dir:
    (import ../../lib/songbook.nix {
      inherit lib;
      songbook = dir;
    }).hook
      {
        inherit registry;
      };

  songbook = import ../../lib/songbook.nix {
    inherit lib;
    songbook = ./songbook;
  };

  songDiscovery = builtins.concatStringsSep " " songbook.songNames;

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
  songLandmineUnread =
    let
      h = hookOver ./songbook;
      sel = selectedOver h {
        song.declared = "alpha";
      };
    in
    "${builtins.toString (builtins.length (h.extraModulesFor sel))}:${lib.boolToString (builtins.elem "landmine" songbook.songNames)}";

  # …and the SAME file throws the moment it is selected. Paired with the case
  # above, this is "an unselected song's file is never read" made executable
  # rather than asserted — root AGENTS.md house rule 7's removable-without-a-trace
  # claim, for songs.
  songLandmineFires =
    let
      h = hookOver ./songbook;
    in
    builtins.length (h.extraModulesFor (selectedOver h { song.declared = "landmine"; }));

  songUnknownName =
    let
      h = hookOver ./songbook;
    in
    builtins.length (h.extraModulesFor (selectedOver h { song.declared = "nope"; }));

  songAvailableUnknown =
    let
      h = hookOver ./songbook;
    in
    builtins.length (h.extraModulesFor (selectedOver h { song.available = [ "nope2" ]; }));

  # A discovered folder with no `rice.nix` can never be selected: the manifest is
  # right to see a song being written, a host is not allowed to perform one.
  songNoRice =
    let
      h = hookOver ./songbook;
    in
    builtins.length (h.extraModulesFor (selectedOver h { song.declared = "noshelf"; }));

  # A second song's aggregation enabled by hand is refused by name, not merged.
  songStrayEnabled =
    let
      h = hookOver ./songbook-borrow;
    in
    builtins.length (
      h.extraModulesFor (
        selectedOver h {
          song.declared = "borrower";
          aggregation.lender.enable = true;
        }
      )
    );

  # A song folder named like an aggregation is refused when the hook is read.
  songNameClash = builtins.attrNames (hookOver ./songbook-clash).registry.aggregations;

  # The positive reading: a declared song with its home builds the one module
  # the hook owns (the `_module.args` and the facts), and no other.
  songDeclaredBuilds =
    let
      h = hookOver ./songbook;
      sel = selectedOver h {
        song.declared = "alpha";
      };
    in
    builtins.length (h.extraModulesFor sel);

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

  # ── Every bar embeds the shared rice-mode control (lib/checks.nix) ─────────
  # `songbook-ricemode/` holds sonata (a bar that embeds the `ricemode` slot,
  # and the floor record) and `mute` (a bar that does not). Paired so the check
  # cannot pass by never firing: it names the mute bar, it holds once that song
  # is out of the manifest, and it names a songbook with no floor. Only the
  # throwing branch needs no `pkgs`; the passing one gets a stand-in builder.
  checks = import ../../lib/checks.nix {
    inherit lib;
    pkgs.runCommand =
      name: _: _:
      "built ${name}";
  };

  ricemodeManifest =
    (import ../../lib/songbook.nix {
      inherit lib;
      songbook = ./songbook-ricemode;
    }).manifestAttrs;

  barRicemodeOf =
    manifest:
    checks.barRicemode {
      songbook = ./songbook-ricemode;
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
    ((walkerOverlay { stock = { }; }) { } { lyra-songbook = "the lane's songbook"; }).lyra-songbook;

  walkerRefusesUnlistedOverride =
    ((walkerOverlay { stock = { }; }) { } { lyra-shell = "another overlay's shell"; }).lyra-shell;
}
