# tests/vm-lock-greeter.nix — NixOS integration test: the login screen and the
# screen lock, end to end, on a graphical VM.
#
# Two machines, each assembled through the real constructor (habit's
# `composition.mkNixosModules`, as lib/aoideos.nix does) over the fixture
# songbook in `tests/lock-greeter/songbook/`, with the paint lanes selected
# (compositor with its hyprland provider, the hyprland behaviour lane, greeter,
# quickshell, lyra):
#
#   face   declares `aubade`, which ships `widgets/greeter.qml` and
#          `widgets/lockscreen.qml`: greetd + the Quickshell greeter at boot,
#          and `lyra lock` locks through the shell's LockHost.
#   plain  declares `plainsong`, which ships no face: ly at boot, and
#          `lyra lock` runs hyprlock.
#
# The song hook is this file's own copy of lib/aoideos.nix's, over the fixture
# songbook instead of the committed one (the same wiring tests/consumer does
# for its own songbook).
#
# Screenshots land in the driver's output directory (`$out` under
# `nix build`, `-o` when the driver runs on its own).
#
# Wired in flake.nix as:
#   checks.<system>.vm-lock-greeter = import ./tests/vm-lock-greeter.nix { inherit pkgs inputs lib; };
{
  pkgs,
  inputs,
  lib,
  system ? "x86_64-linux",
}:
let
  composition = inputs.habit.lib.composition { inherit lib; };

  overlay = (import ../lib/pkgs.nix { inherit lib; }).overlay {
    stock = inputs.nixpkgs.legacyPackages.${system};
  };

  aoideos = import ../lib/aoideos.nix {
    inherit inputs lib system;
    username = "khoa";
  };

  songbookRoot = ./lock-greeter/songbook;
  songbook = import ../lib/songbook.nix {
    inherit lib;
    songbook = songbookRoot;
  };

  hostFor = song: {
    _file = toString ./vm-lock-greeter.nix;

    habit.dendrites.compositor = {
      enable = true;
      provider = "hyprland";
    };
    # The behaviour lane: the SUPER+ESCAPE bind and the session-lock restore.
    habit.dendrites.hyprland.enable = true;
    habit.dendrites.greeter.enable = true;
    habit.dendrites.quickshell.enable = true;
    habit.dendrites.lyra.enable = true;

    habit.dendrites.aoide-nix.enable = true;
    habit.dendrites.aoide-packages.enable = true;
    habit.dendrites.aoided.enable = true;

    habit.song.declared = song;

    habit.users.khoa = {
      definition = ./vm-boot-user.nix;
      home.enable = true;
    };
  };

  resolve =
    song:
    composition.mkNixosModules {
      hostName = "vm-${song}";
      registry = import ../modules;
      host = hostFor song;
      extraModules = [ aoideos.nucleusModule ];
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      overlays = [ overlay ];
      selectionModules = [ songbook.selectionModule ];
      extraModulesFor =
        sel:
        let
          checked = songbook.check {
            inherit (sel) song;
            lyra = sel.dendrites.lyra.enable;
          };
        in
        songbook.songModules checked
        ++ [
          {
            _module.args = {
              inherit (songbook) song borrow;
              songbook = songbookRoot;
            };
            aoide.song = checked.declared;
            aoide.songbook.builtIn = songbook.builtIn checked;
            aoide.songbook.greeter = songbook.greeter checked;
          }
        ];
      specialArgs = {
        username = "khoa";
      };
      inherit system;
    };

  face = resolve "aubade";
  plain = resolve "plainsong";

  machine = resolved: {
    imports = resolved.modules ++ [
      {
        aoide.enable = true;
        aoide.user = "khoa";
        nixpkgs.hostPlatform = system;

        environment.systemPackages = [ pkgs.jq ];

        # A real GPU device, so cage and Hyprland have a DRM node to paint on.
        virtualisation.qemu.options = [
          "-vga none"
          "-device virtio-gpu-pci,xres=1280,yres=800"
        ];
        virtualisation.memorySize = 4096;
        virtualisation.cores = 4;
      }
    ];
  };
