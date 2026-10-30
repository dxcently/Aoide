# modules/dendrites/strillix/lock.nix — the `lock` slot's strillix provider: the
# Quickshell lock (LockHost.qml) on the strillix runtime, which it brings by path
# (README.md), with the hyprlock fallback a rice without a `lockscreen` face
# locks with. The declared lock dress (`$STRILLIX_ROOT/declared/hyprlock.conf`) is
# staging data and is seeded by the runtime, not here.
{ config, pkgs, ... }:
let
  claim = {
    components.lock.provider = "strillix";
    surfaces.lockscreen.owner = "quickshell";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];

  # ── The lock (`strillix lock`) ──────────────────────────────────────────────
  # `strillix lock` runs one of two lockers, and this provider carries both and
  # their PAM services. A rice with a `lockscreen` face is
  # locked by the shell's LockHost.qml, whose one PamContext authenticates
  # through `aoide-lock`: a service of its own rather than `login`, whose
  # stack carries session and tty modules (lastlog, motd, the greeter's
  # own additions) that have no business in a re-authentication, and a
  # name a host can tune for the lock alone (a fingerprint reader, a key).
  # Every other rice runs hyprlock through `hyprlock`, the service name it
  # opens. The package goes in directly (systemPackages below), not through
  # `programs.hyprlock.enable`, which would also switch on a config-less
  # hypridle this provider does not want to own.
  security.pam.services.aoide-lock = { };
  security.pam.services.hyprlock = { };

  # hyprlock's system-wide config (`$XDG_CONFIG_DIRS`, i.e. /etc/xdg), the
  # one it reads when the user has no `~/.config/hypr/hyprlock.conf` of
  # their own; a host that writes one keeps it. It defines the ten
  # `$aoide_lock_*` variables with the plain look as their values, sources
  # the declared and then the staged dress (CONTRACTS.md §4, the lock
  # dress), and only then draws, so the staged rice wins and a missing or
  # broken fragment costs the dress and never the lock. A variable is read
  # by a paint key only, never by a text key, which hyprlock can run as a
  # command. The plain values are hyprlock 0.9.6's own field defaults.
  environment.etc."xdg/hypr/hyprlock.conf".text = ''
    general {
        hide_cursor = false
    }

    $aoide_lock_color = rgba(20, 20, 30, 1.0)
    $aoide_lock_image =
    $aoide_lock_field_y = 0
    $aoide_lock_field_outer = 0xFF111111
    $aoide_lock_field_inner = 0xFFDDDDDD
    $aoide_lock_field_font = 0xFF000000
    $aoide_lock_field_check = 0xFF22CC88
    $aoide_lock_field_fail = 0xFFCC2222
    $aoide_lock_name_y = -200%
    $aoide_lock_name_font = 0x00000000

    source = ${config.strillix.root}/declared/hyprlock.conf
    source = ${config.strillix.root}/stage/hyprlock.conf

    background {
        color = $aoide_lock_color
        path = $aoide_lock_image
    }

    input-field {
        size = 400, 60
        position = 0, $aoide_lock_field_y
        halign = center
        valign = center
        outline_thickness = 3
        dots_size = 0.25
        dots_spacing = 0.3
        dots_center = true
        fade_on_empty = false
        placeholder_text = password
        fail_text = failed ($ATTEMPTS)
        outer_color = $aoide_lock_field_outer
        inner_color = $aoide_lock_field_inner
        font_color = $aoide_lock_field_font
        check_color = $aoide_lock_field_check
        fail_color = $aoide_lock_field_fail
        capslock_color = rgb(220, 120, 60)
        numlock_color = rgb(60, 160, 220)
    }

    label {
        text = $LAYOUT
        position = 0, -80
        halign = center
        valign = center
        font_size = 16
    }

    label {
        text = $USER
        position = 0, $aoide_lock_name_y
        color = $aoide_lock_name_font
        halign = center
        valign = center
        font_size = 20
        shadow_passes = 2
    }
  '';

  environment.systemPackages = [ pkgs.hyprlock ];
  strillix = claim;
}
