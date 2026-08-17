# modules/dendrites/hyprland.nix — host-invariant Hyprland behaviour.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.hyprland.enable (default false — shipped but off;
#     enabled per host beside the compositor facet).
#   - Reads no other module: it writes only the stock home-manager option
#     `wayland.windowManager.hyprland.extraConfig`, plus the shared
#     `config.aoide.user` from the nucleus option contract.
#
# ── The split (why this file exists) ──────────────────────────────────────────
# Hyprland config divides cleanly in two, and the halves change for completely
# different reasons:
#
#   LOOK / SHAPE  → modules/facets/compositor/default.nix
#     Anything derived from `aoide.livery`: border colours, gaps, rounding,
#     blur, the aoide-* layerrules, hyprglass, the kitty opacity/rounding
#     rules. Re-riced whenever the song changes. That file also owns the
#     session plumbing (programs.hyprland, the systemd/Wayland env handoff,
#     hyprpolkitagent, xdg.portal, greetd).
#
#   BEHAVIOUR     → THIS FILE
#     What you want no matter what the desktop looks like: keybinds, input
#     devices, tiling layout, misc quality-of-life, and behavioural window
#     rules (float/workspace-assignment/idle — as opposed to the *appearance*
#     window rules, which are livery's business and stay in the facet).
#     Swapping songs must never disturb any of it.
#
# A facet MAY read `aoide.livery` and is a render surface (CONTRACTS.md §2);
# this module reads no livery and renders nothing, so it is a dendrite — the
# same shape these binds had in dxflake before the port.
#
# ── Ordering inside hyprland.conf ─────────────────────────────────────────────
# Several modules contribute to one `extraConfig` (a `lines` option, so they
# concatenate by merge order, NOT by import order):
#   500  mkBefore  compositor facet    — livery values first
#   1000 (default) THIS FILE           — behaviour
#   1000 (default) screenshot dendrite — its own SUPER+S binds
# Plain (unordered) is deliberate: the livery block must land first so these
# behaviour keys are never overwritten by it. (The quickshell facet is NOT a
# writer here — it autostarts the shell as a systemd user service, not an
# exec-once, so nothing of its lands in hyprland.conf.)
#
# Enabling this without the compositor facet is harmless: home-manager only
# writes hyprland.conf when its own hyprland module is enabled, so the text
# below is simply never emitted.
{
  config,
  lib,
  ...
}:
let
  cfg = config.aoide.hyprland;
