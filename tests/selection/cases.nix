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
      modules = [ mod ];
    };

  desktopHost = {
    aggregation.base.enable = true;
    aggregation.desktop.enable = true;
    aggregation.agents.enable = true;
    aggregation.aoideos.enable = true;
  };
in
rec {
  # The aoideos group resolves to its members, with the providers it names.
  realRegistryResolvesAoideos =
    let
      selection = selectionOf { aggregation.aoideos.enable = true; };
    in
    "${
      builtins.concatStringsSep "," (
        builtins.attrNames (lib.filterAttrs (_: d: d.enable) selection.dendrites)
      )
    }:${selection.dendrites.compositor.provider}";

  # Every nixos lane file the selection kept imports.
  realRegistryLanesImport =
    let
      selection = selectionOf desktopHost;
      lanes = composition.lanesFor {
        inherit (selection) catalogue;
        selected = selection.dendrites;
        lane = "nixos";
        scope = "for the system";
      };
    in
    lanes != [ ] && builtins.all (m: builtins.isAttrs m || builtins.isFunction m || builtins.isPath m) lanes;

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
