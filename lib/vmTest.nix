# lib/vmTest.nix — NixOS integration test: headless boot of the Aoide desktop
# stack.
#
# Exercises the walked module tree (same assembly as mkHost), the aoide +
# drachma packages, greetd wiring, the aoided + shellbridge user services,
# and the graph commands — without real hardware or external network access.
#
# Wired in flake.nix as:
#   checks.<system>.vm-boot = import ./lib/vmTest.nix { inherit pkgs inputs lib; };
#
# ── Trims applied (headless VM) ───────────────────────────────────────────────
#   aoide.facets.stylix.enable = false
#     Reason: Stylix builds a wallpaper with ImageMagick and imports a large
#     theme-target set; the closure is expensive and adds no value for a boot
#     test.
#
#   aoide.facets.quickshell.enable = false
#     Reason: Quickshell sources an upstream flake input with a significant
#     NixOS Wayland closure; its autostart (`quickshell -c shell.qml`) cannot
#     render on the virtual GPU.  The compositor facet is KEPT because it wires
#     greetd + programs.hyprland.
#
#   greetd: will attempt to spawn Hyprland on the virtual GPU and loop.
#     Mitigation: the test asserts the unit exists and is enabled rather than
#     asserting active state, so a respawn loop does not fail the test.
#
#   User services (aoided, shellbridge) are wantedBy graphical-session.target,
#     which never fires headless.  The test enables-linger and starts them
#     explicitly via `systemctl --user`.
#
# ── Stage-dir note ────────────────────────────────────────────────────────────
#   shellbridge.nix sets AOIDE_STAGE_DIR=%h/Aoide/song/stage.  The systemd
#   template expands %h to the service user's HOME at runtime:
#     /home/khoa/Aoide/song/stage
#   The shellbridge binary also honours AOIDE_STAGE_DIR and falls back to the
#   same path from aoide_home(), so both agree on the location.
#   Graph commands are invoked via `su -c` (no login shell), so the user-service
#   environment is not inherited.  The test therefore passes AOIDE_STAGE_DIR
#   explicitly on each graph command invocation.
{
  pkgs,
  inputs,
  lib,
}:
let
  walk = import ./walk.nix { inherit lib; };

  # Walked module tree — identical to mkHost's discovery pass.
  discovered = walk ../modules;

  # Committed songs (same as mkHost).
  songbook = walk ../song/songbook;

  # Optional modules — same tolerance guard as mkHost.
  optionalModule = attr: path: lib.optional (inputs ? ${attr}) path;
  hmModule = optionalModule "home-manager" (inputs.home-manager.nixosModules.home-manager or { });
  stylixModule = optionalModule "stylix" (inputs.stylix.nixosModules.stylix or { });

  # The pkgs overlay injecting the discovered packages — now literally the SAME
  # source as mkHost: both import lib/pkgs.nix's overlay, which auto-discovers
  # pkgs/<name> and guards each name against shadowing a nixpkgs attribute.
  overlayModule =
    { ... }:
    {
      nixpkgs.overlays = [
        (import ./pkgs.nix { inherit lib; }).overlay
      ];
    };

  # home-manager defaults — same as mkHost.
  hmDefaultsModule =
    { ... }:
    lib.optionalAttrs (inputs ? home-manager) {
      home-manager.useGlobalPkgs = lib.mkDefault true;
      home-manager.useUserPackages = lib.mkDefault true;
    };

  # specialArgs mirror what mkHost passes (host/inputs/username/system).
  # Modules that reference these args (e.g. quickshell facet uses `inputs`)
  # receive the real values; the VM node is named "vm-test".
  testSpecialArgs = {
    host = "vm-test";
    username = "khoa";
    system = "x86_64-linux";
    inherit inputs;
  };
