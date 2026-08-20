# lib/song.nix — the widget-record builder.
#
# A song declares each of its widgets as ONE nix function, at
# `song/songbook/<song>/_widgets/<slot>.nix`. A sibling `default.nix` roll-up
# binds the owner and the slot name to each function; the song's `rice.nix`
# composes the result. This file is the song-blind mechanism both halves call —
# the nix analogue of `modules/facets/quickshell/qml/WidgetSlot.qml`, which
# hosts a widget without knowing which song wrote it.
#
# Two exports:
#
#   mkWidget owner slot widgetFn -> record
#   composeSong widgets          -> { arrangement, manifest, packages,
#                                     dependsOn }
#
# WHY A PLAIN FUNCTION AND NOT A MODULE FRAGMENT. A fragment is only reachable
# through the merged config, and a song's `rice.nix` self-gates on
# `aoide.song == "<name>"`. The quickshell derivation walks EVERY song in the
# songbook (see modules/facets/quickshell/default.nix's songs loop and its note
# on why the registry cannot come from `config`), so a fragment-based record
# would be invisible for every song the host is not currently performing —
# nine songs out of ten, silently. A plain function is importable by the
# derivation regardless of which song is active.
#
# `_widgets/` is deliberately underscore-prefixed: `lib/walk.nix` drops any
# path containing `/_`, so the shelf never joins the module merge and
# `checks.song-shape` — which filters walk's output — never sees it. The shelf
# is score, not a module.
#
# WHY EVERY GUARD LIVES IN `composeSong` AND NOT IN `mkWidget`. A composition
# overrides a borrowed widget with plain attrset update — `sonataWidgets //
# { bar = sonataWidgets.bar // { packages = [ … ]; }; }` — which is the
# documented mechanism and runs AFTER `mkWidget` has returned. Guards in
# `mkWidget` are therefore trivially bypassed by the intended composition idiom:
# an override can reinstate any value the guard just rejected, and nothing
# re-checks it. `composeSong` is the one choke point every record reaches
# however it was built, so it is the only place a guard is worth writing.
{ lib }:
let
  # The record's CLOSED key set. A key outside this list is a typo, and the
  # failure it causes is silent: `knid = "surface"` leaves `kind` null, so the
  # widget quietly vanishes from `arrangement.widgets` and its surface never
  # registers. Rejecting strays also keeps this layer as strict as the option
  # it feeds — `modules/nucleus/options.nix`'s `widgetType` submodule refuses
  # an unknown option today, and after the derivation reads records directly a
  # typo would otherwise never reach the module system at all.
  knownKeys = [
    "file"
    "owner"
    "slot"
    "kind"
    "namespace"
    "layer"
    "shortcut"
    "blur"
    "order"
    "packages"
    "dependsOn"
    "helpers"
  ];

  isStringList = v: builtins.isList v && lib.all builtins.isString v;
  isNullOr = pred: v: v == null || pred v;

  # Renders any value for an error message without assuming it is JSON-able —
  # `builtins.toJSON` throws on a function, which would replace the intended
  # message with a confusing one at the exact moment a reader needs the clear
  # one.
  show =
    v:
    if builtins.isString v then
      "\"${v}\""
    else if builtins.isAttrs v || builtins.isList v || builtins.isInt v || builtins.isBool v then
      builtins.toJSON v
    else
      "a ${builtins.typeOf v}";

  # A chain of eval-time guards. Each entry is `{ cond; msg; }`, where `cond`
  # TRUE means the record is BAD. `lib.throwIf` is lazy, so evaluation stops at
  # the first violation and a later guard never sees a value an earlier one was
  # meant to reject.
  #
  # Throwing at eval (not build) matches `lib/checks.nix`'s posture: a contract
  # violation should fail `nix flake check` fast and readably, naming the song
  # AND the slot so the message points at one file.
  guard =
    owner: slot: checks: val:
    if checks == [ ] then
      val
    else
      let
        c = builtins.head checks;
      in
      lib.throwIf c.cond "aoide widget ${owner}/${slot}: ${c.msg}" (
        guard owner slot (builtins.tail checks) val
      );

  # Validates ONE record, keyed by the slot name it is filed under. `name` is
  # the composition's key; `w.slot` is what the record claims. They must agree,
  # or the manifest maps a slot to another slot's body.
  checkRecord =
    name: w:
    let
      # The label every guard message is prefixed with. It must be a string
      # unconditionally: `owner` is record data like any other field, and a
      # non-string one would otherwise be coerced by the message header itself,
      # killing the report with `cannot coerce ...` before the guard that was
      # meant to name the problem ever runs.
      owner = if builtins.isString (w.owner or null) then w.owner else "<invalid owner>";
      file = w.file or null;
      kind = w.kind or null;
      strays = builtins.attrNames (builtins.removeAttrs w knownKeys);
    in
    guard owner name [
      {
        cond = !(builtins.isAttrs w);
        msg =
          "is not a widget record but a ${builtins.typeOf w}"
          + " — a roll-up must apply the widget's function (`mkWidget owner slot fn`)";
      }
      {
        cond = !(w ? owner);
        msg = "record has no `owner` — build it with `mkWidget`, never by hand";
      }
      {
        cond = (w.slot or name) != name;
        msg = "is filed under `${name}` but its record claims slot ${show w.slot}";
      }
      {
        cond = strays != [ ];
        msg =
          "unknown field${lib.optionalString (builtins.length strays > 1) "s"}"
          + " ${lib.concatMapStringsSep ", " (s: "`${s}`") strays}"
          + " (a typo here fails silently — the field is ignored and its slot may never register)";
      }
      {
        cond = file == null;
        msg = "missing required field `file`";
      }
      {
        cond = !(builtins.isString file);
        msg =
          "`file` must be a string basename, got ${show file}"
          + " (a nix path interpolates to a store path, which is never what the runtime resolves)";
      }
      {
        cond = baseNameOf file != file;
        msg = "`file` must be a basename, not a path (got \"${file}\")";
      }
      {
        # null is a fourth, deliberate value the option type does not carry: an
        # anchored-catalog slot, hosted by the facet's shell, registering
        # nothing. `options.nix`'s submodule defaults `kind` to "surface"; a
        # record's ABSENT kind means the opposite, so `composeSong` filters
        # nulls out before mapping and never leans on the option default.
        cond =
          !(builtins.elem kind [
            null
            "surface"
            "dock"
          ]);
        msg = "`kind` must be null, \"surface\" or \"dock\" (got ${show kind})";
      }
      # The five registration fields below are typed here and NOWHERE ELSE.
      # `arrangementWidgets` hands them to the derivation directly, never
      # through the module system, so `widgetType`'s own `types.enum` /
      # `types.bool` / `types.nullOr` never get a chance to reject a bad value —
      # `layer = "bottom"` or `blur = "yes"` would otherwise land verbatim in
      # the generated registry. This is the `knownKeys` argument above applied
      # to the value space instead of the key space; closing one without the
      # other leaves exactly half the hole open.
      {
        cond = !(builtins.isString (w.owner or null));
        msg = "`owner` must be a string song name, got ${show (w.owner or null)}";
      }
      {
        cond = !(isNullOr builtins.isString (w.namespace or null));
        msg = "`namespace` must be null or a string, got ${show (w.namespace or null)}";
      }
      {
        cond =
          !(builtins.elem (w.layer or "overlay") [
            "overlay"
            "top"
          ]);
        msg = "`layer` must be \"overlay\" or \"top\" (got ${show (w.layer or null)})";
      }
      {
        cond = !(isNullOr builtins.isString (w.shortcut or null));
        msg = "`shortcut` must be null or a GlobalShortcut name, got ${show (w.shortcut or null)}";
      }
      {
        cond = !(builtins.isBool (w.blur or true));
        msg = "`blur` must be a bool, got ${show (w.blur or null)}";
      }
      {
        cond = !(isNullOr builtins.isInt (w.order or null));
        msg = "`order` must be null or an int, got ${show (w.order or null)}";
      }
      {
        # NOT named `instruments`. In this tree an instrument is what the VENUE
        # sounds — `docs/Aoide-Wiki/concepts/song/Song-Vocabulary.md:57` defines
        # a venue's instruments as "which facets and dendrites are enabled", and
        # :49 lists widgets themselves among the quickshell facet's instruments.
        # A song-side field by that name would also read as the song choosing
        # them, which `CONTRACTS.md:807` forbids outright. This field is the
        # narrower, song-owned thing: the packages providing the executables a
        # widget's QML shells out to, named so a borrowing song can swap one
        # (`pavucontrol` for `pwvucontrol`) without touching the body.
        #
        # NOT named `dependencies` either, though `CONTRACTS.md:216` uses that
        # word for a dendrite's own inputs: this record already carries
        # `dependsOn` (sibling SLOTS a widget addresses), and two fields a
        # letter apart meaning different kinds of dependency is a worse trap
        # than the collision this rename just closed.
        cond = !(isStringList (w.packages or [ ]));
        msg = "`packages` must be a list of package names, got ${show (w.packages or null)}";
      }
      {
        cond = !(isStringList (w.dependsOn or [ ]));
        msg = "`dependsOn` must be a list of slot names, got ${show (w.dependsOn or null)}";
      }
      {
        cond = !(isStringList (w.helpers or [ ]));
        msg = "`helpers` must be a list of QML filenames, got ${show (w.helpers or null)}";
      }
    ] w;
