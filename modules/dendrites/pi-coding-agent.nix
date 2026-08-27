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

      # The aoide session bridge: pi's twin of claude's ~/.claude/settings.json
      # hooks. Every lifecycle event shells out to `aoide session hook`
      # with a canonical claude-shaped payload on stdin — the aoide side reads
      # it through the pi AgentProfile (hook_event_map, identity normalize, the
      # pi TranscriptSpec). TUI-only: pi-subagents' child pi runs (`--mode json
      # -p`, piped stdio) must never register as sibling sessions. A pi running
      # inside a conducted shell inherits AOIDE_SESSION_ID in this process's
      # env, and the spawned `aoide` inherits it from us — nesting threads
      # automatically.
      home.file.".pi/agent/extensions/aoide-pi-session.ts".text = ''
        import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
        import { spawn } from "node:child_process";

        // Pipe one canonical claude-shaped hook payload into the aoide door.
        // Detached + unref'd so a slow door can never hold pi open; a 10s
        // kill-timer keeps a stuck `aoide` from accumulating one child per
        // event.
        function emit(payload: Record<string, unknown>): void {
          const child = spawn(
            "aoide",
            ["graph", "session", "hook", "--agent", "pi"],
            { stdio: ["pipe", "ignore", "ignore"], detached: true },
          );
          child.unref();
          const timer = setTimeout(() => child.kill("SIGKILL"), 10_000);
          child.on("close", () => clearTimeout(timer));
          child.on("error", () => clearTimeout(timer));
          const stdin = child.stdin;
          if (stdin) {
            stdin.on("error", () => {}); // the door may exit before we write
            stdin.write(JSON.stringify(payload));
            stdin.end();
          }
        }

        // The fields every hook shares; transcript_path lets the aoide side
        // tail-read the live jsonl without re-deriving its location. pid is
        // THIS process's real pid — the aoide door prefers it over its own
        // terminal-ancestry walk, so a pi that dies inside a still-open
        // terminal still trips the reaper's /proc liveness signal (the
        // terminal's pid would outlive it and strand the record forever).
        // context_ceiling is the ACTIVE MODEL's context-window size straight
        // from pi's own model catalog (getContextUsage) — the aoide side
        // prefers it over its built-in catalog, so a custom/provider model
        // never gets a guessed ceiling. Absent when unknown (right after
        // compaction) — omitted, never zero.
        function base(ctx: ExtensionContext): Record<string, unknown> {
          const usage = ctx.getContextUsage();
          const ceiling = usage ? usage.contextWindow : undefined;
          return {
            session_id: ctx.sessionManager.getSessionId(),
            cwd: ctx.cwd,
            pid: process.pid,
            transcript_path: ctx.sessionManager.getSessionFile(),
            ...(ceiling ? { context_ceiling: ceiling } : {}),
          };
        }

        export default function (pi: ExtensionAPI) {
          // Session ids this process already reported, so a reload/resume
          // re-fire of the same session never double-reports a Start. A resume
          // of a DIFFERENT session (new id) is a live session — it reports.
          const reported = new Set<string>();
          const tui = (ctx: ExtensionContext) => ctx.mode === "tui";

          pi.on("session_start", (event, ctx) => {
            if (!tui(ctx)) return;
            const id = ctx.sessionManager.getSessionId();
            if (reported.has(id)) return;
            reported.add(id);
            emit({ hook_event_name: "SessionStart", ...base(ctx) });
          });

          // The interactive prompt is the UserPromptSubmit twin: it moves the
          // session to working and names it (set-once, aoide-side).
          pi.on("input", (event, ctx) => {
            if (!tui(ctx) || event.source !== "interactive") return;
            emit({ hook_event_name: "UserPromptSubmit", user_prompt: event.text, ...base(ctx) });
          });

          pi.on("tool_execution_start", (event, ctx) => {
            if (!tui(ctx)) return;
            emit({
              hook_event_name: "PreToolUse",
              tool_name: event.toolName,
              tool_use_id: event.toolCallId,
              ...base(ctx),
            });
          });

          pi.on("tool_execution_end", (event, ctx) => {
            if (!tui(ctx)) return;
            emit({
              hook_event_name: "PostToolUse",
              tool_name: event.toolName,
              tool_use_id: event.toolCallId,
              ...base(ctx),
            });
          });

          // The agent loop finished answering — the turn is over, back at the
          // prompt (claude's Stop).
          pi.on("agent_end", (_event, ctx) => {
            if (!tui(ctx)) return;
            emit({ hook_event_name: "Stop", ...base(ctx) });
          });

          // A real quit ends the session; reload/new/resume/fork swap sessions
          // instead (the successor reports its own Start). Killed processes
          // (SIGKILL/terminal close) never reach this — the aoide record then
          // leans on the reaper's pid/window signal, same pre-existing gap as
          // claude/kimi.
          pi.on("session_shutdown", (event, ctx) => {
            if (!tui(ctx) || event.reason !== "quit") return;
            emit({ hook_event_name: "SessionEnd", ...base(ctx) });
          });
        }
      '';
    };
  };
}
