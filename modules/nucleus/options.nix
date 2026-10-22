# modules/nucleus/options.nix — THE OPTION CONTRACT.
#
# Every other module (dendrites) builds against the options
# declared here. This is versioned in CONTRACTS.md (theme schema v0). A paint
# lane reads `strillix.theme`, `strillix.layout` and `strillix.surfaces` from here,
# plus the core scalars and the rice selection declared beside them — the
# enumerated, closed set root `AGENTS.md` house rule 5 states in full; no module
# reads another module. That discipline is a documented convention, backed by
# code review — `lib/checks.nix` has no check that inspects a lane's reads, as
# rule 5 says.
#
# Applied unconditionally on every host as an extra module (no mkIf guard;
# the services that act on these options are the `nucleus` aggregation's
# members, not this file). It declares options and eval-clean defaults only;
# it wires no behaviour, so an empty config evaluates cleanly.
{
  lib,
  config,
  ...
}:
let
  inherit (lib)
    mkOption
    types
    literalExpression
    ;

  # A base16 hex colour, with or without leading '#'. Kept permissive so v0
  # theme files stay easy to author; the theme engine's `rice lint` is the
  # authoritative validator (the native theme engine, pkgs/aoide).
  hexColor = types.strMatching "#?[0-9a-fA-F]{6}";

  # ── Enable facts: set only from the directory of the dendrite that owns them
  # NixOS's `readOnly` cannot carry the `default` a fact needs (the default
  # counts as a second definition and fails the owner's own `true`), so the
  # guard is the merge. The merge sees only the winning definitions: the
  # priority filter runs first. A winner from any file but the owner's or this
  # declaration's default is refused, and so is a tie between the owner and a
  # write at its priority. A host write the owner outranks (a `mkDefault`
  # against the owner's plain `true`) never reaches the merge: it is dropped and
  # cannot change the value.
  inherit (import ../default.nix) catalogue;
  mkFact =
    name:
    let
      owner = toString catalogue.${name};
    in
    types.bool
    // {
      merge =
        loc: defs:
        let
          # The owner is the file the catalogue names for `name`, or any file
          # under it when the entry is a directory (`strillix = ./dendrites/strillix;`):
          # a lane may import the owner's files by path, and the fact it sets
          # is still written from the owner's directory.
          fromOwnerOrDecl =
            d:
            toString d.file == owner
            || lib.hasPrefix (owner + "/") (toString d.file)
            || lib.hasSuffix "/modules/nucleus/options.nix" (toString d.file);
        in
        if lib.length defs == 1 && fromOwnerOrDecl (lib.head defs) then
          (lib.head defs).value
        else
          throw "The option `${lib.showOption loc}' is a read-only fact, set only by the dendrite that owns it. Read it, never set it.";
    };

  # ── Component override submodules ─────────────────────────────────────────
  # v0 component tier: bar.* / notif.* / window.*. Each field is optional and
  # falls back to the palette when unset. Paint dendrites read these; nothing else.
  componentOptions =
    fields:
    lib.mapAttrs (
      _: descr:
      mkOption {
        type = types.nullOr hexColor;
        default = null;
        description = descr;
      }
    ) fields;
  mkComponent =
    fields:
    types.submodule {
      options = componentOptions fields;
    };

  barFields = {
    bg = "Bar background colour. Falls back to palette.bg when null.";
    fg = "Bar foreground/text colour. Falls back to palette.fg when null.";
    accent = "Bar accent (active workspace, highlights). Falls back to palette.accent.";
  };
  notifFields = {
    bg = "Notification-center background. Falls back to palette.bg.";
    fg = "Notification text colour. Falls back to palette.fg.";
    urgent = "Urgent-notification colour. Falls back to palette.urgent.";
  };
  windowFields = {
    border = "Focused window border colour. Falls back to palette.accent.";
    borderInactive = "Unfocused window border colour. Falls back to palette.bg.";
  };
  barType = mkComponent barFields;
  notifType = mkComponent notifFields;
  windowType = mkComponent windowFields;

  # ── Geometry tier (v0 optional tier: gaps/border/rounding/blur/opacity)
  # Additive-optional under the existing v0 schema (same nullOr-with-fallback
  # shape as the component tier above): every field is optional and falls
  # back to the compositor lane's opinionated default when unset
  # (`terminalOpacity`: the host's bake in kitty.conf, no Aoide constant). A theme
  # file with no `geometry` block behaves exactly as before — the compositor
  # lane applies the fallback, not the option system.
  geometryOptions = {
    gapsOut = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Outer gap between windows and the screen edge (px). Falls back to the compositor lane's default (8) when null.";
    };
    gapsIn = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Inner gap between adjacent windows (px). Falls back to the compositor lane's default (6) when null.";
    };
    borderSize = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Window border thickness (px). Falls back to the compositor lane's default (2) when null.";
    };
    rounding = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Window corner radius (px). Falls back to the compositor lane's default (0) when null.";
    };
    blurEnabled = mkOption {
      type = types.nullOr types.bool;
      default = null;
      description = "Whether compositor blur is enabled. Falls back to the compositor lane's default (true) when null.";
    };
    blurSize = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Blur kernel size. Falls back to the compositor lane's default (8) when null.";
    };
    blurPasses = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Blur pass count. Falls back to the compositor lane's default (3) when null.";
    };
    terminalOpacity = mkOption {
      type = types.nullOr (types.numbers.between 0 1);
      default = null;
      description = "Terminal (kitty) background opacity, 0–1. Live-side only, in two files: the activation seed writes it into `$STRILLIX_ROOT/declared/terminal-opacity.conf` (deleted when null), and `rice stage` writes it into the staged terminal file (only for a rice with an opinion) and has open kitty windows reload their config. The staged file is included second, so an opinionated stage wins; null is no opinion and the host's bake in kitty.conf shows, with no Aoide constant behind it.";
    };
  };

  # ── Font tier (v0 optional overrides: the face the rice wears) ────────────
  # Additive-optional, the geometry tier's posture: an absent tier means "the
  # stylix lane's own default". The ROLE shape is Stylix's own — `{ package,
  # name }` — because the stylix lane is the tier's one consumer and passes the
  # value through, no translation.
  #
  # BAKED ONLY, the `wallpaper` tier's posture rather than geometry's: nothing
  # at runtime reads a face, so there is no `stage/theme.json` twin and no
  # engine schema — the stylix lane is the only reader, and the change lands on
  # a rebuild. (Geometry rides the stage file because `hyprctl keyword` CAN set
  # it live; a terminal cannot be re-faced mid-session.)
  #
  # A rice names `package` as a literal nix value (the `wallpaper` note's
  # precedent), not a string attribute name — Stylix needs the derivation to
  # install the face, and a face nobody installed is a silent fallback.
  fontRoleType = types.submodule {
    options = {
      name = mkOption {
        type = types.str;
        description = "Fontconfig family name, exactly as `fc-list` reports it (e.g. \"ShureTechMono Nerd Font Mono\").";
      };
      package = mkOption {
        type = types.package;
        description = "The package that installs the face. Required: naming a face nothing installs falls back silently.";
      };
    };
  };

  fontOptions = {
    monospace = mkOption {
      type = types.nullOr fontRoleType;
      default = null;
      description = ''
        The fixed-pitch face — the terminal, where agents live. Falls back to
        the stylix lane's default ("Linux Libertine Mono O") when null. The
        one role a rice uses today; the remaining roles stay the lane's.
      '';
    };
  };

  # ── Palette tier (v0 closed tier: base16-derived) ────────────────────────
  paletteOptions = {
    bg = mkOption {
      type = hexColor;
      default = "#1e1e2e";
      description = "Base background colour (base16 base00).";
    };
    fg = mkOption {
      type = hexColor;
      default = "#cdd6f4";
      description = "Base foreground/text colour (base16 base05).";
    };
    accent = mkOption {
      type = hexColor;
      default = "#89b4fa";
      description = "Primary accent colour (base16 base0D).";
    };
    urgent = mkOption {
      type = hexColor;
      default = "#f38ba8";
      description = "Urgent/error colour (base16 base08).";
    };
    hot = mkOption {
      type = types.nullOr hexColor;
      default = null;
      description = ''
        Hot/trace highlight — the one-neon element (the Pantheon stills'
        optic-nerve green, base16 base0B). Optional: falls back to accent
        when null. NOT part of the stylix base16 synthesis (the base16 tier
        already carries the real scheme); this is the live-note trace colour.
      '';
    };
  };

  # ── Override tier (venue recolour — HOST-set, never rice-set) ─────────────
  # The five palette anchors, each optional. Semantics (CONTRACTS.md §1,
  # "Override tier"): an overridden anchor rewrites EVERY theme colour equal
  # to the rice's authored value for that anchor — palette, base16, component
  # tiers — in one simultaneous pass, in BOTH fan-outs (baked Stylix scheme
  # and the $STRILLIX_ROOT/stage/theme.json seed). A recolour, never a re-key: slots
  # not carrying an overridden anchor's value stay the rice's. Same
  # nullOr-hex-per-field shape as the component tier above (mkComponent).
  #
  # Phase 1b (named-key overrides): sibling sub-tiers `base16`/`bar`/`notif`/
  # `window` set ONE slot exactly — no propagation, no value-match
  # participation (lib/theme.nix's `resolve` applies these as a second,
  # overlay pass). No name collision with the five anchor fields above
  # (anchors are bg/fg/accent/urgent/hot; the sub-tiers are base16/bar/
  # notif/window). `base16` is its OWN all-optional submodule here, built
  # from `base16Roles` the same way `base16Type` is (NOT `base16Type`
  # itself — that one requires all 16 slots once given, right for a rice's
  # scheme, wrong for a slot-exact override where every field must default
  # to null). `bar`/`notif`/`window` reuse `barType`/`notifType`/`windowType`
  # verbatim: those are already nullOr-per-field with default null (the
  # component-tier shape), so an unset override field really is "no
  # override" and reuse costs nothing — minting override-worded twins would
  # be churn for the same shape.
  overrideType = types.submodule {
    options =
      lib.mapAttrs
        (
          _: descr:
          mkOption {
            type = types.nullOr hexColor;
            default = null;
            description = descr;
          }
        )
        {
          bg = "Venue recolour of the rice's bg anchor (and every slot carrying its value).";
          fg = "Venue recolour of the rice's fg anchor (and every slot carrying its value).";
          accent = "Venue recolour of the rice's accent anchor (and every slot carrying its value).";
          urgent = "Venue recolour of the rice's urgent anchor (and every slot carrying its value).";
          hot = "Venue recolour of the rice's hot anchor; when the rice left hot null, sets it directly.";
        }
      // {
        base16 = mkOption {
          type = types.submodule {
            options = lib.mapAttrs (
              slot: role:
              mkOption {
                type = types.nullOr hexColor;
                default = null;
                description = "Slot-exact venue override of base16 ${slot} (${role}). No propagation.";
              }
            ) base16Roles;
          };
          default = { };
          description = "Slot-exact base16 overrides — sets THAT slot, propagates nothing.";
        };
        bar = mkOption {
          type = barType;
          default = { };
          description = "Slot-exact bar.* overrides — sets that field, propagates nothing.";
        };
        notif = mkOption {
          type = notifType;
          default = { };
          description = "Slot-exact notif.* overrides — sets that field, propagates nothing.";
        };
        window = mkOption {
          type = windowType;
          default = { };
          description = "Slot-exact window.* overrides — sets that field, propagates nothing.";
        };
      };
  };

  # ── Base16 scheme submodule (optional full-scheme tier) ───────────────────
  # A rice MAY carry a complete base16 scheme following the base16 standard's
  # slot semantics (https://github.com/chriskempson/base16 — 00..07 the
  # grayscale ramp, 08..0F the accent set). When present, the stylix lane
  # bakes it verbatim instead of synthesising a degenerate scheme from the
  # 4-anchor palette. All 16 slots are required once the tier is given — a
  # partial scheme would silently fall back per-slot and drift.
  base16Roles = {
    base00 = "default background";
    base01 = "lighter background (status bars, line numbers)";
    base02 = "selection background";
    base03 = "comments, invisibles";
    base04 = "dark foreground (status bars)";
    base05 = "default foreground";
    base06 = "light foreground";
    base07 = "lightest background / bright foreground";
    base08 = "red — variables, errors, urgent";
    base09 = "orange — integers, constants";
    base0A = "yellow — classes, search highlight";
    base0B = "green — strings, success";
    base0C = "cyan — support, regex, escapes";
    base0D = "blue — functions, headings, primary accent";
    base0E = "magenta — keywords, storage";
    base0F = "brown — deprecated, embedded punctuation";
  };
  base16Type = types.submodule {
    options = lib.mapAttrs (
      slot: role:
      mkOption {
        type = hexColor;
        description = "base16 ${slot}: ${role}.";
      }
    ) base16Roles;
  };

  # ── Surface-ownership registry entry ──────────────────────────────────────
  surfaceType = types.submodule {
    options = {
      owner = mkOption {
        type = types.str;
        example = "quickshell";
        description = ''
          The single module that renders this surface. The strillix lane
          populates this registry; Stylix reads it and disables its own
          derivation for owned surfaces. `checks.surface-ownership` asserts
          every declared surface names exactly one owner.
        '';
      };
    };
  };

  # ── Declared widget-type registry entry (v1) ──────────────────────────────
  # Lets a rice register a brand-new widget TYPE via nix, apart from the
  # shipped/anchored slot catalog (CONTRACTS.md §5 "Per-rice flavor
  # widgets"). Keyed by slot name in `layout.widgets` below — the attribute
  # name IS the slot name, expected shape `[a-z0-9][a-z0-9-]*` (documented
  # convention, not enforced at this layer — same permissive-here,
  # `rice lint`-is-authoritative posture as `hexColor` above). The rice's own
  # QML body for a declared slot still lives at
  # `rices/<name>/widgets/<slot>.qml` same as any other slot — this
  # option only declares that the slot IS a widget-type registration, not
  # just inert score.
  widgetType = types.submodule {
    options = {
      kind = mkOption {
        type = types.enum [
          "surface"
          "dock"
        ];
        default = "surface";
        description = ''
          The widget's registration kind. `surface` owns its own
          PanelWindow/layer (powermenu/launcher-style overlays); `dock`
          mounts as an Item into AoidePanel's existing gadget column,
          alongside the shipped ConductorGadget/TerminalsGadget/etc.
        '';
      };
      namespace = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          The window/layer-shell namespace this widget registers under.
          null derives to "aoide-<slot>" — the consumer (the compositor
          lane) computes this from the attribute key, since a plain option
          default can't see its own key.
        '';
      };
      layer = mkOption {
        type = types.enum [
          "overlay"
          "top"
        ];
        default = "overlay";
        description = "The compositor layer this widget's surface renders on.";
      };
      shortcut = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          A GlobalShortcut name (e.g. "aoide:<slot>") a venue's compositor
          config may bind a key to. Naming a shortcut is not binding it —
          the venue owns the actual keybind, keeping the rice/venue split
          intact.
        '';
      };
      blur = mkOption {
        type = types.bool;
        default = true;
        description = "Whether the compositor blurs behind this widget's surface.";
      };
      order = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = ''
          Sort key for deterministic layout among multiple declared `dock`
          entries. The registry is serialized through
          `serde_json::Value`/`BTreeMap` on the Rust side (no
          `preserve_order` feature enabled), so a rice's declared widget
          keys do NOT preserve authored order between the build-time nix
          walk and the native hot-sync — `order` is the only way to get
          deterministic layout among multiple `dock` widgets. Consumers
          sort by `(order ?? 0, slot-name)` as tie-break. Meaningless for a
          `kind = "surface"` entry (the compositor lane and the QML
          runtime don't use it) — invalid combinations are rejected
          native-side (Phase 9), not by a nix-level constraint (nix option
          types can't easily express "field X only valid when kind==Y").
        '';
      };
    };
  };

  # ── Expected-paint entry (v1 `layout.surfaces`) ──────────────────────
  # One flag per declared surface. Keyed by slot name in
  # `layout.surfaces` below — the attribute name IS the slot name,
  # resolving to the layer-shell namespace `aoide-<slot>` exactly as
  # `widgetType.namespace` derives one when null. This declares an
  # EXPECTATION and instantiates nothing: no surface is stood up by naming
  # one here.
  surfaceExpectationType = types.submodule {
    options = {
      perMonitor = mkOption {
        type = types.bool;
        default = false;
        description = ''
          How many mapped instances this surface is expected to have:
          true = one per ENABLED OUTPUT (the consumer reads the live output
          list and expects that many namespaced layer surfaces), false =
          exactly one, wherever it lands. The flag is the surface's own
          multiplicity, never the monitor count — a per-monitor surface on a
          single-output host and a one-instance surface on a six-monitor host
          both read correctly.
        '';
      };
    };
  };

