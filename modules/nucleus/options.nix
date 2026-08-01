# modules/nucleus/options.nix — THE OPTION CONTRACT.
#
# Every other module (dendrites, facets) builds against the options
# declared here. This is versioned in CONTRACTS.md (note schema v0). Facets
# read ONLY `aoide.drachma` and `aoide.surfaces`; no module reads another
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
  # note files stay easy to author; the note engine's `rice lint` is the
  # authoritative validator (see pkgs/drachma).
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
  # back to the compositor facet's opinionated default when unset. A notes
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
in
{
  options.aoide = {
    enable = mkEnableOption "the Aoide agent-wearable desktop framework";

    # ── Song selection — the replay seam ───────────────────────────────────
    # The song (rice) this host performs. A song is host-agnostic: ANY host in
    # the fleet replays any committed song by naming it here — one line, no
    # other edits. The shipped standard is song "default"; committed songs live
    # under song/songbook/<name>/ and self-gate on `aoide.song == "<name>"`
    # (same self-registration discipline as dendrites — see CONTRACTS.md §5).
    #
    # The VENUE (host) decides its instruments (facets/dendrites, hardware);
    # the SONG carries only the notes (palette + component tiers). A song must
    # never set host options or enable facets/dendrites.
    song = mkOption {
      type = types.str;
      default = "default";
      example = "moonlight";
      description = ''
        The song (rice) this host performs. Defaults to "default" — the shipped
        standard baseline, guaranteed present. Set to a committed song name
        (a folder under song/songbook/<name>/) to replay it on this host;
        the notes fan-out swaps with zero other edits.
      '';
    };

    # ── Drachma seam (v0 schema) — the ONLY thing facets read ──────────────
    drachma = mkOption {
      description = ''
        The v0 drachma schema — the single seam between the frozen nix layer
        and the live desktop. Facets consume this and nothing else. Drachma IS
        Aoide's design-token layer: the tokens themselves, named for the coin
        the mint stamps — "notes" and "drachma" are one thing, not a values/
        engine split. The container remains the W3C design-tokens format.
        Versioned as "drachma schema v0" in CONTRACTS.md.
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
              (stage/drachma.json) side; keep the two in the same key.
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
          instead of launching anything.
        '';
      };
    };

    auditLog = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/Aoide/log";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/Aoide/log"'';
      description = ''
        Path to the single audit log. Both the CLI and MCP doors write here;
        there is no per-door log (see concepts/Governance).
      '';
    };

    user = mkOption {
      type = types.str;
      default = "khoa";
      description = "The primary user whose home hosts the ~/Aoide fork.";
    };
  };

  # No behaviour wired here — nucleus/options.nix is contract-only, so an
  # empty config evaluates cleanly. Wave-1 modules provide `config`.
}