in
{
  options.aoide.hyprland.enable = lib.mkEnableOption "host-invariant Hyprland behaviour (keybinds, input, layout, window rules)";

  config = lib.mkIf cfg.enable {
    home-manager.users.${config.aoide.user} = {
      wayland.windowManager.hyprland.extraConfig = ''
        # ── Aoide behaviour — hyprland dendrite ───────────────────────────────
        # Look/shape lives in the compositor facet; everything here survives a
        # re-rice untouched.

        # ── Input devices ─────────────────────────────────────────────────────
        # Ported from dxflake's hyprland dendrite (the user's reference rig) so
        # the two boxes feel identical under the hand. `compose:caps` puts the
        # compose key on caps-lock. accel_profile/force_no_accel disable pointer
        # acceleration outright — a deliberate flat-mouse preference, not a
        # default; tune or drop these two lines freely, nothing else reads them.
        input {
            kb_layout = us
            kb_options = compose:caps
            follow_mouse = 1
            sensitivity = 0.8
            accel_profile = flat
            force_no_accel = true

            touchpad {
                natural_scroll = true
                middle_button_emulation = true
                clickfinger_behavior = true
            }
        }

        # ── Tiling layout ─────────────────────────────────────────────────────
        # `layout` is behaviour, so it lives here — while the sibling
        # general{} keys the facet writes (gaps, border_size, col.*_border) are
        # livery-derived look. hyprlang merges repeated sections, so both
        # blocks coexist; keep the split by KEY, not by section name.
        general {
            layout = dwindle
        }

        dwindle {
            preserve_split = true
        }

        master {
            new_status = master
        }

        # ── Misc quality-of-life ──────────────────────────────────────────────
        # force_default_wallpaper = -1 kills Hyprland's built-in wallpaper: the
        # Aoide wallpaper is a quickshell layer surface (AoideWallpaper.qml) and
        # is the sole painter, so Hyprland must not draw its own underneath.
        misc {
            force_default_wallpaper = -1
            disable_hyprland_logo = true
        }

        ecosystem {
            no_update_news = true
            no_donation_nag = true
        }

        # ── Behavioural window rules ──────────────────────────────────────────
        # The seam for rules that are about BEHAVIOUR — float this dialog, pin
        # that app to a workspace, inhibit idle while fullscreen. Deliberately
        # empty: the rules currently shipped (kitty opacity/rounding) are
        # APPEARANCE and correctly live in the compositor facet, and dxflake's
        # rules were workspace assignments for apps Aoide does not ship
        # (vesktop, steam, strawberry), so importing them would bind nothing.
        #
        # Add them here, not in the facet — a re-rice must not move a window.
        # Hyprland 0.56 matches with the `match:<prop> <value>` form; the old
        # `class:^(foo)$` form is rejected. Example:
        #   windowrule = float, match:class pavucontrol
        #   windowrule = workspace 2, match:class thunderbird

        # ── Keybinds ──────────────────────────────────────────────────────────
        # Launcher (Hyprland global shortcut → AoideLauncher.GlobalShortcut
        # toggle). The launcher registers `aoide:launcher` in-process (hyprland-
        # global-shortcuts-v1), so the keypress reaches the running surface
        # directly — no `aoide shell launcher` CLI verb (that verb is an
        # unimplemented stub) and no inbound socket. `global, <appid>:<name>` is
        # Hyprland's dispatcher for it.
        bind = SUPER, SPACE, global, aoide:launcher

        # Wallpaper switcher (Hyprland global shortcut → AoideWallpaperPicker.
        # GlobalShortcut toggle). Registers `aoide:wallpaper` in-process, same
        # global-shortcuts-v1 seam as the launcher — one press summons the cover
        # grid over song/covers/, a pick shells `aoide cover set <path>` which
        # hot-swaps the live wallpaper. No CLI toggle verb, no inbound socket.
        bind = SUPER, W, global, aoide:wallpaper

        # Lock screen
        bind = SUPER, ESCAPE, exec, aoide shell lock

        # Panel dock (Hyprland global shortcut → the center-left panel's
        # GlobalShortcut toggle). SUPER+G opens/closes the center-left book-edge
        # dock that holds the widget sub-panels (conductor, terminals, meters,
        # power). It registers `aoide:dock` in-process (same global-shortcuts-v1
        # seam as the launcher/wallpaper) — no CLI verb (the old `aoide shell
        # dock` was an unimplemented stub), no inbound socket. The dock also
        # peeks out on its own as an alert when an agent needs a response.
        bind = SUPER, G, global, aoide:dock

        # Rice preview / adopt shortcuts
        bind = SUPER SHIFT, P, exec, aoide rice preview
        bind = SUPER SHIFT, A, exec, aoide rice adopt

        # ── Window management (ported from dxflake hyprland dendrite) ────────
        # Normalized to SUPER, matching the Aoide binds above. dxflake exec
        # binds for tools Aoide doesn't ship (rofi, thunar, cliphist,
        # vesktop/discord, gpu-screen-recorder) are dropped — the launcher and
        # the bar's power cell cover those seams. The hyprshot/satty binds
        # (SUPER+S / SUPER SHIFT+S) live in the screenshot dendrite, shipped
        # WITH the tools (modules/dendrites/screenshot.nix). dxflake's
        # media/brightness XF86 keys are also deliberately NOT bound (strict
        # window-management scope; the bar's volume cell owns audio by mouse) —
        # a known seam if hardware keys are wanted later.

        # Terminal (the kitty dendrite ships kitty)
        bind = SUPER, RETURN, exec, kitty

        # Window controls
        bind = SUPER, Q, killactive
        # Clipboard history — opens the Grimoire launcher on the clipboard
        # chapter. The clipboard backend (AoideClipboard.qml) is a non-visual
        # data provider that feeds cliphist entries into the launcher as
        # paginated chapters; no separate picker window.
        bind = SUPER, C, global, aoide:clipboard
        bind = SUPER, V, togglefloating
        bind = SUPER, F, fullscreen

        # Focus movement — arrows carry the full left/down/up/right set; H/J/K/L
        # complete the vim set (left/down/up/right). dxflake's SUPER+L was
        # blocked here by the Aoide lock bind, so the lock moved to SUPER+ESCAPE
        # above, freeing L — the full hjkl set is now live, matching arrows.
        bind = SUPER, left, movefocus, l
        bind = SUPER, down, movefocus, d
        bind = SUPER, up, movefocus, u
        bind = SUPER, right, movefocus, r
        bind = SUPER, H, movefocus, l
        bind = SUPER, J, movefocus, d
        bind = SUPER, K, movefocus, u
        bind = SUPER, L, movefocus, r

        # Move window (same directional scheme)
        bind = SUPER SHIFT, left, movewindow, l
        bind = SUPER SHIFT, down, movewindow, d
        bind = SUPER SHIFT, up, movewindow, u
        bind = SUPER SHIFT, right, movewindow, r
        bind = SUPER SHIFT, H, movewindow, l
        bind = SUPER SHIFT, J, movewindow, d
        bind = SUPER SHIFT, K, movewindow, u
        bind = SUPER SHIFT, L, movewindow, r

        # Resize — binde repeats while held (dxflake's step sizes)
        binde = SUPER ALT, left, resizeactive, -20 0
        binde = SUPER ALT, down, resizeactive, 0 40
        binde = SUPER ALT, up, resizeactive, 0 -40
        binde = SUPER ALT, right, resizeactive, 20 0
        binde = SUPER ALT, H, resizeactive, -20 0
        binde = SUPER ALT, J, resizeactive, 0 40
        binde = SUPER ALT, K, resizeactive, 0 -40

        # Workspaces 1–10 — pairs with the bar's musical workspace glyphs
        bind = SUPER, 1, workspace, 1
        bind = SUPER, 2, workspace, 2
        bind = SUPER, 3, workspace, 3
        bind = SUPER, 4, workspace, 4
        bind = SUPER, 5, workspace, 5
        bind = SUPER, 6, workspace, 6
        bind = SUPER, 7, workspace, 7
        bind = SUPER, 8, workspace, 8
        bind = SUPER, 9, workspace, 9
        bind = SUPER, 0, workspace, 10
        bind = SUPER SHIFT, 1, movetoworkspace, 1
        bind = SUPER SHIFT, 2, movetoworkspace, 2
        bind = SUPER SHIFT, 3, movetoworkspace, 3
        bind = SUPER SHIFT, 4, movetoworkspace, 4
        bind = SUPER SHIFT, 5, movetoworkspace, 5
        bind = SUPER SHIFT, 6, movetoworkspace, 6
        bind = SUPER SHIFT, 7, movetoworkspace, 7
        bind = SUPER SHIFT, 8, movetoworkspace, 8
        bind = SUPER SHIFT, 9, movetoworkspace, 9
        bind = SUPER SHIFT, 0, movetoworkspace, 10
        bind = ALT, Tab, workspace, previous

        # Special workspaces — the bar's icon map carries magic and scratch
        bind = SUPER, X, togglespecialworkspace, magic
        bind = SUPER, Z, togglespecialworkspace, scratch
        bind = SUPER SHIFT, X, movetoworkspace, special:magic
        bind = SUPER SHIFT, Z, movetoworkspace, special:scratch

        # Mouse — SUPER+leftdrag move, SUPER+rightdrag resize
        bindm = SUPER, mouse:272, movewindow
        bindm = SUPER, mouse:273, resizewindow
      '';
    };
  };
}
