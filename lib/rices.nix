# lib/rices.nix — the rices: what is discovered, what a host selects, and
# the manifest/registry generator they share.
#
# THREE callers, one file, so nothing here can drift from anything else:
#
#   - `lib/strillix.nix`, the constructor — `hook` answers the three hooks habit's
#     composition takes for rices: the registry (each rice is an aggregation),
#     the selection fields (`habit.rice.*`) and the platform modules and facts
#     the selection means. `discover` names the rices; `builtIn` and `greeter`
#     are the facts a selection carries.
#   - `pkgs/strillix-rices`'s generator (`nix/manifest.nix`), which imports a COPY
#     of this file at BUILD time to emit `manifest.json`/`registry.json` into the
#     deployed QML tree — over the host's BUILT-IN rices only.
#   - The `ricesManifest` flake output (flake.nix) wraps this so
#     `pkgs/aoide/crates/rice/src/widgets.rs` can shell out to `nix eval
#     --json` and regenerate both files WHOLE at RUNTIME, no rebuild — the
#     hot-sync half of `rice stage`. That one is over EVERY discovered rice
#     (the machine owns its rices; a machine-made rice must regenerate like
#     any other). A per-rice directory scan cannot answer "who owns this slot"
#     once a composition borrows — only `composeRice`, run here, resolves that
#     — so the Rust side asks nix rather than re-deriving.
#
# Discovery is SHALLOW and TYPED, the same shape `modules/aggregations/` uses: a
# rice is an immediate child directory of the rices, `_`-prefixed entries are
# shelved, and NOTHING here imports a rice. A name and a path; the `rice.nix` is
# imported by whoever selected it, and only then. `lib/walk.nix`, which used to
# discover the rices recursively for every host, is gone — a host imports
# what it selected, and the check that catches a stray `.nix` in a rice folder
# reads the directory directly (`strayNixFiles`).
{
  lib,
  # Defaults to the repo's own committed rices — override only for a test
  # fixture that stands up its own miniature rices + `lib/rice.nix` pair.
  ricesRoot ? ../rices,
  # name -> directory, when the rices are already discovered: the merge's
  # registry (lib/rices-merge.nix), whose entries point INTO the input books,
  # so nothing here reads `rices` (a merged directory is a derivation output,
  # and reading it at eval is import-from-derivation). Null = discover `rices`.
  discovered ? null,
}:
let
  riceLib = import ./rice.nix { inherit lib; };

  # ── Discovery ───────────────────────────────────────────────────────────────
  # §7.1: an immediate child directory, not `_`-prefixed. `learnings.md`,
  # `update-playbook.md` and any `.gitkeep` are files, never rices; a nested
  # directory is part of its rice (`design/`, `widgets/`, `_widgets/`), never a
  # sibling rice of its own.
  discover =
    dir:
    lib.mapAttrs (name: _: dir + "/${name}") (
      lib.filterAttrs (name: type: type == "directory" && !lib.hasPrefix "_" name) (builtins.readDir dir)
    );

  riceDirs = if discovered != null then discovered else discover ricesRoot;
  discoveredNames = builtins.attrNames riceDirs;

  # ── The rices: a rice with a `rice.nix` is an aggregation body ──────────────
  # The same one discovery as `rices`, named once here so the hook below and
  # the flake's `rices` export read the same map. A rice without a `rice.nix` is
  # a folder of widgets only (built in, never performed).
  riceNames = lib.filter (name: builtins.pathExists (riceDirs.${name} + "/rice.nix")) discoveredNames;
  rices = lib.genAttrs riceNames (name: riceDirs.${name} + "/rice.nix");

  # ── The two arguments every rice is handed ──────────────────────────────────
  # §7.4 constraint 1: a rice is a SELF-CONTAINED folder. Where a `rice.nix` or a
  # shelf used to reach out of itself — `../../../lib/rice.nix`, or another
  # rice's `_widgets/` — the two things it actually needed arrive as ARGUMENTS:
  #
  #   rice    the `lib/rice.nix` API (`composeRice`, `mkWidget`, …)
  #   borrow  name → that rice's `_widgets/`, rolled up with the same two args
  #
  # `borrow` is the one cross-rice idiom there is (`lib/rice.nix`'s header), and
  # it is keyed by NAME: a rice composes another rice's slots without ever naming
  # a path, so nothing in a rice folder can point outside it. It is also the one
  # place a shelf is imported — `riceMeta` below borrows a rice's own shelf
  # through it rather than opening the directory a second time.
  #
  # The two names are OFFERED, not forced: a shelf is handed exactly the
  # arguments its own signature declares (`builtins.functionArgs` ∩ the offer), so
  # a shelf that still says `{ lib }:` — every rice's did before this slice —
  # receives `lib` alone and is unaffected. A shelf's signature is its own; the
  # injection is what makes `rice` and `borrow` available, not what obliges a
  # shelf to take them.
  borrow =
    name:
    let
      dir = riceDirs.${name} or null;
      shelf = if dir == null then null else dir + "/_widgets";
      entry =
        if shelf == null || !(builtins.pathExists (shelf + "/default.nix")) then
          null
        else
          shelf + "/default.nix";
      fn = if entry == null then null else import entry;
      offered = {
        inherit lib borrow;
        rice = riceLib;
      };
      required = lib.attrNames (lib.filterAttrs (_: hasDefault: !hasDefault) (builtins.functionArgs fn));
      unoffered = lib.subtractLists (lib.attrNames offered) required;
    in
    if dir == null then
      throw "borrow: rice '${name}' is not in the rices; discovered: ${lib.concatStringsSep ", " discoveredNames}"
    else if fn == null then
      throw "borrow: rice '${name}' has no _widgets/ shelf to borrow from (no ${toString shelf}/default.nix)"
    else if !(builtins.isFunction fn) then
      throw "borrow: rice '${name}'s _widgets/default.nix is not a function; a shelf is `{ lib, rice, borrow, ... }: { <slot> = <record>; }`"
    else if unoffered != [ ] then
      throw "borrow: rice '${name}'s _widgets/default.nix wants ${lib.concatStringsSep ", " unoffered}, which a shelf is not handed; it receives ${lib.concatStringsSep ", " (lib.attrNames offered)}"
    else
      fn (lib.intersectAttrs (builtins.functionArgs fn) offered);

  isSlotFile = name: type: type == "regular" && builtins.match "[a-z0-9].*\\.qml" name != null;

  # Per rice: `_widgets/` shelf present (sonata today) — the
  # shelf is authoritative, run through `composeRice`, cross-checked against the
  # `widgets/` directory scan both directions (a stray `.qml` with no record, or
  # a record naming a file that doesn't exist). No shelf (barcarolle, cadenza,
  # nocturne today) — synthesize straight from the scan: owner is always the
  # rice itself, `file` is always "<slot>.qml"; the registry falls back to
  # `theme.json`'s `.widgets // {}`, unjudged. See
  # `modules/dendrites/strillix/runtime.nix`'s original comment (git history,
  # W3a/W3b) for the full reasoning; this file keeps only what the
  # computation itself needs.
  #
  # A shelf record's `owner` need not be `name`: a composition may borrow a
  # slot from another rice's shelf (`sonataWidgets // { bar = lenderWidgets.bar;
  # }`, the documented idiom). The existence check below is therefore
  # OWNER-RELATIVE, not composing-rice-relative: a borrowed record's `file`
  # is resolved under the OWNER's `widgets/` directory, because that is
  # where the borrowed body actually lives — the composing rice has no
  # `widgets/` directory of its own to hold it, and checking against
  # `widgetsDir` (this rice's own) would reject every borrow unconditionally,
  # which is exactly what happened before this comment existed.
  #
  # `packages` is carried through for the same reason `manifest` is: a widget
  # names the executables its QML shells out to, and the lane that installs a
  # built-in rice is the lane that has to install them. A shelf-less rice has no
  # record to declare them in, so it declares none — the scan path synthesizes a
  # manifest, and nothing else.
  riceMeta = lib.listToAttrs (
    map (
      name:
      let
        riceDir = riceDirs.${name};
        widgetsDir = riceDir + "/widgets";
        shelfDir = riceDir + "/_widgets";
        hasShelf = builtins.pathExists shelfDir;
        themeFile = riceDir + "/theme.json";

        widgetsEntries = if builtins.pathExists widgetsDir then builtins.readDir widgetsDir else { };
        scanSlots = map (n: lib.removeSuffix ".qml" n) (
          builtins.filter (n: isSlotFile n widgetsEntries.${n}) (builtins.attrNames widgetsEntries)
        );

        scanRegistry =
          if builtins.pathExists themeFile then
            (builtins.fromJSON (builtins.readFile themeFile)).widgets or { }
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
            composed = riceLib.composeRice (borrow name);
            shelfSlots = builtins.attrNames composed.manifest;

            # The slots THIS rice actually authors — the only ones the
            # directory scan (`scanSlots`, this rice's own `widgets/`) can
            # ever corroborate. A borrowed slot's body lives under its
            # owner's `widgets/`, which this rice's scan never looks at.
            ownSlots = lib.filter (slot: composed.manifest.${slot}.owner == name) shelfSlots;

            # Checked FIRST: a record naming an `owner` with no rices
            # directory would otherwise fall through to `missingOnDisk` and
            # report a missing FILE at a path that was never going to exist
            # — the typo is in the owner name, not the filename, and the
            # message should say so.
            unknownOwner = lib.filter (
              slot: !builtins.elem composed.manifest.${slot}.owner discoveredNames
            ) shelfSlots;

            missingRecord = lib.subtractLists shelfSlots scanSlots;
            missingFile = lib.subtractLists scanSlots ownSlots;

            missingOnDisk = lib.filter (
              slot:
              let
                e = composed.manifest.${slot};
              in
              !builtins.pathExists (riceDirs.${e.owner} + "/widgets/${e.file}")
            ) shelfSlots;
          in
          lib.throwIf (unknownOwner != [ ])
            (
              "strillix rices ${name}: slot(s) ${lib.concatStringsSep ", " unknownOwner}"
              + " name an `owner` rice with no directory in the rices"
            )
            (
              lib.throwIf (missingRecord != [ ])
                (
                  "strillix rices ${name}: widgets/ has slot(s) ${lib.concatStringsSep ", " missingRecord}"
                  + " with no _widgets/ record — add the record or delete the stray .qml"
                )
                (
                  lib.throwIf (missingFile != [ ])
                    (
                      "strillix rices ${name}: _widgets/ declares slot(s) ${lib.concatStringsSep ", " missingFile}"
                      + " with no matching widgets/*.qml — the shelf record points at nothing"
                    )
                    (
                      lib.throwIf (missingOnDisk != [ ])
                        (
                          "strillix rices ${name}: slot(s) ${lib.concatStringsSep ", " missingOnDisk}"
                          + " name a `file` that does not exist under their owner's widgets/"
                        )
                        {
                          inherit (composed) manifest;
                          registry = composed.layout.widgets;
                          inherit (composed) packages;
                        }
                    )
                )
            )
      )
    ) discoveredNames
  );

  # ── A rice folder holds notes, and only rice.nix is a module ────────────────
  # Every `.nix` under a rice that is neither `<rice>/rice.nix` nor part of the
  # rice's `_widgets/` shelf. Such a file would join the module merge silently
  # and could set arbitrary host options, so it is a build failure rather than a
  # convention (`checks.rice-shape`). Written with `builtins.readDir` recursion,
  # not `lib.filesystem`: the check is the replacement for a walker that is gone,
  # and a check that needed one would not survive it.
  #
  # ── One walk, two questions ─────────────────────────────────────────────────
  # Every `.nix` under one rice. Two callers want different views of that list,
  # so the differences are knobs rather than a second recursion:
  #
  #   `pruneShelf`  skip `_widgets/` (a shelf is score, not a module — the strays
  #                 check is about modules, and the shelf's own files are its own)
  #   `rootRice`    this directory IS the rice root, so `rice.nix` here is the
  #                 rice's module and not a stray — while a nested `rice.nix` is.
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
      dir = riceDirs.${name};
      pruneShelf = true;
      rootRice = true;
    }
  ) discoveredNames;

  # ── A rice folder names nothing outside itself ──────────────────────────────
  # §7.4 constraint 1's enforcement half. A `../` path literal in any `.nix`
  # under a rice is the shape that makes a rice un-shippable — the shelf or
  # `rice.nix` reaching for `lib/rice.nix` or another rice's `_widgets/`, which
  # is exactly what the injected `rice`/`borrow` replaced. Scanned by reading the
  # text, and `_widgets/` is NOT pruned here (the shelf is where those escapes
  # lived), so this is the one walk that sees every `.nix` in a rice — through
  # the knobs the walk above already has.
  escapingNixFiles = lib.concatMap (
    name:
    let
      # `rel` is `<name>/<path inside the rice>`: read it where the rice lives.
      fileOf = rel: riceDirs.${name} + lib.removePrefix name rel;
    in
    lib.filter (rel: lib.hasInfix "../" (builtins.readFile (fileOf rel))) (walkNix {
      prefix = name;
      dir = riceDirs.${name};
    })
  ) discoveredNames;

  # ── The rice hook ───────────────────────────────────────────────────────────
  # One function, three answers, one per hook the constructor takes
  # (`lib/composition.nix`): a rice IS an aggregation, so the hook is what makes
  # a `rice.nix` selectable at all.
  #
  #   registry          the caller's registry, with every discovered rice that
  #                     has a `rice.nix` added to `aggregations`, named by its
  #                     folder. Its body is the aggregation body: `system` and
  #                     `home` halves, each with its `members`, `providers` and
  #                     `module`. habit imports the body only when it is selected.
  #   selectionModules  `habit.rice.{declared, available}`, and, for the declared
  #                     rice only, the enable that selects its aggregation for the
  #                     host (`habit.aggregation.<n>.enable`). Its home half reaches
  #                     every Home Manager user with `home.enable = true` (habit
  #                     v0.5.0); a user who should not perform it opts out per
  #                     aggregation.
  #   extraModulesFor   the checks, then the `_module.args` a body reads (`rice`,
  #                     `borrow`, `rices`) and the facts the paint lanes read.
  #
  # The reads, exhaustively: `habit.rice.*` and `habit.aggregation.<rice>`. Never
  # a `rice.*` option, and never a body but the selected one. The checks are the list `extraModulesFor` returns,
  # so a bad selection throws on the spine before any module is imported.
  hook =
    { registry }:
    let
      ricesDir = ricesRoot;

      clashing = lib.intersectLists riceNames (builtins.attrNames registry.aggregations);

      selectionModule =
        { config, ... }:
        {
          options.rice = {
            declared = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "sonata";
              description = ''
                The rice this host performs. A committed rice name (a folder under
                `rices/<name>/` with a `rice.nix`); null means no rice is
                named. It becomes the platform fact `strillix.rice`, and its body is
                the one aggregation the host selects.
              '';
            };
            available = lib.mkOption {
              type = lib.types.listOf lib.types.str;
              default = [ ];
              example = [ "cadenza" ];
              description = ''
                Rices this host builds in but does not perform: staged without a
                rebuild, theme not active. Their bodies are never imported; their
                widgets and templates are shipped. The built-in set is
                `declared ∪ available`, closed under borrows.
              '';
            };
          };

          config = lib.mkIf (config.rice.declared != null) {
            aggregation = lib.genAttrs riceNames (name: {
              enable = lib.mkIf (config.rice.declared == name) true;
            });
          };
        };

      describe =
        sel:
        if sel.rice.declared == null then
          "no habit.rice.declared"
        else
          "habit.rice.declared = \"${sel.rice.declared}\"";

      check =
        sel:
        let
          inherit (sel) rice;
          selected = lib.unique (lib.optional (rice.declared != null) rice.declared ++ rice.available);
          unknown = lib.filter (n: !(builtins.elem n discoveredNames)) selected;
          noRice = lib.filter (n: !(builtins.elem n riceNames)) selected;
          enabled = lib.filter (n: sel.aggregation.${n}.enable) riceNames;
          stray = lib.filter (n: n != rice.declared) enabled;
        in
        lib.throwIf (unknown != [ ])
          (
            "rice: ${lib.concatStringsSep ", " (map (n: "'${n}'") unknown)}"
            + " is not in the rices; discovered: ${lib.concatStringsSep ", " discoveredNames}"
          )
          (
            lib.throwIf (noRice != [ ]) "rice ${lib.concatStringsSep ", " noRice} has no rice.nix" (
              lib.throwIf (stray != [ ]) (
                "rice aggregation ${lib.concatStringsSep ", " stray} is enabled, but ${describe sel}:"
                + " a host performs one rice, so select only the declared one"
              ) [ ]
            )
          );
    in
    lib.throwIf (clashing != [ ])
      (
        "rice(s) ${lib.concatStringsSep ", " clashing} share a name with an aggregation;"
        + " rename the rice folder"
      )
      {
        registry = registry // {
          aggregations = registry.aggregations // rices;
        };
        selectionModules = [ selectionModule ];
        extraModulesFor =
          sel:
          check sel
          ++ [
            {
              _module.args = {
                inherit borrow;
                rice = riceLib;
                ricesRoot = ricesDir;
                inherit riceDirs;
              };
              strillix.rice = sel.rice.declared;
              strillix.rices.builtIn = builtIn sel.rice;
              strillix.rices.greeter = greeter sel.rice;
            }
          ];
      };

  # ── The built-in set, closed under borrows ──────────────────────────────────
  # What a host performs, what it can stage live, and — for every rice in that
  # set — the rices its widget records BORROW from, to a fixpoint. A borrow is a
  # slot whose body lives in another rice's `widgets/` (CONTRACTS.md §5), so a
  # lender that is not itself built in leaves the borrower's manifest naming a
  # directory that is not on disk: the deployed `rices/<owner>/<file>` does not
  # exist, and the slot renders nothing. The closure is not a new concept — it is
  # exactly what the host writing `habit.rice.available = [ … ]` would say — and it is
  # the same set the runtime resolves against (`StagingEngine`'s `entry.owner`),
  # so the widget copy, the shipped templates, `packagesFor`, the machine
  # rices seed and `builtin.json` all agree by construction. A lender becomes
  # stageable on that host, which is intended.
  #
  # Sorted the way the rices's own discovery sorts, so neither what a case
  # prints nor what a manifest is filtered by depends on which rice was declared.
  builtIn =
    sel:
    let
      close =
        names:
        let
          lenders = lib.unique (
            lib.concatMap (
              name: map (slot: riceMeta.${name}.manifest.${slot}.owner) (lib.attrNames riceMeta.${name}.manifest)
            ) names
          );
          next = lib.unique (names ++ lenders);
        in
        if next == names then names else close next;
    in
    builtins.sort builtins.lessThan (
      close (lib.unique (lib.optional (sel.declared != null) sel.declared ++ sel.available))
    );

  # The declared rice's greeter face: its own manifest record for the
  # `greeter` slot (`{ owner, file }`, a borrow naming its lender), or null
  # when the declared rice dresses no greeter. The greeter is chosen before the
  # system is built, so this is read at eval time from the rices itself,
  # and it is a fact about the selection like `builtIn`
  # (`strillix.rices.greeter`). Only the DECLARED rice counts: the login
  # screen is part of the build, never of what a user later stages.
  greeter =
    sel: if sel.declared == null then null else riceMeta.${sel.declared}.manifest.greeter or null;

  # The package names the built-in rices' widgets declare (`lib/rice.nix`'s
  # `composeRice`), resolved by the caller against its own `pkgs`. Takes the
  # NAMES, not a selection: the lane that installs them reads the fact
  # `strillix.rices.builtIn`, not the host module.
  packagesFor = names: lib.unique (lib.concatMap (name: riceMeta.${name}.packages or [ ]) names);
in
{
  inherit
    discover
    riceDirs
    discoveredNames
    riceNames
    rices
    riceMeta
    strayNixFiles
    escapingNixFiles
    hook
    builtIn
    greeter
    packagesFor
    borrow
    ;

  # The `lib/rice.nix` API under the name a rice receives it by (the injected
  # argument is `rice`), so one site names what a rice is handed.
  rice = riceLib;

  # Only rices with at least one slot appear in manifest.json; registry.json
  # keeps EVERY committed rice, `{}` when it declares nothing — an empty
  # registration set is itself meaningful output, not an omission. This
  # asymmetry is deliberate and pre-existing (W3b) — see the callers' own
  # comments before changing it.
  manifestAttrs = lib.mapAttrs (_: m: m.manifest) (
    lib.filterAttrs (_: m: builtins.attrNames m.manifest != [ ]) riceMeta
  );
  registryAttrs = lib.mapAttrs (_: m: m.registry) riceMeta;
}
