# lib/songbook.nix — the songbook manifest/registry generator, factored out
# of `modules/facets/quickshell/default.nix` (W3a/W3b) so it exists in
# exactly ONE place (C4/W3's "one generator" requirement). Two callers:
#
#   - `modules/facets/quickshell/default.nix`'s `quickshellConfig` derivation
#     imports this at BUILD time to emit `manifest.json`/`registry.json` into
#     the deployed QML tree.
#   - The `songbookManifest` flake output (flake.nix) wraps this so
#     `pkgs/aoide/crates/song/src/widgets.rs` can shell out to `nix eval
#     --json` and regenerate both files WHOLE at RUNTIME, no rebuild — the
#     hot-sync half of `rice stage`. A per-song directory scan cannot answer
#     "who owns this slot" once a composition borrows (only `composeSong`,
#     run here, resolves that), so the Rust side asks nix rather than
#     re-deriving — see widgets.rs's module doc for the full rationale.
#
# Byte-for-byte parity between the two call sites is automatic: they run the
# SAME function. There is no second implementation left to drift out of sync.
{
  lib,
  # Defaults to the repo's own committed songbook — override only for a test
  # fixture that stands up its own miniature songbook + `lib/song.nix` pair.
  songbook ? ../song/songbook,
}:
let
  songLib = import ./song.nix { inherit lib; };

  songbookNames = builtins.attrNames (
    lib.filterAttrs (_: t: t == "directory") (builtins.readDir songbook)
  );

  isSlotFile = name: type: type == "regular" && builtins.match "[a-z0-9].*\\.qml" name != null;

  # Per song: `_widgets/` shelf present (sonata today) — the shelf is
  # authoritative, run through `composeSong`, cross-checked against the
  # `widgets/` directory scan both directions (a stray `.qml` with no
  # record, or a record naming a file that doesn't exist). No shelf (fugue,
  # etude, nocturne today) — synthesize straight from the scan: owner is
  # always the song itself, `file` is always "<slot>.qml"; the registry
  # falls back to `livery.json`'s `.widgets // {}`, unjudged. See
  # `modules/facets/quickshell/default.nix`'s original comment (git history,
  # W3a/W3b) for the full reasoning; this file keeps only what the
  # computation itself needs.
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
          }
        else
          let
            composed = songLib.composeSong (import shelfDir { inherit lib; });
            shelfSlots = builtins.attrNames composed.manifest;

            missingRecord = lib.subtractLists shelfSlots scanSlots;
            missingFile = lib.subtractLists scanSlots shelfSlots;

            missingOnDisk = lib.filter (
              slot: !builtins.pathExists (widgetsDir + "/${composed.manifest.${slot}.file}")
            ) shelfSlots;
          in
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
                      + " name a `file` that does not exist under widgets/"
                    )
                    {
                      manifest = composed.manifest;
                      registry = composed.arrangement.widgets;
                    }
                )
            )
      )
    ) songbookNames
  );
in
{
  inherit songMeta;

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
