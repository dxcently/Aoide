# pkgs/aoide/module/options.nix — the CORE `aoide.*` option contract: what
# every consumer of `nixosModules.default` may set, declared once here.
{ lib, config, ... }:
let
  inherit (lib)
    mkOption
    mkEnableOption
    types
    literalExpression
    ;
in
{
  options.aoide = {
    enable = mkEnableOption "the Aoide agent-wearable desktop framework";

    root = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/.aoide";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/.aoide"'';
      description = ''
        The AOIDE RUNTIME root (L-C2, strillix-carrier lane, task #107) —
        `state/` (conducting state + account/usage state), `run/` (the
        live-deployed QML and element trees), `log` and the sessions all hang
        off this one directory. Rice state does not: it lives under
        `aoide.strillix.root`. Exported as `AOIDE_ROOT` on every unit that runs
        an `aoide`/`aoided`/`strillix` binary, and into `aoide.user`'s own login
        shells (never another account's, root's included). Core code default,
        matching this option's own default exactly (unset == set-to-default):
        `~/.aoide`, no nix required.

        `~/Aoide` (this option's sibling, `aoide.checkout`) is NOT the
        runtime root on any host — it is purely the dev git checkout. A
        pre-L-C2 host's old `~/Aoide/{rice/stage,state,log}` trees migrate
        into this root's equivalents one-shot, at the first real
        `aoide`/`aoided`/`strillix` invocation after the switch (see
        `aoide_storage::fs::migrate_root_once`'s own doc for the exact
        mechanism).
      '';
    };

    strillix.root = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/.strillix";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/.strillix"'';
      description = ''
        The RICE runtime root (`$STRILLIX_ROOT`): the stage (`stage/`, the
        live twin the paint reads), `declared/` (the declared twin), `rices/`
        (the machine's own rices) and `wallpapers/`. Exported as
        `STRILLIX_ROOT` on every unit that runs a `strillix` binary, and into
        `aoide.user`'s login shells beside `AOIDE_ROOT`. Core code default,
        matching this option's default exactly: `~/.strillix`.
      '';
    };

    checkout = mkOption {
      type = types.str;
      default = "/home/${config.aoide.user}/Aoide";
      defaultText = literalExpression ''"/home/''${config.aoide.user}/Aoide"'';
      description = ''
        The dev git checkout — the seam `aoide soundcheck`'s scan root and the
        committed rices's `nix eval` registry regen all read the checkout through.
        Exported as
        `AOIDE_FLAKE_ROOT` on the same units, and in `aoide.user`'s own login
        shells, as `aoide.root` is.
        Separate from the runtime root (`aoide.root`) since L-C2: composing
        a rice happens under the runtime root, committing it happens in
        this checkout. Core code default, matching this option's own
        default exactly: `~/Aoide`.
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
        Exported as `AOIDE_HOST_CONFIG` on the same units, and in
        `aoide.user`'s own login shells, as `aoide.checkout` is. A consumer
        whose hosts live elsewhere sets its own path. Core code default,
        matching this option's own default exactly:
        `<checkout>/hosts/<hostname>/default.nix`.
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

    sessionTarget = mkOption {
      type = types.str;
      default = "default.target";
      description = ''
        The systemd user target `aoided` STARTS with. `default.target` on a
        headless box, so the daemon and its doors come up at boot with
        linger on; a painting host names its own session target instead,
        and the unit starts once that target is reached — while keeping
        its lifetime its own: the anchor sets no `partOf`, so a dead
        desktop never stops the policy surface.
      '';
    };
  };
}