in
pkgs.testers.runNixOSTest {
  name = "aoide-vm-lock-greeter";

  # Both machines are assembled by the same constructor with the same
  # specialArgs, so one node-wide value serves them.
  node.specialArgs = face.specialArgs;
  node.pkgsReadOnly = false;

  nodes.face = machine face;
  nodes.plain = machine plain;

  testScript = ''
    import json
    import shlex

    def uid(m):
        return m.succeed("id -u khoa").strip()

    # A command as khoa inside the graphical session's environment: its
    # runtime dir, bus, Wayland display and Hyprland instance. `quickshell ipc`
    # only reaches instances on the caller's display.
    def khoa_cmd(m, cmd):
        u = uid(m)
        env = (
            f"XDG_RUNTIME_DIR=/run/user/{u} "
            f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{u}/bus "
            f"WAYLAND_DISPLAY=$(basename $(ls /run/user/{u}/wayland-* | grep -v lock | head -n1)) "
            "HYPRLAND_INSTANCE_SIGNATURE=$(ls /run/user/" + u + "/hypr | head -n1)"
        )
        return f"su - khoa -c {shlex.quote(env + ' ' + cmd)}"

    def as_khoa(m, cmd):
        return m.succeed(khoa_cmd(m, cmd))

    # The display session the way the popup watchers find it: from outside
    # every session (no XDG_SESSION_ID), the user's Display session.
    def display_session(m):
        return m.succeed("env -u XDG_SESSION_ID loginctl show-user khoa -p Display --value").strip()

    def locked_hint(m):
        return m.succeed(f"loginctl show-session {display_session(m)} -p LockedHint --value").strip()

    def wait_hint(m, want):
        m.wait_until_succeeds(f"test \"$(loginctl show-session {display_session(m)} -p LockedHint --value)\" = {want}", timeout=30)

    def pam_failures(m, service):
        return int(m.succeed(f"journalctl -b --no-pager | grep -c 'pam_unix({service}:auth): authentication failure' || true").strip())

    def lyra_lock(m):
        out = as_khoa(m, "lyra lock --json")
        return json.loads(out)

    def wait_shell(m, secure="false"):
        m.wait_until_succeeds(
            khoa_cmd(m, "quickshell -p $HOME/.aoide/run/qml/shell.qml ipc call lock secure") + f" | grep -qx {secure}",
            timeout=90,
        )

    # Swap the aubade lock face for a fixture, restart the shell, lock.
    def lock_with_face(m, fixture):
        m.succeed(
            "f=/home/khoa/.aoide/run/qml/songs/aubade/lockscreen.qml; "
            f"chmod u+w $(dirname $f); rm -f $f; cp {fixture} $f; chown khoa $f"
        )
        as_khoa(m, "systemctl --user restart aoide-quickshell.service")
        wait_shell(m)
        o = lyra_lock(m)
        assert o["data"]["locker"] == "shell", f"lyra lock used {o}"
        wait_hint(m, "yes")

    start_all()

    # ══ face: greetd + the Quickshell greeter ══════════════════════════════
    with subtest("face: the login screen is the Quickshell greeter"):
        face.wait_for_unit("greetd.service")
        face.wait_until_succeeds("ps -u greeter -o args= | grep -q /bin/cage", timeout=90)
        face.wait_until_succeeds("ps -u greeter -o args= | grep -q 'quickshell.*aoide-greeter-tree'", timeout=90)
        face.fail("systemctl is-active ly.service")
        # The face has read passwd, picked the one account and asked greetd
        # for its password prompt; give the frame a moment to paint.
        face.sleep(8)
        face.screenshot("01-greeter")

    with subtest("face: the greeter refuses a wrong password"):
        before = pam_failures(face, "greetd")
        face.send_chars("wrong-password\n")
        face.wait_until_succeeds(f"test $(journalctl -b --no-pager | grep -c 'pam_unix(greetd:auth): authentication failure') -gt {before}", timeout=30)
        face.sleep(3)
        face.fail("loginctl list-sessions --no-legend | grep -w khoa")
        face.screenshot("02-greeter-refused")

    with subtest("face: the right password starts a user-class session"):
        face.send_chars("test\n")
        face.wait_until_succeeds("loginctl list-sessions --no-legend | grep -w khoa", timeout=60)
        session = display_session(face)
        cls = face.succeed(f"loginctl show-session {session} -p Class --value").strip()
        assert cls == "user", f"khoa's session is class {cls!r}, not 'user'"
        face.wait_until_succeeds("ps -u khoa -o args= | grep -Eq '(^|/)Hyprland( |$)'", timeout=60)
        wait_shell(face)
        face.sleep(5)
        face.screenshot("03-desktop")

    # ══ face: lyra lock through the shell ═════════════════════════════════
    with subtest("face: lyra lock locks through the shell and sets LockedHint"):
        o = lyra_lock(face)
        assert o["data"]["locker"] == "shell", f"lyra lock used {o}"
        wait_hint(face, "yes")
        # The secrets/pairing popup watchers' locked check, read the way they
        # read it: from outside every session, through the Display session.
        assert locked_hint(face) == "yes"
        face.sleep(3)
        face.screenshot("04-lock-face")

    with subtest("face: Ctrl+Alt+Del reaches the lock surface"):
        face.send_key("ctrl-alt-delete")
        face.wait_until_succeeds("journalctl -b --no-pager | grep -q 'ctrl+alt+delete reached the lock face'", timeout=15)
        assert locked_hint(face) == "yes"

    with subtest("face: the lock refuses a wrong password, then unlocks"):
        before = pam_failures(face, "aoide-lock")
        face.send_chars("wrong-password\n")
        face.wait_until_succeeds(f"test $(journalctl -b --no-pager | grep -c 'pam_unix(aoide-lock:auth): authentication failure') -gt {before}", timeout=30)
        face.sleep(2)
        assert locked_hint(face) == "yes", "a wrong password unlocked the session"
        face.screenshot("05-lock-refused")
        face.send_chars("test\n")
        wait_hint(face, "no")
        face.sleep(2)
        face.screenshot("06-unlocked")

    with subtest("face: SUPER+ESCAPE locks through lyra lock"):
        face.send_key("meta_l-esc")
        wait_hint(face, "yes")
        face.sleep(2)
        face.send_chars("test\n")
        wait_hint(face, "no")

    with subtest("face: a crashed shell leaves the session locked, and the restarted shell takes it back"):
        o = lyra_lock(face)
        assert o["data"]["locker"] == "shell", f"lyra lock used {o}"
        wait_hint(face, "yes")
        pid = as_khoa(face, "systemctl --user show -p MainPID --value aoide-quickshell.service").strip()
        face.succeed(f"kill -KILL {pid}")
        face.sleep(2)
        face.screenshot("07-shell-crashed-while-locked")
        assert locked_hint(face) == "yes", "the hint dropped with the shell"
        # Restart=always brings the shell back; it reads LockedHint=yes at
        # start and locks on its own — no `lyra lock` here.
        wait_shell(face, secure="true")
        face.succeed("journalctl -b --no-pager | grep -q 'LockedHint is still yes; taking the lock back'")
        assert locked_hint(face) == "yes"
        face.sleep(3)
        face.screenshot("08-relocked")
        face.send_chars("test\n")
        wait_hint(face, "no")
        face.sleep(2)
        face.screenshot("08b-relock-unlocked")

    with subtest("face: a face that fails to load leaves the floor, which unlocks"):
        lock_with_face(face, "${./lock-greeter/broken-lockscreen.qml}")
        face.sleep(3)
        face.screenshot("09-floor")
        face.succeed("journalctl -b --no-pager | grep -q 'the face failed to load'")
        face.send_chars("test\n")
        wait_hint(face, "no")

    with subtest("face: a face that never reports ready gives way to the floor, which unlocks"):
        # Every aubade lock so far stood well past the wait: a face that
        # reports ready is never taken down.
        face.fail("journalctl -b --no-pager | grep -q 'the face did not report ready'")
        lock_with_face(face, "${./lock-greeter/throwing-lockscreen.qml}")
        face.wait_until_succeeds("journalctl -b --no-pager | grep -q 'the face did not report ready'", timeout=15)
        face.sleep(2)
        face.screenshot("09b-floor-after-unready-face")
        # The throwing face has no field: only the floor can take this.
        face.send_chars("test\n")
        wait_hint(face, "no")

    # ══ plain: ly + hyprlock ═══════════════════════════════════════════════
    with subtest("plain: the login screen is ly"):
        plain.wait_for_unit("display-manager.service")
        plain.fail("systemctl is-active greetd.service")
        plain.succeed("systemctl cat display-manager.service | grep -q ly")
        plain.sleep(5)
        plain.screenshot("10-ly")

    with subtest("plain: lyra lock runs hyprlock"):
        plain.send_chars("khoa")
        plain.send_key("tab")
        plain.send_chars("test\n")
        plain.wait_until_succeeds("loginctl list-sessions --no-legend | grep -w khoa", timeout=60)
        plain.wait_until_succeeds("ps -u khoa -o args= | grep -Eq '(^|/)Hyprland( |$)'", timeout=60)
        plain.sleep(5)
        # A hint a dead shell lock left at yes: hyprlock taking the screen
        # clears it, since hyprlock is seen by its process, never the hint.
        as_khoa(plain, "lyra lock hint yes")
        wait_hint(plain, "yes")
        o = lyra_lock(plain)
        assert o["data"]["locker"] == "hyprlock", f"lyra lock used {o}"
        assert o["data"]["hintCleared"] is True, f"the stale hint stood: {o}"
        assert locked_hint(plain) == "no"
        # hyprlock leaves at once when the compositor refuses its lock, so
        # one still running seconds later holds the session.
        plain.wait_until_succeeds("ps -u khoa -o args= | grep -Eq '(^|/)hyprlock( |$)'", timeout=10)
        # hyprlock waits out its 2 s resource gathering here (no screencopy
        # buffer on this GPU) and then fades its widgets in: the field is fully
        # drawn some 8 s after the lock.
        plain.sleep(10)
        plain.succeed("ps -u khoa -o args= | grep -Eq '(^|/)hyprlock( |$)'")
        plain.screenshot("11-hyprlock")
        before = pam_failures(plain, "hyprlock")
        plain.send_chars("wrong-password\n")
        plain.wait_until_succeeds(f"test $(journalctl -b --no-pager | grep -c 'pam_unix(hyprlock:auth): authentication failure') -gt {before}", timeout=30)
        plain.succeed("ps -u khoa -o args= | grep -Eq '(^|/)hyprlock( |$)'")
        # hyprlock takes no input while it shows the failure.
        plain.sleep(4)
        plain.send_chars("test\n")
        plain.wait_until_fails("ps -u khoa -o args= | grep -Eq '(^|/)hyprlock( |$)'", timeout=30)
        plain.sleep(2)
        plain.screenshot("12-hyprlock-unlocked")
  '';
}
