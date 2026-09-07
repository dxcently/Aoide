# modules/nucleus/options.nix — THE OPTION CONTRACT.
#
# Every other module (dendrites, facets) builds against the options
# declared here. This is versioned in CONTRACTS.md (livery schema v0). Facets
# read ONLY `aoide.livery`, `aoide.arrangement` and `aoide.surfaces` — an
# enumerated, closed whitelist (AGENTS.md house rule 5); no module reads another
# module. The coupling discipline is enforced by lib/checks.nix, not by
# politeness.
#
# Discovered and applied unconditionally on every host (like dxflake's
# nucleus — no mkIf guard). It declares options and eval-clean defaults only;
# it wires no behaviour, so an empty config evaluates cleanly.
{ lib, config, ... }:
let
  inherit (lib)
    mkOption
    mkEnableOption
    types
    literalExpression
    ;

  # A base16 hex colour, with or without leading '#'. Kept permissive so v0
  # livery files stay easy to author; the livery engine's `rice lint` is the
  # authoritative validator (the native livery engine, pkgs/aoide).
  hexColor = types.strMatching "#?[0-9a-fA-F]{6}";

  # ── Component override submodules ─────────────────────────────────────────
  # v0 component tier: bar.* / notif.* / window.*. Each field is optional and
  # falls back to the palette when unset. Facets read these; nothing else does.
  mkComponent =
    fields:
    types.submodule {
      options = lib.mapAttrs (
        _: descr:
        mkOption {
          type = types.nullOr hexColor;
          default = null;
          description = descr;
        }
      ) fields;
    };

  barType = mkComponent {
    bg = "Bar background colour. Falls back to palette.bg when null.";
    fg = "Bar foreground/text colour. Falls back to palette.fg when null.";
    accent = "Bar accent (active workspace, highlights). Falls back to palette.accent.";
  };
  notifType = mkComponent {
    bg = "Notification-center background. Falls back to palette.bg.";
    fg = "Notification text colour. Falls back to palette.fg.";
    urgent = "Urgent-notification colour. Falls back to palette.urgent.";
  };
  windowType = mkComponent {
    border = "Focused window border colour. Falls back to palette.accent.";
    borderInactive = "Unfocused window border colour. Falls back to palette.bg.";
  };

  # ── Geometry submodule (v0 optional tier: gaps/border/rounding/blur) ──────
  # Additive-optional under the existing v0 schema (same nullOr-with-fallback
  # shape as the component tier above): every field is optional and falls
  # back to the compositor facet's opinionated default when unset. A livery
  # file with no `geometry` block behaves exactly as before — the compositor
  # facet applies the fallback, not the option system.
  geometryType = types.submodule {
    options = {
      gapsOut = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Outer gap between windows and the screen edge (px). Falls back to the compositor facet's default (8) when null.";
      };
      gapsIn = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Inner gap between adjacent windows (px). Falls back to the compositor facet's default (6) when null.";
      };
      borderSize = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Window border thickness (px). Falls back to the compositor facet's default (2) when null.";
      };
      rounding = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Window corner radius (px). Falls back to the compositor facet's default (0) when null.";
      };
      blurEnabled = mkOption {
        type = types.nullOr types.bool;
        default = null;
        description = "Whether compositor blur is enabled. Falls back to the compositor facet's default (true) when null.";
      };
      blurSize = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Blur kernel size. Falls back to the compositor facet's default (8) when null.";
      };
      blurPasses = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Blur pass count. Falls back to the compositor facet's default (3) when null.";
      };
    };
  };

  # ── Palette submodule (v0 closed tier: base16-derived) ────────────────────
  paletteType = types.submodule {
    options = {
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
  };

  # ── Override tier (venue recolour — HOST-set, never song-set) ─────────────
  # The five palette anchors, each optional. Semantics (CONTRACTS.md §1,
  # "Override tier"): an overridden anchor rewrites EVERY livery colour equal
  # to the song's authored value for that anchor — palette, base16, component
  # tiers — in one simultaneous pass, in BOTH fan-outs (baked Stylix scheme
  # and the song/stage/livery.json seed). A recolour, never a re-key: slots
  # not carrying an overridden anchor's value stay the song's. Same
  # nullOr-hex-per-field shape as the component tier above (mkComponent).
  #
  # Phase 1b (named-key overrides): sibling sub-tiers `base16`/`bar`/`notif`/
  # `window` set ONE slot exactly — no propagation, no value-match
  # participation (lib/livery.nix's `resolve` applies these as a second,
  # overlay pass). No name collision with the five anchor fields above
  # (anchors are bg/fg/accent/urgent/hot; the sub-tiers are base16/bar/
  # notif/window). `base16` is its OWN all-optional submodule here, built
  # from `base16Roles` the same way `base16Type` is (NOT `base16Type`
  # itself — that one requires all 16 slots once given, right for a song's
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
          bg = "Venue recolour of the song's bg anchor (and every slot carrying its value).";
          fg = "Venue recolour of the song's fg anchor (and every slot carrying its value).";
          accent = "Venue recolour of the song's accent anchor (and every slot carrying its value).";
          urgent = "Venue recolour of the song's urgent anchor (and every slot carrying its value).";
          hot = "Venue recolour of the song's hot anchor; when the song left hot null, sets it directly.";
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
  # A song MAY carry a complete base16 scheme following the base16 standard's
  # slot semantics (https://github.com/chriskempson/base16 — 00..07 the
  # grayscale ramp, 08..0F the accent set). When present, the Stylix facet
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
          The single module that renders this surface. The Quickshell facet
          populates this registry; Stylix reads it and disables its own
          derivation for owned surfaces. `checks.surface-ownership` asserts
          every declared surface names exactly one owner.
        '';
      };
    };
  };

  # ── Declared widget-type registry entry (v1) ──────────────────────────────
  # Lets a song register a brand-new widget TYPE via nix, apart from the
  # shipped/anchored slot catalog (CONTRACTS.md §5 "Per-song flavor
  # widgets"). Keyed by slot name in `arrangement.widgets` below — the attribute
  # name IS the slot name, expected shape `[a-z0-9][a-z0-9-]*` (documented
  # convention, not enforced at this layer — same permissive-here,
  # `rice lint`-is-authoritative posture as `hexColor` above). The song's own
  # QML body for a declared slot still lives at
  # `song/songbook/<name>/widgets/<slot>.qml` same as any other slot — this
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
          facet) computes this from the attribute key, since a plain option
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
          the venue owns the actual keybind, keeping the song/venue split
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
          `preserve_order` feature enabled), so a song's declared widget
          keys do NOT preserve authored order between the build-time nix
          walk and the native hot-sync — `order` is the only way to get
          deterministic layout among multiple `dock` widgets. Consumers
          sort by `(order ?? 0, slot-name)` as tie-break. Meaningless for a
          `kind = "surface"` entry (the compositor facet and the QML
          runtime don't use it) — invalid combinations are rejected
          native-side (Phase 9), not by a nix-level constraint (nix option
          types can't easily express "field X only valid when kind==Y").
        '';
      };
    };
  };