in
{
  # ── mkWidget ───────────────────────────────────────────────────────────────
  # A binder, not a validator (see the header note). `owner` and `slot` are
  # bound LAST, after the widget's own function has run, so no widget file can
  # hand-type either one. `owner` is what makes the generated manifest a
  # provenance record: under a mixed composition it is the only artifact that
  # answers "which song painted this slot" without running the desktop.
  #
  # The widget function is applied to `{ }`. It takes an argument so a widget
  # is a function per the design, but nothing is threaded through it today;
  # overriding a borrowed widget is attrset update at the call site, which is
  # one mechanism instead of two and is what `composeSong` re-validates.
  mkWidget =
    owner: slot: widgetFn:
    let
      produced = if builtins.isFunction widgetFn then widgetFn { } else widgetFn;
    in
    lib.throwIf (!(builtins.isAttrs produced))
      (
        "aoide widget ${owner}/${slot}: its function returned"
        + " ${builtins.typeOf produced}, not an attrset"
      )
      (
        produced
        // {
          inherit owner slot;
        }
      );

  # ── composeSong ────────────────────────────────────────────────────────────
  # Takes a RESOLVED `slot -> record` attrset — every function already applied,
  # which the roll-up does — and returns what the derivation and the song's
  # merged config each need.
  #
  # A composition is plain attrset update: `sonataWidgets // { calendar = …; }`.
  # That is also why the record's dependency fields are FLAT rather than nested
  # under one `dependencies` attrset: `//` is shallow, so a song swapping one
  # widget's packages would silently wipe its sibling dependency lists.
  composeSong =
    rawWidgets:
    let
      # Every record is validated HERE, after any composition-time override.
      widgets = lib.mapAttrs checkRecord rawWidgets;

      declared = lib.filterAttrs (_: w: (w.kind or null) != null) widgets;

      # Exactly `aoide.arrangement.widgets.<slot>`'s shape, field for field
      # against `modules/nucleus/options.nix`'s `widgetType` submodule. The
      # defaults are restated here rather than left to the option so the
      # generated value is complete at this layer too — the derivation reads
      # this attrset directly, and it never passes through the module system.
      arrangementWidgets = lib.mapAttrs (_: w: {
        kind = w.kind;
        namespace = w.namespace or null;
        layer = w.layer or "overlay";
        shortcut = w.shortcut or null;
        blur = w.blur or true;
        order = w.order or null;
      }) declared;

      # The provenance record. `owner` per slot is what retires a resolution
      # search: a consumer looks a slot up rather than walking a fallback chain.
      manifest = lib.mapAttrs (_: w: {
        inherit (w) file owner;
      }) widgets;

      packages = lib.unique (lib.concatMap (w: w.packages or [ ]) (builtins.attrValues widgets));

      dependsOn = lib.mapAttrs (_: w: w.dependsOn or [ ]) widgets;

      # ── The slot-dependency closure ────────────────────────────────────────
      # Widgets talk to each other by slot name, so a composition that borrows
      # a body without the slots it addresses is broken — and broken invisibly:
      # the missing counterpart shows up as a popout that does nothing, not as
      # an error. Refusing the composition at eval time is the only place this
      # is cheap to catch. A widget naming itself is trivially satisfied, which
      # is correct: the slot IS present.
      present = builtins.attrNames widgets;
      unmet = lib.filterAttrs (_: needs: needs != [ ]) (
        lib.mapAttrs (_: w: lib.subtractLists present (w.dependsOn or [ ])) widgets
      );
      unmetMsg = lib.concatStringsSep "; " (
        lib.mapAttrsToList (
          slot: needs: "`${slot}` (owner ${widgets.${slot}.owner}) needs ${lib.concatStringsSep ", " needs}"
        ) unmet
      );
    in
    lib.throwIf (unmet != { })
      (
        "aoide composition: slot dependency not satisfied — ${unmetMsg}."
        + " Add the slot to the composition, or drop the dependency from the widget's record."
      )
      {
        arrangement.widgets = arrangementWidgets;
        inherit
          manifest
          packages
          dependsOn
          ;
      };
}
