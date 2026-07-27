# modules/nucleus/options.nix — THE OPTION CONTRACT.
#
# Every other module (dendrites, facets, rime) builds against the options
# declared here. This is versioned in CONTRACTS.md (note schema v0). Facets
# read ONLY `aoide.notes` and `aoide.surfaces`; no module reads another
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
    };
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
    # under song/repertoire/<name>/ and self-gate on `aoide.song == "<name>"`
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
        (a folder under song/repertoire/<name>/) to replay it on this host;
        the notes fan-out swaps with zero other edits.
      '';
    };

    # ── Note seam (v0 schema) — the ONLY thing facets read ─────────────────
    notes = mkOption {
      description = ''
        The v0 note schema — the single seam between the frozen nix layer
        and the live desktop. Facets consume this and nothing else. Notes are
        Aoide's design-token layer; the container remains the W3C
        design-tokens format. Versioned as "note schema v0" in CONTRACTS.md.
      '';
      default = { };
      type = types.submodule {
        options = {
          palette = mkOption {
            type = paletteType;
            default = { };
            description = "Palette tier (base16-derived): bg / fg / accent / urgent.";
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