in
{
  options.aoide = {
    enable = mkEnableOption "the Aoide agent-wearable desktop framework";

    # ── Song selection — the replay seam ───────────────────────────────────
    # The song (rice) this host performs. A song is host-agnostic: ANY host in
    # the fleet replays any committed song by naming it here — one line, no
    # other edits. The shipped standard is song "sonata"; committed songs live
    # under song/songbook/<name>/ and self-gate on `aoide.song == "<name>"`
    # (same self-registration discipline as dendrites — see CONTRACTS.md §5).
    #
    # The VENUE (host) decides its instruments (facets/dendrites, hardware);
    # the SONG carries only the livery (palette + component tiers). A song must
    # never set host options or enable facets/dendrites.
    song = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "sonata";
      description = ''
        The song (rice) this host performs. Set to a committed song name
        (a folder under song/songbook/<name>/) to replay it on this host;
        the livery fan-out swaps with zero other edits. Null (the default)
        means no song is named: "no song, no service" — a paint facet reads
        this null and deploys nothing (no QML tree, no shell service) rather
        than an empty surface, and every song's own rice.nix stays inert
        (its self-gate `config.aoide.song == "<name>"` is never true against
        null). A host wanting the desktop names its song explicitly, e.g.
        `aoide.song = "sonata";` for the shipped standard.
      '';
    };

    # ── Livery seam (v0 schema) — the ONLY thing facets read ───────────────
    livery = mkOption {
      description = ''
        The v0 livery schema — the single seam between the frozen nix layer
        and the live desktop. Facets consume this and nothing else. Livery IS
        Aoide's design-token layer: the tokens themselves, named for the one
        set of house colours every surface wears in unison. Values and engine
        are one thing: the tokens are livery, resolved/validated/emitted by
        livery. The container remains the W3C design-tokens format. Versioned
        as "livery schema v0" in CONTRACTS.md.
      '';
      default = { };
      type = types.submodule {
        options = {
          palette = mkOption {
            type = paletteType;
            default = { };
            description = "Palette tier (base16-derived): bg / fg / accent / urgent.";
          };
          base16 = mkOption {
            type = types.nullOr base16Type;
            default = null;
            description = ''
              Optional full base16 scheme (all 16 slots, base16-standard
              semantics). When set, the Stylix facet bakes this scheme for
              terminals/editors/GTK instead of synthesising one from the
              4-anchor palette. The palette tier still drives the live
              (stage/livery.json) side; keep the two in the same key.
            '';
          };
          bar = mkOption {
            type = barType;
            default = { };
            description = "Component overrides for the Quickshell bar surface.";
          };
          notif = mkOption {
            type = notifType;
            default = { };
            description = "Component overrides for the notification surface.";
          };
          window = mkOption {
            type = windowType;
            default = { };
            description = "Component overrides for compositor window decoration.";
          };
          geometry = mkOption {
            type = geometryType;
            default = { };
            description = ''
              Geometry tier (v0 optional overrides): gaps/border/rounding/blur.
              Every field is nullOr and falls back to the compositor facet's
              opinionated default when unset — additive-optional, same status
              as the base16 tier. Hyprland-only in this pass; no QML consumer.
            '';
          };
          wallpaper = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = ''
              The cover-art note: the wallpaper image this song carries, as a
              literal nix path (copied to the store — never a song/ runtime
              read). Facets bake it as the Stylix base-context image. null means
              "no cover" — the stylix facet falls back to its deterministic
              solid-colour derivation (from palette.bg), so the baked path stays
              buildable with no binary asset.
            '';
          };
          override = mkOption {
            type = overrideType;
            default = { };
            description = ''
              Venue recolour tier (CONTRACTS.md §1, "Override tier"). Set by
              the HOST (never by a song) to repaint the performed song's
              anchors without editing the songbook. Consumers apply it
              through lib/livery.nix's `resolve` — the option system stores
              it inert, same posture as the component-tier nulls.
            '';
          };
        };
      };
    };

    # ── Arrangement seam (v1) — structure, livery's sibling ────────────────
    arrangement = mkOption {
      description = ''
        The v1 arrangement schema — the second (and only other) namespace a
        facet may read. Where `aoide.livery` carries the song's DRESS
        (palette · base16 · component tiers · geometry · cover), arrangement
        carries its STRUCTURE: which widget/surface TYPES the song brings
        into existence. Dress and structure are different questions, so they
        are different option trees; the facet read-whitelist stays an
        enumerated, closed PAIR (AGENTS.md house rule 5), never an open
        `aoide.*`.
      '';
      default = { };
      type = types.submodule {
        options = {
          widgets = mkOption {
            type = types.attrsOf widgetType;
            default = { };
            description = ''
              Declared widget-type registry: lets a song register a brand-new
              widget TYPE via nix, apart from the shipped/anchored slot
              catalog. Two kinds exist (`kind`, on each entry): `surface`
              owns its own PanelWindow/layer (powermenu/launcher-style);
              `dock` mounts as an Item into AoidePanel's existing gadget
              column. Keyed by slot name (the attribute name IS the slot
              name). The song's own QML body for a declared slot still lives at
              `song/songbook/<name>/widgets/<slot>.qml` same as any other slot
              (CONTRACTS.md §5) — this option only declares that the slot IS a
              widget-type registration, not just inert score.

              PHYSICAL STORAGE is unchanged by the arrangement rename: a song's
              declarations live in that song's `livery.json` under a flat
              top-level `.widgets` key, sibling to `.palette`/`.base16`/`.bar`
              (livery.json's fields are flat-per-concern, never nested under a
              `"livery"` key). livery.json is the one stage/draft-ROUTED twin
              file — `rice mode draft` symlinks it — so splitting a second file
              off would have to duplicate that routing and keep two files
              atomically consistent across the flip. The option-tree split is a
              NIX NAMESPACE decision about what facets may read; it is not a
              file split. Precedent: livery.json already carries a top-level
              `song` key with no `aoide.livery.song` option (CONTRACTS.md §4).
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
        Render-surface ownership registry. The Quickshell facet declares the
        surfaces it owns; Stylix reads this and disables derivation for them.
        `checks.surface-ownership` fails eval if a surface has no owner.
      '';
    };

    # ── Agent control plane ────────────────────────────────────────────────
    mcp.enable = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Enable the MCP façade. Off by default (house policy): agents spawn
        `aoide mcp serve --stdio` per session. Network MCP is user-only and
        never agent-enabled.
      '';
    };

    # ── A2A door (CONTRACTS.md §6, v0) ────────────────────────────────────────
    a2a = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the A2A (Agent2Agent) door. Off by default (house policy),
          same as the MCP façade. Localhost/user-scoped; forwarded A2A
          messages are untrusted data.
        '';
      };

      bindAddress = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = ''
          The A2A HTTP bind address. Loopback by default; networked A2A is a
          deliberate, user-only choice.
        '';
      };

      port = mkOption {
        type = types.port;
        default = 8710;
        description = "The A2A HTTP port.";
      };

      spawnAgent = mkOption {
        type = types.str;
        default = "";
        description = ''
          The command `a2a serve` conducts for an A2A-spawned task
          (CONTRACTS.md §6, `message/send`'s spawn path). The A2A client
          supplies only the prompt/message, NEVER the command — the
          executable always comes from this option, set by the operator at
          rebuild time (the admission). Empty (the default) disables
          spawning entirely: `message/send` returns a structured error
          instead of launching anything. A bare-word command must also
          appear on the unit's PATH via `spawnPath` below — a systemd user
          unit's default PATH carries none of the system profile.
        '';
      };

      spawnPath = mkOption {
        type = types.listOf types.package;
        default = [ ];
        description = ''
          Packages placed on the aoide-a2a unit's PATH so `spawnAgent`
          (and the conducted child it launches) can resolve by bare name.
          A systemd user unit's default PATH is minimal (coreutils and
          friends) and does not include /run/current-system/sw/bin — found
          live when a door-summoned spawn on a headless host failed with
          "failed to conduct `claude`: No such file or directory" while an
          interactive shell resolved it fine. Same explicit-package rule
          shellbridge and the secrets popup watcher already follow.
        '';
      };

      tokenFile = mkOption {
        type = types.str;
        default = "";
        description = ''
          Path to a file holding the shared secret an inbound `message/send`
          must present (`Authorization: Bearer <token>`) to be trusted
          (CONTRACTS.md §6 amendment, 2026-08-18). Empty (the default) is
          the fully-open behavior every prior release shipped: loopback
          auto-delivers, Spawn is gated only by `spawnAgent` being set. Once
          non-empty, TWO things change together, with no separate opt-out:
          Spawn REQUIRES a valid token, and loopback STOPS being an implicit
          trust signal (closing the gap where a reverse proxy or tunnel
          makes a remote caller look loopback to the door). The file itself
          is never read by nix — only its path crosses this option and the
          `aoide-a2a` unit's environment; the secret is read off disk once,
          at `a2a serve` launch.
        '';
      };

      bearerSecret = mkOption {
        type = types.str;
        default = "";
        description = ''
          Name of a secret in the local `aoide secrets` broker the A2A door
          expects as its inbound `Authorization: Bearer` token. Only the
          NAME crosses this option and the `aoide-a2a` unit's environment
          (`AOIDE_A2A_BEARER_SECRET`); the door resolves the value fresh on
          every request through the broker socket (consumer `a2a-door`), and
          a resolve failure fails closed. Takes precedence over `tokenFile`
          when set. Empty (the default) leaves the door on the `tokenFile`
          behavior. The unit's user must be able to reach the broker socket
          (group `aoide-secrets-access`).
        '';
      };

      discoveryAdvertise = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Force this instance's own LAN discovery advertisement on (P-P6 +
          task #120, docs/architecture/PAIRING.md's "Discovery
          (advertise-but-locked)" section): `a2a serve` sends a one-line
          `{v, name, host, user}` UDP broadcast advertisement (name plus
          its ssh hop claim — never a door URL, never a key or
          fingerprint: rendezvous, not authentication) on a fixed port,
          ~30s jittered cadence, for `aoide node discover`/`aoide
          pair`'s hostname arm (and bare `aoide pair`) on the same LAN to
          hear. Off by default, same house policy as every other A2A
          knob above; the runtime switch beside this declarative force is
          `aoide node advertise on|off`. Discovery only ever feeds `node
          discover`'s table and `pair`'s hostname-target resolution;
          the pairing ceremony above remains the ONLY thing that ever
          writes a node record.
        '';
      };

      # ── Pairing-ceremony popup (P-PV3, task #132) ─────────────────────────
      pairingPopup = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the `aoide-pair-watch.service` graphical-session user unit
          (`aoide pair watch --popup`) — a typed-code entry dialog per
          actionable pairing request, `lyra pair ask` when it resolves
          (the same six-boxes-plus-dash surface `lyra secrets ask` renders),
          `zenity --entry` otherwise. Off by default, same house policy as
          the MCP façade/A2A door/usage poller/secrets broker: the unit is
          desktop-facet-gated exactly like `aoide-secrets-watch`
          (`aoide.a2a.enable && aoide.facets.quickshell.enable`), but this
          flag is the deliberate opt-in ON TOP of that gate — a host with
          a2a and the quickshell facet both on does NOT get the popup
          unless it also sets this. It lives in the `a2a` family because it
          is meaningless without the door (`aoide.a2a.enable` is part of
          its gate). See `modules/nucleus/aoided.nix` for the unit and
          CONTRACTS.md §6's "Pairing events feed" subsection for the
          popup's own typed-code contract.
        '';
      };
    };

    # ── Usage widget + poller (opt-in, off by default) ───────────────────────
    usage = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the claude.ai usage widget + poller (`aoide usage`, a
          systemd.user timer running it on `aoide.usage.interval`). Off by
          default, same house policy as the MCP façade and A2A door.
        '';
      };

      interval = mkOption {
        type = types.str;
        default = "300s";
        description = ''
          Poller cadence (systemd `OnUnitActiveSec` duration) for `aoide
          usage`, which writes `state/usage.json` (CONTRACTS.md §4): the
          local token/cost rollup from this machine's own transcripts, plus
          the live claude.ai block the unit spawns `curl` for. The widget
          calls a document stale at three times this cadence.
        '';
      };
    };

    # ── Secrets broker (P-V4 of the secrets workstream, renamed from
    # "vault" at P-V4b) ──────────────────────────────────────────────────
    secrets = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Deploy Aoide's secrets broker (`aoide secrets serve`) as a SYSTEM
          service under its own uid (`aoide-secrets`), per the secrets
          design's target topology: secrets home `/var/lib/aoide-secrets`
          (0700, secrets-uid), socket `/run/aoide-secrets/secrets.sock`
          (0660, group `aoide-secrets-access`) as the only door. Off by
          default, same house policy as the MCP façade/A2A door/usage
          poller above — see `modules/nucleus/secrets.nix` for the unit,
          and `pkgs/aoide/crates/secrets/README.md` for the broker itself
          (the binary is nix-independent; this option is deployment only).
        '';
      };

      members = mkOption {
        type = types.listOf types.str;
        default = [ ];
        example = literalExpression ''[ "khoa" ]'';
        description = ''
          User names added to the `aoide-secrets-access` group — the
          socket's group, so membership is what lets an ordinary
          operator-uid agent connect to `/run/aoide-secrets/secrets.sock`
          at all (the policy gate inside the broker still decides
          per-secret/per-consumer after that). Empty by default:
          `aoide.secrets.enable = true` alone grants nobody access until a
          host names its operator here.
        '';
      };
    };

    # ── Lyra paint binary (P-A8 of the binary-split workstream) ────────────
    lyra = {
      enable = mkOption {
        type = types.bool;
        default = config.aoide.facets.quickshell.enable;
        defaultText = literalExpression "config.aoide.facets.quickshell.enable";
        description = ''
          Install the `lyra` paint/rice binary — `pkgs.aoide.rice`, the
          separate output P-A8 split off the combined aoide/aoided/lyra
          derivation so a headless closure never has to carry it. Defaults
          to whether the quickshell facet is enabled (lyra exists to paint a
          shell, so a graphical host wants it and a headless one doesn't by
          default), but is independently overridable: an explicit flag,
          not only the facet inference, for a future aoide config that wants
          lyra without quickshell (or vice versa). Every unit that execs
          `lyra` (shellbridge, the dunst herald feed) gates on this flag too,
          so flipping it off never leaves a unit pointed at a missing binary.
        '';
      };
    };

    root = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/.aoide";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/.aoide"'';
      description = ''
        The AOIDE RUNTIME root (L-C2, lyra-carrier lane, task #107) —
        `song/stage/`, `state/` (conducting state + account/usage state),
        `run/qml/` (the live-deployed QML tree), and the composed
        `songbook/` all hang off this one directory. Exported as
        `AOIDE_ROOT` on every unit that runs an `aoide`/`aoided`/`lyra`
        binary and into interactive shells. Core code default, matching
        this option's own default exactly (unset == set-to-default): `~/.aoide`,
        no nix required.

        `~/Aoide` (this option's sibling, `aoide.checkout`) is NOT the
        runtime root on any host — it is purely the dev git checkout. A
        pre-L-C2 host's old `~/Aoide/{song/stage,state,log}` trees migrate
        into this root's equivalents one-shot, at the first real
        `aoide`/`aoided`/`lyra` invocation after the switch (see
        `aoide_storage::fs::migrate_root_once`'s own doc for the exact
        mechanism).
      '';
    };

    checkout = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/Aoide";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/Aoide"'';
      description = ''
        The dev git checkout — the seam `rice declare`'s commit-in step,
        `aoide soundcheck`'s scan root, and the committed songbook's `nix
        eval` registry regen all read the checkout through. Exported as
        `AOIDE_FLAKE_ROOT` on the same units/shells `aoide.root` is.
        Separate from the runtime root (`aoide.root`) since L-C2: composing
        a song happens under the runtime root, committing it happens in
        this checkout. Core code default, matching this option's own
        default exactly: `~/Aoide`.
      '';
    };

    auditLog = mkOption {
      type = types.str;
      default = "${config.aoide.root}/log";
      defaultText = literalExpression ''"''${config.aoide.root}/log"'';
      description = ''
        Path to the single audit log. Both the CLI and MCP doors write here;
        there is no per-door log (see concepts/Governance).
      '';
    };

    terminal = mkOption {
      type = types.str;
      default = "";
      example = "kitty -e {cmd}";
      description = ''
        The terminal emulator invocation `spawn --windowed` and
        `resurrect` open a session in, as a plain string with a
        `{cmd}` placeholder. A bare `{cmd}` splices the conducted argv in
        as separate arguments (`kitty -e {cmd}`); a quoted one is joined
        into a single shell word (`foot sh -c '{cmd}'`).

        Empty means no terminal is configured, and both commands answer
        with a taught error naming this option rather than guessing an
        emulator. A terminal dendrite sets this with `mkDefault`, so
        enabling one is normally the whole configuration; naming it here
        overrides that pick.

        The daemon needs this because a systemd user unit inherits no
        shell environment: without it the boot-time auto-resume sweep runs
        and silently resumes nothing.
      '';
    };

    user = mkOption {
      type = types.str;
      default = "khoa";
      description = "The primary user whose home hosts the ~/Aoide clone.";
    };
  };

  # No behaviour wired here — nucleus/options.nix is contract-only, so an
  # empty config evaluates cleanly. Wave-1 modules provide `config`.
}
