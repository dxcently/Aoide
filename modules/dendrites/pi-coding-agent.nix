# modules/dendrites/pi-coding-agent.nix — the Pi Coding Agent CLI
# (earendil-works/pi, nixpkgs' pi-coding-agent).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.pi-coding-agent.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# Third sibling of claude-code.nix / kimi-code.nix: another agentic terminal
# coding CLI, each toggled independently rather than bundled into
# devtools.nix (same reasoning as that split). Pi is also the structural
# reference for aoide's own crate-per-charter package restructure
# (docs/architecture/PACKAGE-LAYOUT.md) — packaging its CLI here is
# unrelated to that: this just puts the binary on PATH, MIT-licensed,
# straight from nixpkgs (no unfree flag, unlike claude-code).
#
# What this dendrite does:
#   - Installs `pi` (pkgs.pi-coding-agent) system-wide.
#   - Installs a global Pi extension that replaces the compact startup wordmark
#     with the full block-art mascot. `/builtin-header` restores Pi's default.
#     The extension is TUI-only and has no effect in print/JSON/RPC modes.
#   - No systemd unit or adapter wiring (unlike melete.nix/mneme.nix, which run
#     daemons this repo dispatches jobs to). Auth/config is out of scope here.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.pi-coding-agent.enable = lib.mkEnableOption "the Pi Coding Agent CLI (earendil-works/pi)";

  config = lib.mkIf config.aoide.pi-coding-agent.enable {
    environment.systemPackages = [ pkgs.pi-coding-agent ];

    home-manager.users.${config.aoide.user} = {
      home.file.".pi/agent/extensions/aoide-pi-header.ts".text = ''
        import type { ExtensionAPI, Theme } from "@earendil-works/pi-coding-agent";
        import { VERSION } from "@earendil-works/pi-coding-agent";

        function piMascot(theme: Theme): string[] {
          const accent = (text: string) => theme.fg("accent", text);
          const dim = (text: string) => theme.fg("dim", text);
          const eye = `█''${dim("▌")}`;
          const bar = accent("█".repeat(14));
          const legs = `     ''${accent("██")}    ''${accent("██")}`;

          return [
            "",
            `     ''${eye}  ''${eye}`,
            `  ''${bar}`,
            legs,
            legs,
            legs,
            legs,
            "",
            `''${theme.fg("muted", "   pi — the minimal coding agent")} ''${theme.fg("dim", `v''${VERSION}`)}`,
          ];
        }

        export default function (pi: ExtensionAPI) {
          pi.on("session_start", (_event, ctx) => {
            if (ctx.mode !== "tui") return;

            ctx.ui.setHeader((_tui, theme) => ({
              render(_width: number): string[] {
                return piMascot(theme);
              },
              invalidate() {},
            }));
          });

          pi.registerCommand("builtin-header", {
            description: "Restore Pi's compact startup header",
            handler: async (_args, ctx) => {
              ctx.ui.setHeader(undefined);
              ctx.ui.notify("Pi's built-in header restored", "info");
            },
          });
        }
      '';
    };
  };
}
