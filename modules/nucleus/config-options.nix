# modules/nucleus/config-options.nix — the `aoide.config` option contract.
#
# `aoide.config.settings` is the whole portable runtime config
# (CONTRACTS.md §4's `config.toml` subsection) as the nix front-end spells it;
# its type comes from `pkgs.formats.toml`, so the option and the generator
# agree on one schema. The declaration is the contract (an option a consumer or
# another module may set or read); the `aoide-config` dendrite
# (`modules/dendrites/aoide-config.nix`) is what renders it and points
# `AOIDE_CONFIG` at the result.
{ lib, pkgs, ... }:

let
  toml = pkgs.formats.toml { };
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
}
