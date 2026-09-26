# lib/songbook.nix — the songbook: what is discovered, what a host selects, and
# the manifest/registry generator they share.
#
# THREE callers, one file, so nothing here can drift from anything else:
#
#   - `lib/aoideos.nix`, the constructor — `discover` names the songs,
#     `selectionModule` puts `song.declared` / `song.available` on the host
#     record, and `songModules` / `builtIn` turn that selection into the
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
    lib.filterAttrs (name: type: type == "directory" && !lib.hasPrefix "_" name) (builtins.readDir dir);

  songs = discover songbook;
  songNames = builtins.attrNames songs;

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
            composed = songLib.composeSong (import shelfDir { inherit lib; });
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
  # convention (`checks.song-shape`).
  #
  # Written with `builtins.readDir` recursion, not `lib.filesystem`: the check
  # is the replacement for a walker that is gone, and a check that needed one
  # would not survive it. `_widgets/` is pruned by NAME, so a shelf's own
  # default.nix and its per-slot records are never strays.
  # `rice.nix` at the SONG ROOT is the song's module — the one `.nix` a song may
  # hold outside its `_widgets/` shelf — and the exclusion is by (depth, name),
  # not by name alone: a nested `design/rice.nix` is still a stray.
  listNix =
    {
      prefix,
      dir,
      root ? false,
    }:
    let
      entries = builtins.readDir dir;
    in
    lib.concatMap (
      entry:
      let
        path = dir + "/${entry}";
      in
      if entries.${entry} == "directory" then
        (
          if entry == "_widgets" then
            [ ]
          else
            listNix {
              prefix = "${prefix}/${entry}";
              dir = path;
            }
        )
      else
        lib.optional (lib.hasSuffix ".nix" entry && !(root && entry == "rice.nix")) "${prefix}/${entry}"
    ) (builtins.attrNames entries);

  strayNixFiles = lib.concatMap (
    name:
    listNix {
      prefix = name;
      dir = songbook + "/${name}";
      root = true;
    }
  ) songNames;

  # ── Selection ───────────────────────────────────────────────────────────────
  # `song.declared` and `song.available` are HOST-RECORD fields: whether a song's
  # `rice.nix` is imported is decided in the gate pass, before any platform
  # evaluation, and only the host record is read that early
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
  # `rice.nix`, and a song with no performer are all record typos or record
  # mistakes, and all three are cheaper to catch before a module graph exists
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
          "song.declared = \"${song.declared}\""
        else
          "song.available = ${builtins.toJSON song.available}";
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
            + " select aggregations.aoideos (or dendrites.lyra) on this host"
          ) song
        )
      );

  # The songs a host builds in: what it performs, plus what it can stage live.
  builtIn = sel: lib.unique (lib.optional (sel.declared != null) sel.declared ++ sel.available);

  # The modules a built-in song contributes: its `rice.nix`, and nothing else.
  # An unselected song's file is never imported, which is what makes a
  # discovered-but-unselected song inert (tests/selection proves it with a
  # landmine).
  songModules = sel: map (name: songbook + "/${name}/rice.nix") (builtIn sel);

  # The package names the built-in songs' widgets declare (`lib/song.nix`'s
  # `composeSong`), resolved by the caller against its own `pkgs`. Takes the
  # NAMES, not a selection: the lane that installs them reads the fact
  # `aoide.songbook.builtIn`, not the host record.
  packagesFor = names: lib.unique (lib.concatMap (name: songMeta.${name}.packages or [ ]) names);
in
{
  inherit
    discover
    songs
    songNames
    songMeta
    strayNixFiles
    selectionModule
    check
    builtIn
    songModules
    packagesFor
    ;

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
