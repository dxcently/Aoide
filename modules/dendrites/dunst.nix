# modules/dendrites/dunst.nix — the dunst notification daemon.
#
# Dendrite shape (CONTRACTS.md §2):
#   - no enable flag: selection is the switch; its body also needs the strillix fact
#   - carries its own dependencies: the dunst package (dunstctl rides along for
#     history and the pause levels), libnotify for the stock `notify-send`
#     client other aoide code already calls, and the daemon unit itself via
#     Home Manager's services.dunst module
#   - reads no other module. Note what is NOT read any more: config.aoide.theme.
#     dunst draws nothing now, so it has no dress and needs no design tokens.
#
# ── dunst is the daemon; it is not the face ───────────────────────────────
# dunst owns org.freedesktop.Notifications — the bus name, the spec, history,
# stacking, pause levels, and the rule engine. It does NOT draw. Every rule
# below sets `skip_display`, and a `script` hands the notification to
# `aoide notifications push`, which packs it over the shellbridge socket into
# state/stage/notifications.json (CONDUCTING, CONTRACTS.md §4), where the Quickshell
# notifications reads it and draws the real widget (song slot `notifications` for the
# popup, `notifications-center` for the dock ledger).
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
#     journal carried the proof ("Failed to load icon from path: …notifications-plate-
#     normal.png"). Cell arithmetic in markup is fragile by construction.
# QML has hit testing, layout, and the theme. So the face moved there and the
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
#   - DUNST_URGENCY is UPPERCASE (LOW/NORMAL/CRITICAL); the push command lowers it.
#   - there is no DUNST_ACTIONS. A third-party sender's action labels never
#     reach us, so the notifications offers no buttons for them. This costs nothing for
#     the case that matters: an aoide permission summons is published to the
#     stage file by `aoide session permit` itself, which knows its own verdicts.
# And one consequence worth stating plainly: a notification that is never
# displayed is never "displayed", so it does not expire — dunst drops it
# straight into history. TIMEOUT rides in the record and the QML notifications owns
# the dismiss clock. That is why no timeout is configured here.
#
# Select it with one line in hosts/yomi-strix/default.nix:
#   habit.dendrites.dunst.enable = true;
#
# hosts/ knows dendrites; dendrites never know hosts.
#
# ── P-A8: also gated on aoide.strillix.enable ──────────────────────────────────
# The notifications-feed rule below hands every notification to `strillix notifications push`
# (pkgs.aoide.strillix — a SEPARATE, droppable output as of P-A8's multi-output
# split); that push is this whole module's reason to exist, so a config that
# leaves dunst on but turns strillix off would otherwise script an exec against a
# binary no longer in the closure. Unlike shellbridge (gated on the shell
# and strillix enable facts), dunst carries no shell dependency of its own — it
# is gated here directly on the strillix flag instead.

{
  config,
  lib,
  pkgs,
  ...
}:

{

  config = lib.mkIf config.aoide.strillix.enable {
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
    habit.home.services.dunst = {
      enable = true;
      settings = {
        global = {
          # Nothing here is a dress — dunst never draws. These are the daemon
          # behaviours the notifications reads through, and nothing else.

          # Sender text is DATA. Never parsed as markup, on either side of the
          # seam: dunst hands it to the script verbatim, and the QML notifications
          # renders it as plain text. Untrusted text is never re-interpreted.
          markup = "no";

          # Duplicate stacking is the daemon's job and stays on; the notifications
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
        # The script is `aoide notifications push`, which reads the DUNST_* environment
        # and writes the record over $AOIDE_BRIDGE_SOCKET. It is a real command and
        # not a shell wrapper on purpose: notification bodies are arbitrary
        # sender text, and hand-rolled shell JSON escaping is a bug farm.
        #
        # `script` takes ONE executable, not a command line — dunst appends its
        # own five positional arguments (appname summary body icon urgency), so
        # the command's arguments cannot be written here. Hence the wrapper. Its
        # store path is absolute on purpose: dunst resolves a bare name through
        # PATH, and the HM user unit's PATH is not ours to rely on. The command
        # reads the DUNST_* environment, so the positional arguments dunst adds
        # are ignored.
        notifications-feed = {
          summary = ".*";
          skip_display = true;
          always_run_script = true;
          # `notifications` left core's registry at P-A5 of the binary-split
          # workstream — it now lives only in `strillix` (crates/strillix/src/
          # registry.rs). Both binaries ship from the same `pkgs.aoide`
          # derivation (P-A7), but as of P-A8 `strillix` lives in that
          # derivation's separate `strillix` output (`pkgs.aoide.strillix`) — named
          # explicitly here, same reasoning as shellbridge.nix's ExecStart.
          script = "${pkgs.writeShellScript "aoide-notifications-push" ''
            exec ${pkgs.aoide.strillix}/bin/strillix notifications push
          ''}";
        };
      };
    };
  };
}
