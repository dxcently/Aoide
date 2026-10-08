# lib/songbook.nix — the songbook: what is discovered, what a host selects, and
# the manifest/registry generator they share.
#
# THREE callers, one file, so nothing here can drift from anything else:
#
#   - `lib/aoideos.nix`, the constructor — `discover` names the songs,
#     `selectionModule` puts `habit.song.declared` / `habit.song.available`
#     on the host module, and `songModules` / `builtIn` turn that selection into the
#     platform's modules and facts.
#   - `modules/dendrites/lyra/default.nix`'s `quickshellConfig` derivation
#     imports this at BUILD time to emit `manifest.json`/`registry.json` into
#     the deployed QML tree — over the host's BUILT-IN songs only.
#   - The `songbookManifest` flake output (flake.nix) wraps this so
#     `pkgs/aoide/crates/song/src/widgets.rs` can shell out to `nix eval
#     --json` and regenerate both files WHOLE at RUNTIME, no rebuild — the
#     hot-sync half of `rice stage`. That one is over EVERY discovered song
#     (the machine owns its songbook; a machine-made song must regenerate like
#     any other). A per-song directory scan cannot answer "who owns this slot"
#     once a composition borrows — only `composeSong`, run here, resolves that
#     — so the Rust side asks nix rather than re-deriving.
#
# Discovery is SHALLOW and TYPED, the same shape `modules/aggregations/` uses: a
# song is an immediate child directory of the songbook, `_`-prefixed entries are
# shelved, and NOTHING here imports a song. A name and a path; the `rice.nix` is
# imported by whoever selected it, and only then. `lib/walk.nix`, which used to
# discover the songbook recursively for every host, is gone — a host imports
# what it selected, and the check that catches a stray `.nix` in a song folder
# reads the directory directly (`strayNixFiles`).
{
  lib,
  # Defaults to the repo's own committed songbook — override only for a test
  # fixture that stands up its own miniature songbook + `lib/song.nix` pair.
  songbook ? ../song/songbook,
}:
let
  songLib = import ./song.nix { inherit lib; };

  # ── Discovery ───────────────────────────────────────────────────────────────
  # §7.1: an immediate child directory, not `_`-prefixed. `learnings.md`,
  # `update-playbook.md` and any `.gitkeep` are files, never songs; a nested
  # directory is part of its song (`design/`, `widgets/`, `_widgets/`), never a
  # sibling song of its own.
  discover =
    dir:
    lib.mapAttrs (name: _: dir + "/${name}") (
      lib.filterAttrs (name: type: type == "directory" && !lib.hasPrefix "_" name) (builtins.readDir dir)
    );

  songs = discover songbook;
  songNames = builtins.attrNames songs;

  # ── The two arguments every song is handed ──────────────────────────────────
  # §7.4 constraint 1: a song is a SELF-CONTAINED folder. Where a `rice.nix` or a
  # shelf used to reach out of itself — `../../../lib/song.nix`, or another
  # song's `_widgets/` — the two things it actually needed arrive as ARGUMENTS:
  #
  #   song    the `lib/song.nix` API (`composeSong`, `mkWidget`, …)
  #   borrow  name → that song's `_widgets/`, rolled up with the same two args
  #
  # `borrow` is the one cross-song idiom there is (`lib/song.nix`'s header), and
  # it is keyed by NAME: a song composes another song's slots without ever naming
  # a path, so nothing in a song folder can point outside it. It is also the one
  # place a shelf is imported — `songMeta` below borrows a song's own shelf
  # through it rather than opening the directory a second time.
  #
  # The two names are OFFERED, not forced: a shelf is handed exactly the
  # arguments its own signature declares (`builtins.functionArgs` ∩ the offer), so
  # a shelf that still says `{ lib }:` — every song's did before this slice —
  # receives `lib` alone and is unaffected. A shelf's signature is its own; the
  # injection is what makes `song` and `borrow` available, not what obliges a
  # shelf to take them.
  borrow =
    name:
    let
      dir = songs.${name} or null;
      shelf = if dir == null then null else dir + "/_widgets";
      entry =
        if shelf == null || !(builtins.pathExists (shelf + "/default.nix")) then
          null
        else
          shelf + "/default.nix";
      fn = if entry == null then null else import entry;
      offered = {
        inherit lib borrow;
        song = songLib;
      };
      required = lib.attrNames (lib.filterAttrs (_: hasDefault: !hasDefault) (builtins.functionArgs fn));
      unoffered = lib.subtractLists (lib.attrNames offered) required;
    in
    if dir == null then
      throw "borrow: song '${name}' is not in the songbook; discovered: ${lib.concatStringsSep ", " songNames}"
    else if fn == null then
      throw "borrow: song '${name}' has no _widgets/ shelf to borrow from (no ${toString shelf}/default.nix)"
    else if !(builtins.isFunction fn) then
      throw "borrow: song '${name}'s _widgets/default.nix is not a function; a shelf is `{ lib, song, borrow, ... }: { <slot> = <record>; }`"
    else if unoffered != [ ] then
      throw "borrow: song '${name}'s _widgets/default.nix wants ${lib.concatStringsSep ", " unoffered}, which a shelf is not handed; it receives ${lib.concatStringsSep ", " (lib.attrNames offered)}"
    else
      fn (lib.intersectAttrs (builtins.functionArgs fn) offered);

  isSlotFile = name: type: type == "regular" && builtins.match "[a-z0-9].*\\.qml" name != null;

  # Per song: `_widgets/` shelf present (sonata, fugue, quodlibet today) — the
  # shelf is authoritative, run through `composeSong`, cross-checked against the
  # `widgets/` directory scan both directions (a stray `.qml` with no record, or
  # a record naming a file that doesn't exist). No shelf (cadenza, etude,
  # nocturne today) — synthesize straight from the scan: owner is always the
  # song itself, `file` is always "<slot>.qml"; the registry falls back to
  # `livery.json`'s `.widgets // {}`, unjudged. See
  # `modules/dendrites/lyra/default.nix`'s original comment (git history,
  # W3a/W3b) for the full reasoning; this file keeps only what the
  # computation itself needs.
  #
  # A shelf record's `owner` need not be `name`: a composition may borrow a
  # slot from another song's shelf (`sonataWidgets // { bar = fugueWidgets.bar;
  # }`, the documented idiom). The existence check below is therefore
  # OWNER-RELATIVE, not composing-song-relative: a borrowed record's `file`
  # is resolved under the OWNER's `widgets/` directory, because that is
  # where the borrowed body actually lives — the composing song has no
  # `widgets/` directory of its own to hold it, and checking against
  # `widgetsDir` (this song's own) would reject every borrow unconditionally,
  # which is exactly what happened before this comment existed.
  #
  # `packages` is carried through for the same reason `manifest` is: a widget
  # names the executables its QML shells out to, and the lane that installs a
  # built-in song is the lane that has to install them. A shelf-less song has no
  # record to declare them in, so it declares none — the scan path synthesizes a
  # manifest, and nothing else.
  songMeta = lib.listToAttrs (
    map (
      name:
      let
        songDir = songbook + "/${name}";
        widgetsDir = songDir + "/widgets";
        shelfDir = songDir + "/_widgets";
        hasShelf = builtins.pathExists shelfDir;
        liveryFile = songDir + "/livery.json";

        widgetsEntries = if builtins.pathExists widgetsDir then builtins.readDir widgetsDir else { };
        scanSlots = map (n: lib.removeSuffix ".qml" n) (
          builtins.filter (n: isSlotFile n widgetsEntries.${n}) (builtins.attrNames widgetsEntries)
        );

        scanRegistry =
          if builtins.pathExists liveryFile then
            (builtins.fromJSON (builtins.readFile liveryFile)).widgets or { }
          else
            { };
      in
      lib.nameValuePair name (
        if !hasShelf then
          {
            manifest = lib.listToAttrs (
              map (
                slot:
                lib.nameValuePair slot {
                  owner = name;
                  file = "${slot}.qml";
                }
              ) scanSlots
            );
            registry = scanRegistry;
            packages = [ ];
          }
        else
          let
            composed = songLib.composeSong (borrow name);
            shelfSlots = builtins.attrNames composed.manifest;

            # The slots THIS song actually authors — the only ones the
            # directory scan (`scanSlots`, this song's own `widgets/`) can
            # ever corroborate. A borrowed slot's body lives under its
            # owner's `widgets/`, which this song's scan never looks at.
            ownSlots = lib.filter (slot: composed.manifest.${slot}.owner == name) shelfSlots;

            # Checked FIRST: a record naming an `owner` with no songbook
            # directory would otherwise fall through to `missingOnDisk` and
            # report a missing FILE at a path that was never going to exist
            # — the typo is in the owner name, not the filename, and the
            # message should say so.
            unknownOwner = lib.filter (
              slot: !builtins.elem composed.manifest.${slot}.owner songNames
            ) shelfSlots;

            missingRecord = lib.subtractLists shelfSlots scanSlots;
            missingFile = lib.subtractLists scanSlots ownSlots;

            missingOnDisk = lib.filter (
              slot:
              let
                e = composed.manifest.${slot};
              in
              !builtins.pathExists (songbook + "/${e.owner}/widgets/${e.file}")
            ) shelfSlots;
          in
          lib.throwIf (unknownOwner != [ ])
            (
              "aoide songbook ${name}: slot(s) ${lib.concatStringsSep ", " unknownOwner}"
              + " name an `owner` song with no directory in the songbook"
            )
            (
              lib.throwIf (missingRecord != [ ])
                (
                  "aoide songbook ${name}: widgets/ has slot(s) ${lib.concatStringsSep ", " missingRecord}"
                  + " with no _widgets/ record — add the record or delete the stray .qml"
                )
                (
                  lib.throwIf (missingFile != [ ])
                    (
                      "aoide songbook ${name}: _widgets/ declares slot(s) ${lib.concatStringsSep ", " missingFile}"
                      + " with no matching widgets/*.qml — the shelf record points at nothing"
                    )
                    (
                      lib.throwIf (missingOnDisk != [ ])
                        (
                          "aoide songbook ${name}: slot(s) ${lib.concatStringsSep ", " missingOnDisk}"
                          + " name a `file` that does not exist under their owner's widgets/"
                        )
                        {
                          inherit (composed) manifest;
                          registry = composed.arrangement.widgets;
                          inherit (composed) packages;
                        }
                    )
                )
            )
      )
    ) songNames
  );

  # ── A song folder holds notes, and only rice.nix is a module ────────────────
  # Every `.nix` under a song that is neither `<song>/rice.nix` nor part of the
  # song's `_widgets/` shelf. Such a file would join the module merge silently
  # and could set arbitrary host options, so it is a build failure rather than a
  # convention (`checks.song-shape`). Written with `builtins.readDir` recursion,
  # not `lib.filesystem`: the check is the replacement for a walker that is gone,
  # and a check that needed one would not survive it.
  #
  # ── One walk, two questions ─────────────────────────────────────────────────
  # Every `.nix` under one song. Two callers want different views of that list,
  # so the differences are knobs rather than a second recursion:
  #
  #   `pruneShelf`  skip `_widgets/` (a shelf is score, not a module — the strays
  #                 check is about modules, and the shelf's own files are its own)
  #   `rootRice`    this directory IS the song root, so `rice.nix` here is the
  #                 song's module and not a stray — while a nested `rice.nix` is.
  walkNix =
    {
      prefix,
      dir,
      pruneShelf ? false,
      rootRice ? false,
    }:
    let
      entries = builtins.readDir dir;
    in
    lib.concatMap (
      entry:
      let
        isDir = entries.${entry} == "directory";
      in
      if isDir && pruneShelf && entry == "_widgets" then
        [ ]
      else if isDir then
        walkNix {
          prefix = "${prefix}/${entry}";
          dir = dir + "/${entry}";
          inherit pruneShelf;
        }
      else
        lib.optional (lib.hasSuffix ".nix" entry && !(rootRice && entry == "rice.nix")) "${prefix}/${entry}"
    ) (builtins.attrNames entries);

  strayNixFiles = lib.concatMap (
    name:
    walkNix {
      prefix = name;
      dir = songs.${name};
      pruneShelf = true;
      rootRice = true;
    }
  ) songNames;

  # ── A song folder names nothing outside itself ──────────────────────────────
  # §7.4 constraint 1's enforcement half. A `../` path literal in any `.nix`
  # under a song is the shape that makes a song un-shippable — the shelf or
  # `rice.nix` reaching for `lib/song.nix` or another song's `_widgets/`, which
  # is exactly what the injected `song`/`borrow` replaced. Scanned by reading the
  # text, and `_widgets/` is NOT pruned here (the shelf is where those escapes
  # lived), so this is the one walk that sees every `.nix` in a song — through
  # the knobs the walk above already has.
  escapingNixFiles = lib.concatMap (
    name:
    lib.filter (rel: lib.hasInfix "../" (builtins.readFile (songbook + "/${rel}"))) (walkNix {
      prefix = name;
      dir = songs.${name};
    })
  ) songNames;

  # ── Selection ───────────────────────────────────────────────────────────────
  # `habit.song.declared` and `habit.song.available` are HOST fields: whether a
  # song's `rice.nix` is imported is decided in the selection pass, before any
  # platform evaluation, and only the host module's `habit.*` keys are read that early
  # (docs/architecture/NIX-COMPOSITION.md, "Selection before platform
  # evaluation"). A user-level selection could be added later, additively; there
  # is no such thing today.
  #
  # `declared` is the song the host performs — it becomes the platform fact
  # `aoide.song`. `available` is the songs this host can STAGE without a rebuild
  # but does not perform: they are built in (imported, copied, installed) and
  # their livery is not the active one.
  #
  # Both are checked HERE, in the gate pass, because a typo is cheaper to catch
  # before a module graph exists than after: an unknown name fails naming the
  # songs that DO exist, and a name that exists but has no `rice.nix` fails
  # saying so (the folder is still discovered — it may be a song being written,
  # and the manifest is right to see it).
  selectionModule = {
    options.song = {
      declared = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "sonata";
        description = ''
          The song this host performs. A committed song name (a folder under
          `song/songbook/<name>/`); null means no song is named. It becomes the
          platform fact `aoide.song`, which every `rice.nix` self-gates on.
        '';
      };
      available = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        example = [ "cadenza" ];
        description = ''
          Songs this host builds in but does not perform: staged without a
          rebuild, livery not active. The built-in set is `declared ∪
          available`.
        '';
      };
    };
  };

  # ── The gate-pass checks ────────────────────────────────────────────────────
  # Run by the constructor's hook before the platform module list is assembled,
  # which is as early as the selection exists: a bad song name, a song with no
  # `rice.nix`, and a song with no performer are all typos or mistakes in the
  # host's selection, and all three are cheaper to catch before a module graph exists
  # than after.
  #
  # A `throw`, not an assertion hung on the selection config: the gate pass is a
  # bare `evalModules` that the constructor reads attribute by attribute, so an
  # `assertions` definition there would sit unchecked.
  #
  # `lyra` is an ARGUMENT rather than a read, because this file knows the
  # songbook and not the dendrite tree: the caller passes the fact from its own
  # selection (`sel.dendrites.lyra.enable`).
  check =
    { song, lyra }:
    let
      selected = lib.unique (lib.optional (song.declared != null) song.declared ++ song.available);
      unknown = lib.filter (n: !(builtins.elem n songNames)) selected;
      noRice = lib.filter (n: !(builtins.pathExists (songbook + "/${n}/rice.nix"))) selected;
      named =
        if song.declared != null then
          "habit.song.declared = \"${song.declared}\""
        else
          "habit.song.available = ${builtins.toJSON song.available}";
    in
    lib.throwIf (unknown != [ ])
      (
        "song: ${lib.concatStringsSep ", " (map (n: "'${n}'") unknown)}"
        + " is not in the songbook; discovered: ${lib.concatStringsSep ", " songNames}"
      )
      (
        lib.throwIf (noRice != [ ]) "song ${lib.concatStringsSep ", " noRice} has no rice.nix" (
          lib.throwIf (selected != [ ] && !lyra) (
            "${named} needs the lyra dendrite:"
            + " select habit.aggregation.aoideos (or habit.dendrites.lyra) on this host"
          ) song
        )
      );

  # ── The built-in set, closed under borrows ──────────────────────────────────
  # What a host performs, what it can stage live, and — for every song in that
  # set — the songs its widget records BORROW from, to a fixpoint. A borrow is a
  # slot whose body lives in another song's `widgets/` (CONTRACTS.md §5), so a
  # lender that is not itself built in leaves the borrower's manifest naming a
  # directory that is not on disk: the deployed `songs/<owner>/<file>` does not
  # exist, and the slot renders nothing. The closure is not a new concept — it is
  # exactly what the host writing `habit.song.available = [ … ]` would say — and it is
  # the same set the runtime resolves against (`StagingEngine`'s `entry.owner`),
  # so the widget copy, the shipped templates, `packagesFor`, the machine
  # songbook seed and `builtin.json` all agree by construction. A lender becomes
  # stageable on that host, which is intended.
  #
  # Sorted the way the songbook's own discovery sorts, so neither what a case
  # prints nor what a manifest is filtered by depends on which song was declared.
  builtIn =
    sel:
    let
      close =
        names:
        let
          lenders = lib.unique (
            lib.concatMap (
              name: map (slot: songMeta.${name}.manifest.${slot}.owner) (lib.attrNames songMeta.${name}.manifest)
            ) names
          );
          next = lib.unique (names ++ lenders);
        in
        if next == names then names else close next;
    in
    builtins.sort builtins.lessThan (
      close (lib.unique (lib.optional (sel.declared != null) sel.declared ++ sel.available))
    );

  # The modules a built-in song contributes: its `rice.nix`, and nothing else.
  # An unselected song's file is never imported, which is what makes a
  # discovered-but-unselected song inert (tests/selection proves it with a
  # landmine).
  songModules = sel: map (name: songbook + "/${name}/rice.nix") (builtIn sel);

  # The package names the built-in songs' widgets declare (`lib/song.nix`'s
  # `composeSong`), resolved by the caller against its own `pkgs`. Takes the
  # NAMES, not a selection: the lane that installs them reads the fact
  # `aoide.songbook.builtIn`, not the host module.
  packagesFor = names: lib.unique (lib.concatMap (name: songMeta.${name}.packages or [ ]) names);
in
{
  inherit
    discover
    songs
    songNames
    songMeta
    strayNixFiles
    escapingNixFiles
    selectionModule
    check
    builtIn
    songModules
    packagesFor
    borrow
    ;

  # The `lib/song.nix` API under the name a song receives it by (the injected
  # argument is `song`), so one site names what a song is handed.
  song = songLib;

  # Only songs with at least one slot appear in manifest.json; registry.json
  # keeps EVERY committed song, `{}` when it declares nothing — an empty
  # registration set is itself meaningful output, not an omission. This
  # asymmetry is deliberate and pre-existing (W3b) — see the callers' own
  # comments before changing it.
  manifestAttrs = lib.mapAttrs (_: m: m.manifest) (
    lib.filterAttrs (_: m: builtins.attrNames m.manifest != [ ]) songMeta
  );
  registryAttrs = lib.mapAttrs (_: m: m.registry) songMeta;
}
