# modules/dendrites/dunst.nix — the dunst notification daemon.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - one enable toggle: aoide.dunst.enable (off by default)
#   - carries its own dependencies: the dunst package (dunstctl rides along for
#     history and the pause levels), libnotify for the stock `notify-send`
#     client other aoide code already calls, and the daemon unit itself via
#     Home Manager's services.dunst module
#   - reads no other module. Note what is NOT read any more: config.aoide.livery.
#     dunst draws nothing now, so it has no dress and needs no design tokens.
#
# ── dunst is the daemon; it is not the face ───────────────────────────────
# dunst owns org.freedesktop.Notifications — the bus name, the spec, history,
# stacking, pause levels, and the rule engine. It does NOT draw. Every rule
# below sets `skip_display`, and a `script` hands the notification to
# `aoide herald push`, which packs it over the shellbridge socket into
# song/stage/herald.json, where the Quickshell herald reads it and draws the
# real widget (song slot `herald` for the popup, `herald-center` for the dock
# ledger).
#
# Why: the previous dunstrc carried the entire card as Pango markup in `format`
# strings. That ceiling was structural, not cosmetic —
#   - dunst has no per-region hit testing, so a drawn `[deny]` chip could only
#     ever fire the window-wide left-click binding, which approved. A permission
#     gate whose deny button approves is the defect this whole handover exists
#     to kill.
#   - an image can only land in dunst's own icon column, outside the text band:
#     nothing can be drawn INSIDE the frame.
#   - the progress bar was dunst's, styled by dunst's knobs, not the rig's.
#   - the layout was load-bearing on generated `default_icon` plates that were
#     not always in the store; when one failed to resolve the column collapsed
#     and every measured run in that rule ended ~43px short. The daemon's own
#     journal carried the proof ("Failed to load icon from path: …herald-plate-
#     normal.png"). Cell arithmetic in markup is fragile by construction.
# QML has hit testing, layout, and the livery. So the face moved there and the
# markup is gone — not tuned, gone.
#
# ── the feed, proven live before it was written down (2026-08-17) ─────────
# Reloaded a live dunst 1.13.2 onto a probe config carrying exactly the rule
# below, then fired four notifications:
#   - the script fired for all four                     (skip_display does not
#     suppress it; `always_run_script` is set anyway, which covers the separate
#     `format = ""` suppression case)
#   - `dunstctl count displayed` stayed 0               (dunst drew nothing)
#   - `dunstctl count history` counted all of them      (history intact)
#   - DUNST_PROGRESS arrived as `40` when the sender set a value, `-1` when not
#   - DUNST_ICON_PATH arrived as a full resolved path
#   - DUNST_CATEGORY carried `x-aoide.permission` through
# Two things the env does NOT carry, learned the same way, both handled on the
# aoide side rather than papered over here:
#   - DUNST_URGENCY is UPPERCASE (LOW/NORMAL/CRITICAL); the push verb lowers it.
#   - there is no DUNST_ACTIONS. A third-party sender's action labels never
#     reach us, so the herald offers no buttons for them. This costs nothing for
#     the case that matters: an aoide permission summons is published to the
#     stage file by `aoide graph permit` itself, which knows its own verdicts.
# And one consequence worth stating plainly: a notification that is never
# displayed is never "displayed", so it does not expire — dunst drops it
# straight into history. TIMEOUT rides in the record and the QML herald owns
# the dismiss clock. That is why no timeout is configured here.
#
# Enable with one line in hosts/yomi-strix/default.nix:
#   aoide.dunst.enable = true;
#
# hosts/ knows dendrites; dendrites never know hosts.
{
  config,
  lib,
  pkgs,
  ...
}:

{
  options.aoide.dunst.enable = lib.mkEnableOption "dunst notification daemon (the herald's delivery backend)";

  config = lib.mkIf config.aoide.dunst.enable {
    # libnotify rides along for `notify-send`: the stock client the freedesktop
    # world (and aoide's own shellbridge mode-toggle path) reaches for. dunstify
    # alone left it missing on the box.
    environment.systemPackages = [
      pkgs.dunst
      pkgs.libnotify
    ];

    # HM's module owns the whole daemon lifecycle: dunstrc generation from
    # `settings`, a Type=dbus unit holding BusName=org.freedesktop.Notifications
    # bound to the graphical session, D-Bus activation file, and a dunstrc
    # reload trigger. Hand-rolling that (clipboard.nix style) would buy nothing.
    home-manager.users.${config.aoide.user}.services.dunst = {
      enable = true;
      settings = {
        global = {
          # Nothing here is a dress — dunst never draws. These are the daemon
          # behaviours the herald reads through, and nothing else.

          # Sender text is DATA. Never parsed as markup, on either side of the
          # seam: dunst hands it to the script verbatim, and the QML herald
          # renders it as plain text. Untrusted text is never re-interpreted.
          markup = "no";

          # Duplicate stacking is the daemon's job and stays on; the herald
          # ledger reads one entry per stack.
          stack_duplicates = true;
          hide_duplicate_count = false;

          # The window the dock ledger keeps. The stage file is the live view;
          # this is dunst's own record behind it (`dunstctl history`).
          history_length = 20;

          # ── quiet hours ──────────────────────────────────────────────────
          # `dunstctl set-paused true` (level 100) is total silence. Level 60 is
          # the useful middle: ordinary toasts hold, critical (70) and the
          # permission summons (90) still come through. Pause is enforced by the
          # daemon BEFORE the script runs, which is precisely why quiet hours
          # still work with the face moved to QML.
          default_pause_level = 0;

          # The feed rule's catch-all matcher is a real regex, not a glob.
          enable_posix_regex = true;
        };

        # ── the feed ───────────────────────────────────────────────────────
        # One catch-all rule: draw nothing, hand everything to aoide. `summary`
        # is the matcher because every notification has one; `.*` matches all of
        # them (posix regex is on above). `always_run_script` guarantees the
        # script still runs for a notification dunst would otherwise consider
        # suppressed.
        #
        # The script is `aoide herald push`, which reads the DUNST_* environment
        # and writes the record over $AOIDE_BRIDGE_SOCKET. It is a real verb and
        # not a shell wrapper on purpose: notification bodies are arbitrary
        # sender text, and hand-rolled shell JSON escaping is a bug farm.
        #
        # `script` takes ONE executable, not a command line — dunst appends its
        # own five positional arguments (appname summary body icon urgency), so
        # the verb's arguments cannot be written here. Hence the wrapper. Its
        # store path is absolute on purpose: dunst resolves a bare name through
        # PATH, and the HM user unit's PATH is not ours to rely on. The verb
        # reads the DUNST_* environment, so the positional arguments dunst adds
        # are ignored.
        herald-feed = {
          summary = ".*";
          skip_display = true;
          always_run_script = true;
          # `herald` left core's registry at P-A5 of the binary-split
          # workstream — it now lives only in `lyra` (crates/lyra/src/
          # registry.rs). Both binaries ship from the same `pkgs.aoide`
          # derivation (P-A7), so this is still a plain sibling store path.
          script = "${pkgs.writeShellScript "aoide-herald-push" ''
            exec ${pkgs.aoide}/bin/lyra herald push
          ''}";
        };
      };
    };
  };
}