in
{
  # The core's own module (`pkgs/aoide/module`, `nixosModules.default` of the
  # `aoide` input) is imported by the nucleus module that closes over this flake's
  # inputs — `lib/strillix.nix`'s `nucleusModule` — and not here: an `imports`
  # list cannot read a module argument that `_module.args` supplies (the module
  # list is what `config` is assembled from, so the read is a cycle), and this
  # file must keep evaluating when a bare `evalModules` imports it without any
  # lane around it (`lib/options.nix` reads the option declarations that way).
  options.strillix = {
    # ── Rice runtime root and host file (moved here from the core module) ──
    # Both name Strillix's own state. The core module (`pkgs/aoide/module`) owns
    # no strillix option, so nothing in core reads them: a host that never
    # selects the strillix dendrite still carries these as plain defaults.
    root = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/.strillix";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/.strillix"'';
      description = ''
        The RICE runtime root (`$STRILLIX_ROOT`): the stage (`stage/`, the
        live twin the paint reads), `declared/` (the declared twin), `rices/`
        (the machine's own rices) and `wallpapers/`. Exported as
        `STRILLIX_ROOT` on every unit that runs a `strillix` binary, and into
        `aoide.user`'s login shells beside `AOIDE_ROOT`. Code default, matching
        this option's default exactly: `~/.strillix`.
      '';
    };

    hostConfig = mkOption {
      type = types.str;
      default = "${config.aoide.checkout}/hosts/${config.networking.hostName}/default.nix";
      defaultText = literalExpression ''"''${config.aoide.checkout}/hosts/''${config.networking.hostName}/default.nix"'';
      description = ''
        This machine's host config file — the module holding its one
        `habit.rice.declared = "<rice>";` line, which `strillix rice declare
        <rice> --host` rewrites so the next rebuild performs that rice.
        Exported as `STRILLIX_HOST_CONFIG` on the strillix units, and in
        `aoide.user`'s own login shells. A consumer whose hosts live elsewhere
        sets its own path. Code default, matching this option's own default
        exactly: `<checkout>/hosts/<hostname>/default.nix`.
      '';
    };

    # ── Rice selection — the replay seam ───────────────────────────────────
    # The rice (rice) this host performs. A rice is host-agnostic: ANY host in
    # the fleet replays any committed rice by naming it here — one line, no
    # other edits. The shipped standard is rice "sonata"; committed rices live
    # under rices/<name>/ as bodies that the selected rice alone brings
    # in (the same selection discipline as a dendrite — see CONTRACTS.md §5).
    #
    # The VENUE (host) decides its instruments (dendrites, hardware);
    # the RICE carries only the theme (palette + component tiers). A rice must
    # never set host options or enable dendrites.
    rice = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "sonata";
      description = ''
        The rice (rice) this host performs. Set to a committed rice name
        (a folder under rices/<name>/) to replay it on this host;
        the theme fan-out swaps with zero other edits. Null (the default)
        means no rice is named: a paint lane reads this null and deploys
        nothing — no QML tree, no seeded stage, no rice restart — rather than
        an empty surface, and no rice's body is imported. Naming a rice is what
        puts a shell tree under the runtime root;
        a host that brings its own config directory still runs a shell on it
        (`strillix.quickshell.config`), rice or no rice. A host wanting the
        desktop names its rice explicitly, e.g.
        `strillix.rice = "sonata";` for the shipped standard.
      '';
    };

    # The rices this host BUILDS IN — the host's `habit.rice.declared ∪
    # habit.rice.available`, derived and set by the constructor's hook
    # (`lib/strillix.nix`, over `lib/rices.nix`'s `builtIn`). A FACT about the
    # selection, exactly as `strillix.rice` is, and never hand-set: the lane that
    # builds a rice's widgets, installs its packages and seeds its folder reads
    # it instead of asking the host, which it cannot see.
    #
    # It is declared here, in the unconditional core, because the hook sets it
    # on EVERY host — including one that selected no paint lane at all, where
    # nothing reads it and it stays the empty default. A lane that reads it is
    # reading a fact about the host, not another module (root AGENTS.md house
    # rule 5 names it in the closed list for that reason).
    rices.builtIn = mkOption {
      type = types.listOf types.str;
      default = [ ];
      example = [ "sonata" ];
      description = ''
        The rices built into this system: the `rice.nix` files imported, the
        widgets copied into the deployed shell tree, the packages installed and
        the folders the machine rices is seeded with. Derived from the host
        record's rice selection — and CLOSED UNDER BORROWS (CONTRACTS.md §5): a
        rice whose records borrow another rice's slots builds the lender in too,
        because the borrower's slots resolve to `rices/<owner>/<file>` on disk.
        A host never sets it.
      '';
    };

    # The declared rice's greeter face — a fact about the selection, derived
    # and set by the same hook as `builtIn` (`lib/strillix.nix`, over
    # `lib/rices.nix`'s `greeter`) and never hand-set. The greeter lane
    # reads it to choose between a rice-painted Quickshell greeter and ly,
    # and to find the face's files in the shipped rices.
    rices.greeter = mkOption {
      type = types.nullOr (types.attrsOf types.str);
      default = null;
      example = {
        owner = "barcarolle";
        file = "greeter.qml";
      };
      description = ''
        The declared rice's `greeter` slot record, `{ owner, file }`: the
        rice whose `widgets/` holds the face (the declared rice itself, or
        the rice it borrows the slot from) and the file's name there. `null`
        when the declared rice ships no greeter face, or no rice is
        declared. Derived from the host record's rice selection; a host
        never sets it.
      '';
    };

    # ── Theme seam (v0 schema) — the ONLY thing paint lanes read ─────────
    # The v0 theme schema — the single seam between the frozen nix layer
    # and the live desktop. Paint dendrites consume this and nothing else.
    # Aoide's design-token layer: the tokens themselves, named for the one
    # set of house colours every surface wears in unison. Values and engine
    # are one thing: the tokens are theme, resolved/validated/emitted by
    # theme. The container remains the W3C design-tokens format. Versioned
    # as "theme schema v0" in CONTRACTS.md.
    #
    # A plain option set, not a submodule. The module system pushes a rice's
    # `mkDefault` down through option sets to each option, and an option whose
    # type is a submodule takes its value whole. So each value below is its own
    # option: a host's plain write outranks only the value it names, except for
    # `base16`, `fonts.monospace` and `override`, which a host replaces whole.
    theme = {
      # Palette tier (base16-derived): bg / fg / accent / urgent.
      palette = paletteOptions;
      polarity = mkOption {
        type = types.enum [
          "light"
          "dark"
        ];
        default = "light";
        description = ''
          The register the base16 ramp reads as — "light" or "dark". A rice
          declares it beside its palette (a palette brings its polarity),
          and the stylix lane hands it to `stylix.polarity`, which themes
          every Stylix-managed target from it. The default keeps every host
          that names no polarity evaluating exactly as before.

          BAKED ONLY, the font tier's posture: no emitter carries it and
          `rice stage` does not apply it — GTK/Qt and Stylix read it at
          build time, so flipping it is a user-gated rebuild, not a stage.
          It is still carried in a rice's `theme.json` beside `palette`
          and validated by `rice lint` (exactly "light"|"dark", or absent
          for "no opinion"), and it reaches `$STRILLIX_ROOT/stage/theme.json` through
          the activation seed like every other top-level key — where nothing
          reads it today.
        '';
      };
      base16 = mkOption {
        type = types.nullOr base16Type;
        default = null;
        description = ''
          Optional full base16 scheme (all 16 slots, base16-standard
          semantics). When set, the stylix lane bakes this scheme for
          terminals/editors/GTK instead of synthesising one from the
          4-anchor palette. The palette tier still drives the live
          (stage/theme.json) side; keep the two in the same key.
        '';
      };
      # Component overrides for the Quickshell bar surface.
      bar = componentOptions barFields;
      # Component overrides for the notification surface.
      notif = componentOptions notifFields;
      # Component overrides for compositor window decoration.
      window = componentOptions windowFields;
      # Geometry tier (v0 optional overrides): gaps/border/rounding/blur.
      # Every field is nullOr and falls back to the compositor lane's
      # opinionated default when unset — additive-optional, same status
      # as the base16 tier. Hyprland-only in this pass; no QML consumer.
      geometry = geometryOptions;
      # Font tier (v0 optional overrides, the geometry tier's posture):
      # the face this rice wears, per Stylix font role — `monospace`
      # today, the terminal an agent lands in. Null means "the stylix
      # lane's own default" (Linux Libertine), so a rice that sets no
      # face bakes exactly the theme it baked before.
      #
      # BAKED ONLY — like `wallpaper`, and unlike `geometry`: this tier
      # has no `stage/theme.json` twin and no engine schema entry,
      # because a face cannot hot-swap. It lands on the user-gated
      # rebuild, and a staged rice cannot re-face the terminal without
      # one.
      fonts = fontOptions;
      wallpaper = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = ''
          The wallpaper-art note: the wallpaper image this rice carries, as a
          literal nix path (copied to the store — never a rice/ runtime
          read). Paint dendrites bake it as the stylix base-context image. null
          "no wallpaper" — the stylix lane falls back to its deterministic
          solid-colour derivation (from palette.bg), so the baked path stays
          buildable with no binary asset.
        '';
      };
      override = mkOption {
        type = overrideType;
        default = { };
        description = ''
          Venue recolour tier (CONTRACTS.md §1, "Override tier"). Set by
          the HOST (never by a rice) to repaint the performed rice's
          anchors without editing the rices. Consumers apply it
          through lib/theme.nix's `resolve` — the option system stores
          it inert, same posture as the component-tier nulls.
        '';
      };
    };

    # ── Layout seam (v1) — structure, theme's sibling ────────────────
    layout = mkOption {
      description = ''
        The v1 layout schema — one of the three namespaces a paint lane may
        read. Where `strillix.theme` carries the rice's DRESS (palette · base16
        · component tiers · geometry · fonts · wallpaper), layout carries its
        STRUCTURE: which widget/surface TYPES the rice brings into existence
        (`widgets`) and which painted surfaces it expects to stay mapped
        (`surfaces`). Dress and structure are different questions, so they
        are different option trees; the paint read-whitelist stays an
        enumerated, closed set — theme, layout, and the `strillix.surfaces`
        ownership registry (AGENTS.md house rule 5) — never an open `aoide.*`.
      '';
      default = { };
      type = types.submodule {
        options = {
          widgets = mkOption {
            type = types.attrsOf widgetType;
            default = { };
            description = ''
              Declared widget-type registry: lets a rice register a brand-new
              widget TYPE via nix, apart from the shipped/anchored slot
              catalog. Two kinds exist (`kind`, on each entry): `surface`
              owns its own PanelWindow/layer (powermenu/launcher-style);
              `dock` mounts as an Item into AoidePanel's existing gadget
              column. Keyed by slot name (the attribute name IS the slot
              name). The rice's own QML body for a declared slot still lives at
              `rices/<name>/widgets/<slot>.qml` same as any other slot
              (CONTRACTS.md §5) — this option only declares that the slot IS a
              widget-type registration, not just inert score.

              PHYSICAL STORAGE is unchanged by the layout rename: a rice's
              declarations live in that rice's `theme.json` under a flat
              top-level `.widgets` key, sibling to `.palette`/`.base16`/`.bar`
              (theme.json's fields are flat-per-concern, never nested under a
              `"theme"` key). theme.json is the one stage/draft-ROUTED twin
              file — `rice mode draft` symlinks it — so splitting a second file
              off would have to duplicate that routing and keep two files
              atomically consistent across the flip. The option-tree split is a
              NIX NAMESPACE decision about what paint lanes may read; it is not a
              file split. Precedent: theme.json already carries a top-level
              `rice` key with no `strillix.theme.rice` option (CONTRACTS.md §4).
            '';
          };
          surfaces = mkOption {
            type = types.attrsOf surfaceExpectationType;
            default = { };
            example = literalExpression ''
              { bar.perMonitor = true; wallpaper.perMonitor = true; dock.perMonitor = false; }
            '';
            description = ''
              Which painted surfaces this rice EXPECTS to be mapped, declared
              so a health consumer can assert what SHOULD be painted instead
              of only counting whatever is. Keyed by slot name; each key
              resolves to the layer-shell namespace `aoide-<slot>` (the same
              derivation `widgetType.namespace` applies when null), so the
              consumer compares declarations directly against the
              compositor's live layer list and derives nothing of its own.
              The build publishes this as `run/qml/rices/surfaces.json`
              (CONTRACTS.md §5), which is what the consumer reads; it is NOT
              keyed by rice, since an expectation is about what the ACTIVE
              rice should have mapped right now.

              Declare only surfaces that are PERSISTENTLY mapped — a surface
              summoned on demand (a launcher, an OSD, a preview) must NOT be
              declared, or a consumer treating a closed surface as a loss
              would act on a healthy desktop.

              This declares an EXPECTATION and instantiates nothing. It is a
              different question from `layout.widgets` above, which
              declares and INSTANTIATES per-rice flavor widget types: the
              lane-owned surfaces named here are hardcoded in the shell and
              are never widget registrations, so declaring one here must
              never be read as asking for a second copy of it.

              An EMPTY set (the default) declares nothing and expects
              nothing: a consumer must then fall back to its previous
              behaviour, so a rice that says nothing here keeps it. Nothing
              else populates this option — the default is the whole of the
              no-declaration case.
            '';
          };
        };
      };
    };

    # ── Surface-ownership registry ─────────────────────────────────────────
    surfaces = mkOption {
      type = types.attrsOf surfaceType;
      default = { };
      example = literalExpression ''{ bar.owner = "quickshell"; notifications.owner = "quickshell"; }'';
      description = ''
        Render-surface ownership registry. The strillix lane declares the
        surfaces it owns; Stylix reads this and disables derivation for them.
        `checks.surface-ownership` fails eval if a surface has no owner.
      '';
    };

    # ── The component slots this host filled ───────────────────────────────
    components = mkOption {
      type = types.attrsOf (
        types.submodule {
          options = {
            provider = mkOption {
              type = types.str;
              description = "The provider that fills this slot, by its name in the slot's provider set.";
            };
            actions = mkOption {
              type = types.attrsOf (types.listOf types.str);
              default = { };
              description = ''
                The commands, as argv lists, this provider answers a bridge
                action with, by action name. Empty for a `strillix` provider,
                whose component the shell reaches itself.
              '';
            };
          };
        }
      );
      default = { };
      example = literalExpression ''{ bar.provider = "strillix"; launcher = { provider = "rofi"; actions.open = [ "rofi" "-show" "drun" ]; }; }'';
      description = ''
        The component slots (`bar`, `launcher`, `powermenu`, `notifications`,
        `wallpaper`, `dock`, `calendar`, `switcher`, `lock`, `greeter`, and
        any a consumer adds) this host filled, keyed by slot. Each slot's
        chosen provider writes its own entry, so the set is exactly what the
        selection picked. A contract option rather than the runtime's: a
        consumer's own provider writes it with no strillix runtime present. The
        runtime publishes it as `components.json` (CONTRACTS.md §4).
      '';
    };

    # ── Enable facts — the cross-lane seam ─────────────────────────────────
    # What a paint lane brings into existence, declared ONCE. No module reads
    # another module (root AGENTS.md house rule 5), so a cross-lane question —
    # "does this host paint", "is there a shell here" — is never answered by
    # reading the lane's own option. Each fact is true exactly when its dendrite's
    # module is in the host (selected, or imported by path by a lane that brings
    # it), and a file in that dendrite's directory sets it to plain `true`;
    # `mkFact` refuses any other write that takes effect. Each defaults to `false`: a host with no
    # paint lane is headless. `strillix.enable` below, declared beside the
    # package it installs, is the third fact in this family.
    quickshell = {
      enable = mkOption {
        type = mkFact "quickshell";
        default = false;
        description = ''
          A fact: a Quickshell shell surface exists on this host. True exactly
          when the `quickshell` dendrite's system half is imported
          (`dendrites/quickshell/system.nix`, the package and the
          `aoide-quickshell` service), which sets it: by selecting the
          dendrite, or by a lane that brings the shell (`strillix`). Set only from
          that dendrite's directory; read it, never set it. A host whose shell is
          painted by something else leaves the dendrite unselected and this
          false, while still having a graphical session.
        '';
      };

      config = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          The directory the shell runs: a store path or a host-owned config
          directory, as a string. `null` (the default) is the bare case — the
          quickshell package is installed and no shell service runs. Set by
          the lane that supplies a config, read by the lane that installs the
          package, so neither has to know the other by name. A lane with no
          config to give leaves this null: nothing is wired.
        '';
      };
    };

    stylix.enable = mkOption {
      type = mkFact "stylix";
      default = false;
      description = ''
        A fact: a Stylix baked-theme fan-out exists on this host — the theme is
        derived at build time and painted by nix. True exactly when the `stylix`
        dendrite is selected, which sets it. Set only by that dendrite; read it,
        never set it. A host that themes itself entirely at runtime leaves the
        dendrite unselected.
      '';
    };

    # ── Who paints a rice's wallpaper — the capability's fact ──────────────
    # Not an `enable`: this capability ALWAYS has an answer (the shell's own
    # layer is the default), and a host picks between alternatives exactly the
    # way it picks its compositor (`habit.dendrites.wallpaper.provider`).
    # The name is therefore the fact, and it stays a plain string on purpose:
    # the alternatives live in the registry
    # (`modules/dendrites/wallpaper/default.nix`) where adding one is one file
    # plus one line, and an enum here would be a second list to amend.
    wallpaper.provider = mkOption {
      type = types.str;
      default = "quickshell";
      description = ''
        Which provider paints a rice's wallpaper on this host, by the name the
        host selected in the `wallpaper` capability's registry: `"quickshell"`
        (the default — the shell's own `aoide-wallpaper` layer draws the rice's
        default AND the user's picks) or `"skwd-wall"` (the external engine
        paints picks on its own surface while the shell's layer stays mapped
        and paints nothing).

        Set `mkDefault` by the provider file the host chose, so the answer is
        the selection's own; a host may override it to stand a provider's
        config down without deselecting it. The runtime reads it — strillix hands
        it to the CLI and the QML as `$STRILLIX_ROOT/stage/wallpaper-provider`
        (CONTRACTS.md §4), and anything else that must know who paints reads
        the same file.
      '';
    };

    # ── Strillix paint binary (P-A8 of the binary-split workstream) ────────────
    # The fact is Strillix's own, not Aoide's: a host may select strillix with
    # Aoide disabled, and nothing in core reads this.
    enable = mkOption {
      type = mkFact "strillix";
      default = false;
      description = ''
        A fact: the `strillix` paint/rice binary is installed — `pkgs.aoide.strillix`,
        the separate output P-A8 split off the combined aoide/aoided/strillix
        derivation so a headless closure never has to carry it. True exactly
        when the `strillix` dendrite's runtime (`dendrites/strillix/runtime.nix`) is
        imported, which sets it. Set only from that dendrite's directory;
        read it, never set it. Every unit that execs `strillix`
        (shellbridge, the dunst notifications feed) gates on this fact too, so a host
        without the dendrite never has a unit pointed at a missing binary.
      '';
    };

  };

  # No behaviour wired here — nucleus/options.nix is contract-only, so an
  # empty config evaluates cleanly. Wave-1 modules provide `config`.
}