in
pkgs.testers.runNixOSTest {
  name = "aoide-vm-boot";

  # node.specialArgs is the per-node specialArgs seam exposed by the NixOS
  # test framework (nixos/lib/testing/nodes.nix).  Modules receiving `host`,
  # `inputs`, `username`, `system` from specialArgs get the values above.
  node.specialArgs = testSpecialArgs;

  # Allow setting nixpkgs.overlays inside the test node — required so
  # overlayModule (and any other walked module) can inject packages.
  node.pkgsReadOnly = false;

  nodes.machine =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      imports =
        discovered
        ++ songbook
        ++ hmModule
        ++ stylixModule
        ++ [
          overlayModule
          hmDefaultsModule
          # ── Aoide VM configuration ──────────────────────────────────────
          # Mirrors yomi-strix but without real hardware + trimmed facet set.
          (
            { lib, ... }:
            {
              # ── Aoide flags ─────────────────────────────────────────────
              aoide.enable = true;
              aoide.user = "khoa";
              aoide.song = "default";

              # Compositor kept: wires greetd so the unit exists + is enabled.
              aoide.facets.compositor.enable = true;
              # Quickshell omitted: heavy closure + cannot render headless.
              aoide.facets.quickshell.enable = false;
              # Stylix omitted: ImageMagick wallpaper + large target set.
              aoide.facets.stylix.enable = false;
              # Obsidian off (same as yomi-strix).
              aoide.obsidian.enable = false;

              # ── Baseline ────────────────────────────────────────────────
              networking.hostName = "vm-test";
              system.stateVersion = lib.mkDefault "25.11";
              nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";

              # ── User account ─────────────────────────────────────────────
              users.users.khoa = {
                isNormalUser = lib.mkDefault true;
                extraGroups = lib.mkDefault [
                  "wheel"
                  "video"
                  "audio"
                  "networkmanager"
                ];
                # Passwordless-friendly for test invocations.
                initialPassword = "test";
              };

              home-manager.users.khoa.home.stateVersion = lib.mkDefault "25.11";

              # ── System packages on PATH ───────────────────────────────────
              # jq only (JSON validation). aoide + drachma come from the
              # nucleus (modules/nucleus/packages.nix) — the test must exercise
              # the REAL install path, not mask its absence (which it did until
              # the first live switch surfaced the gap).
              environment.systemPackages = [
                pkgs.jq
              ];

              # ── VM resources ─────────────────────────────────────────────
              virtualisation.memorySize = 4096;
              virtualisation.cores = 4;
            }
          )
        ];
    };

  testScript = ''
    import json
    import time

    # ── 1. multi-user.target reached ────────────────────────────────────────
    machine.wait_for_unit("multi-user.target")

    # ── 2. aoide + drachma on PATH ──────────────────────────────────────────
    machine.succeed("which aoide")
    machine.succeed("which drachma")

    # `aoide schema --json` must parse and report exactly 45 commands.
    # This is a deliberate drift tripwire: adding or removing a command must
    # consciously update this count (it caught 8 commands that had landed
    # unrecorded — graph wrap/reap/send, conduct, conductor, and the graph session
    # write-verbs; bumped by 4 for the a2a serve / agent add|list|remove stubs
    # (CONTRACTS.md §6); bumped by 1 for `usage` (CONTRACTS.md §4); bumped by 1
    # for `a2a agent send` — the Phase D client-side drive verb; most recently
    # bumped by 1 for `rice design status` — design-mode Phase A (read-only;
    # CONTRACTS.md §4's `stage/design.json` entry).
    schema_raw = machine.succeed("aoide schema --json")
    schema_doc = json.loads(schema_raw)
    # schema --json emits a JSON Outcome envelope:
    #   { "ok": true, "command": "schema", "message": "...", "data": { "commands": [...] } }
    # Fall back to treating the top level as the schema doc for robustness.
    if "commands" in schema_doc:
        cmd_count = len(schema_doc["commands"])
    elif "data" in schema_doc and "commands" in schema_doc.get("data", {}):
        cmd_count = len(schema_doc["data"]["commands"])
    else:
        raise Exception(f"unexpected schema --json shape: {list(schema_doc.keys())}")
    assert cmd_count == 45, (
        f"expected 45 commands, got {cmd_count}.  "
        f"schema output (first 500 chars): {schema_raw[:500]}"
    )

    # `aoide guide` exits 0.
    machine.succeed("aoide guide")

    # ── 3. greetd ────────────────────────────────────────────────────────────
    # greetd will attempt to spawn Hyprland on the virtual GPU and loop.
    # We assert the unit is *enabled* (exists in the service graph) rather than
    # asserting active/activating, so a respawn loop does not fail the test.
    machine.succeed("systemctl is-enabled greetd.service")

    # ── 4. User services: aoided + shellbridge ───────────────────────────────
    # These are wantedBy graphical-session.target which never fires headless.
    # Enable linger so the user slice persists, then start them explicitly.
    machine.succeed("loginctl enable-linger khoa")

    # Wait for the khoa user systemd manager to be up.
    # The user manager unit is user@<uid>.service at the system level.
    khoa_uid = machine.succeed("id -u khoa").strip()
    machine.wait_for_unit(f"user@{khoa_uid}.service")

    runtime_dir = f"/run/user/{khoa_uid}"
    bus_addr = f"unix:path={runtime_dir}/bus"

    def user_cmd(cmd):
        """Run cmd as khoa with the user systemd environment set."""
        return (
            f"su -s /bin/sh khoa -c "
            f"'XDG_RUNTIME_DIR={runtime_dir} "
            f"DBUS_SESSION_BUS_ADDRESS={bus_addr} "
            f"{cmd}'"
        )

    # Start aoided and shellbridge.
    machine.succeed(user_cmd("systemctl --user start aoided.service"))
    machine.succeed(user_cmd("systemctl --user start shellbridge.service"))

    # Assert both services ran successfully.
    # The current skeleton binaries exit cleanly with code 0 after seeding their
    # state (daemon prints status JSON; shellbridge writes stage files).  Because
    # Restart=on-failure only triggers on non-zero exits, the service lands in
    # inactive(dead) with Result=success rather than remaining active.
    # We therefore assert Result=success (clean run) rather than is-active.
    #
    # If a future Agent B revision makes the daemons long-running, this test
    # will naturally accept them as active (active implies Result=success or "").
    def assert_service_ran_ok(svc):
        """Assert the service is active OR completed with Result=success."""
        # is-active exits 0 if active, 3 if inactive but Result=success is fine.
        # We check the Result property directly to cover both.
        result = machine.succeed(
            user_cmd(f"systemctl --user show --value -p Result {svc}")
        ).strip()
        assert result in ("success", ""), (
            f"{svc} Result={result!r}; expected 'success' or empty (active)"
        )

    assert_service_ran_ok("aoided.service")
    assert_service_ran_ok("shellbridge.service")

    # Assert shellbridge seeded its stage files.
    # AOIDE_STAGE_DIR=%h/Aoide/song/stage in the unit expands to
    # /home/khoa/Aoide/song/stage (systemd %h = service user HOME).
    # The binary's fallback from aoide_home() produces the same path.
    # Give shellbridge up to 15 seconds to write the files.
    stage_dir = "/home/khoa/Aoide/song/stage"
    for attempt in range(15):
        try:
            machine.succeed(f"test -f {stage_dir}/sessions.json")
            machine.succeed(f"test -f {stage_dir}/hooks.json")
            break
        except Exception:
            time.sleep(1)
    else:
        # Last try — will raise with a useful message if files are absent.
        listing = machine.succeed(f"ls {stage_dir}/ 2>&1 || true")
        machine.succeed(
            f"test -f {stage_dir}/sessions.json || "
            f"{{ echo 'MISSING sessions.json; stage contents: {listing}'; exit 1; }}"
        )

    # Validate sessions.json: valid JSON + schemaVersion == "0".
    sess_raw = machine.succeed(f"cat {stage_dir}/sessions.json")
    sess = json.loads(sess_raw)
    assert sess.get("schemaVersion") == "0", (
        f"sessions.json schemaVersion expected '0', got: {sess_raw[:200]}"
    )

    # Validate hooks.json: valid JSON + schemaVersion == "0".
    hooks_raw = machine.succeed(f"cat {stage_dir}/hooks.json")
    hooks = json.loads(hooks_raw)
    assert hooks.get("schemaVersion") == "0", (
        f"hooks.json schemaVersion expected '0', got: {hooks_raw[:200]}"
    )

    # ── 5. Graph commands (run as khoa) ──────────────────────────────────────
    # AOIDE_STAGE_DIR is set explicitly because graph commands are invoked via
    # `su -c` (no login shell), so the user-service environment is not present.
    graph_env = (
        f"AOIDE_STAGE_DIR={stage_dir} "
        f"AOIDE_USER=khoa "
        f"HOME=/home/khoa"
    )

    def khoa_graph(cmd):
        return f"su -s /bin/sh khoa -c '{graph_env} {cmd}'"

    # `aoide graph project add demo /tmp --json` exits 0.
    add_raw = machine.succeed(khoa_graph("aoide graph project add demo /tmp --json"))
    add_doc = json.loads(add_raw)
    # Outcome envelope: ok == true or status == "ok".
    ok = add_doc.get("ok") is True or add_doc.get("status") == "ok"
    assert ok, f"graph project add failed: {add_raw[:300]}"

    # `aoide graph view --json` parses with the demo project node present.
    view_raw = machine.succeed(khoa_graph("aoide graph view --json"))
    view_doc = json.loads(view_raw)
    nodes = None
    if "nodes" in view_doc:
        nodes = view_doc["nodes"]
    elif "data" in view_doc and "nodes" in view_doc.get("data", {}):
        nodes = view_doc["data"]["nodes"]
    assert nodes is not None, f"graph view --json missing nodes key: {view_raw[:300]}"
    demo_nodes = [n for n in nodes if n.get("kind") == "project" and n.get("name") == "demo"]
    assert demo_nodes, f"demo project node not found in graph view: {view_raw[:300]}"

    # `aoide graph emit --json` exits 0.
    emit_raw = machine.succeed(khoa_graph("aoide graph emit --json"))
    emit_doc = json.loads(emit_raw)
    ok = emit_doc.get("ok") is True or emit_doc.get("status") == "ok"
    assert ok, f"graph emit failed: {emit_raw[:300]}"

    # graph.json was written and is valid JSON with a nodes key.
    graph_raw = machine.succeed(f"cat {stage_dir}/graph.json")
    graph_doc = json.loads(graph_raw)
    assert "nodes" in graph_doc, f"graph.json missing 'nodes': {graph_raw[:200]}"
    assert "schemaVersion" in graph_doc, f"graph.json missing 'schemaVersion': {graph_raw[:200]}"
  '';
}
