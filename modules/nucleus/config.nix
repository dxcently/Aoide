# modules/nucleus/config.nix — nix as ONE authoring front-end for the
# portable runtime config (task #135 P-C, CONTRACTS.md §4's `config.toml`
# subsection).
#
# Core is portable: `aoide`/`aoided` are cargo-buildable on any Linux, with
# no nix shell-outs and no NixOS assumption (root AGENTS.md). So a core
# command's configuration cannot LIVE in a NixOS option — on a non-nix host
# that option does not exist, and "rebuild to change a grant" is not an
# operation. The runtime file `$AOIDE_ROOT/config.toml` is the source of
# truth; this module is one way to author it, never its owner.
#
# Nix POINTS, never copies: `aoide.config.settings` renders through
# `pkgs.formats.toml` to a read-only store path, and `AOIDE_CONFIG` names
# that path. Immutability IS the provenance — there is no marker field to go
# stale, and the two worlds never write the same file, so a rebuild
# structurally cannot eat an `aoide config set` edit. The CLI reads the store
# path fine and refuses to write it, naming the option to edit instead.
#
# Whole-file or nothing. Enabled, nix owns the entire config; disabled
# (the default), `$AOIDE_ROOT/config.toml` is the operator's own editable
# file and nothing here touches it. Partial management — nix owning one
# section while the CLI owns another — is deliberately not offered: two
# writers on one document is the split-brain this design exists to avoid.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.aoide.config;
  toml = pkgs.formats.toml { };
  rendered = toml.generate "aoide-config.toml" cfg.settings;
in
{
  options.aoide.config = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Render `aoide.config.settings` to a read-only store path and point
        `AOIDE_CONFIG` at it, making this host's runtime config
        nix-authored. Off by default, same house policy as the MCP façade,
        the A2A door, the usage poller, and the secrets broker: an
        unconfigured host keeps `$AOIDE_ROOT/config.toml` as its own
        editable file, which `aoide config set` writes in place.

        Turning this on is whole-file: every key the CLI would otherwise
        write comes from `settings` instead, and `aoide config set` refuses
        with a taught error naming this option. Turning it back off does not
        migrate anything — the store path simply stops being pointed at, and
        `$AOIDE_ROOT/config.toml` (whatever it holds) is authoritative
        again.
      '';
    };

    settings = lib.mkOption {
      inherit (toml) type;
      default = { };
      example = lib.literalExpression ''{ pairing.defaultGrant = [ "read" "spawn" ]; }'';
      description = ''
        The whole runtime config, as an attrset rendered to TOML. The schema
        is `aoide config`'s own (CONTRACTS.md §4's `config.toml`
        subsection), not a second nix-side vocabulary: keys are spelled
        exactly as they read in the file, and an unknown key or an unknown
        capability is refused by the binary at read time rather than
        silently ignored — `aoide config` on the host is how a rendered
        config is verified.
      '';
    };
  };

  config = lib.mkIf (config.aoide.enable && cfg.enable) {
    # Two consumers of one path, the same "one option, two consumers" shape
    # `AOIDE_TERMINAL`/`AOIDE_ROOT` already hold in aoided.nix: an operator's
    # own `aoide config` in a shell, and every aoide user unit. The unit half
    # goes through the user manager's own default environment rather than an
    # enumeration of unit names — every aoide unit is a user unit, and an
    # enumeration is one more list to drift each time a unit is added.
    environment.sessionVariables.AOIDE_CONFIG = "${rendered}";
    systemd.user.settings.Manager.DefaultEnvironment = "AOIDE_CONFIG=${rendered}";
  };
}
