# tests/vm-boot.nix — NixOS integration test: headless boot of the Aoide
# desktop stack.
#
# Lives in tests/, not lib/: lib/ holds build/eval machinery (checks.nix,
# aoideos.nix, pkgs.nix, walk.nix); this is test content. See tests/README.md.
#
# Exercises the module tree through the SAME assembly a real host gets — the
# constructor (`lib/composition.nix` driving `composition.mkNixosModules`, as
# lib/aoideos.nix does) over the record below — plus the aoide package, greeter
# wiring, the aoided user service, and the graph commands, without real hardware
# or external network access. shellbridge is NOT exercised: its module gates on
# the lyra enable fact (it exists to feed the painted shell), and this VM does
# not select the lyra lane at all — see the trims below.
#
# Wired in flake.nix as:
#   checks.<system>.vm-boot = import ./tests/vm-boot.nix { inherit pkgs inputs lib; };
#
# ── Trims applied (headless VM) ───────────────────────────────────────────────
#   stylix NOT SELECTED
#     Reason: Stylix builds a wallpaper with ImageMagick and imports a large
#     theme-target set; the closure is expensive and adds no value for a boot
#     test. Its lane is simply not in the record, so the fact stays false.
#
#   quickshell and lyra NOT SELECTED
#     Reason: Quickshell sources an upstream flake input with a significant
#     NixOS Wayland closure; its autostart (`quickshell -c shell.qml`) cannot
#     render on the virtual GPU.  The compositor lane IS selected because it
#     wires programs.hyprland, and the greeter lane because the test asserts the
#     ly unit.  Not selecting lyra also means shellbridge.service does not
#     exist: shellbridge.nix gates on the lyra fact, so the test neither starts
#     nor asserts it. With lyra unselected the VM also names NO song: a song
#     with no lyra lane to paint it is refused by the platform
#     (`modules/nucleus/assertions.nix`), and the palette the greeter reads is
#     then nucleus's own default — still palette-shaped and non-stock, which is
#     all the ly assertion below claims.
#
#   ly: will attempt to spawn Hyprland on the virtual GPU and loop.
#     Mitigation: the test asserts the unit exists and is enabled rather than
#     asserting active state, so a respawn loop does not fail the test.
#
#   The aoided user service anchors on a target that never fires in this VM
#     (graphical-session or default, lane-dependent).  The test enables-linger
#     and starts it explicitly via `systemctl --user`.
#
# ── Stage-dir note ────────────────────────────────────────────────────────────
#   Graph commands are invoked via `su -c` (no login shell), so no user-service
#   environment is inherited.  The test therefore passes AOIDE_STAGE_DIR
#   explicitly on each graph command invocation, and the commands create what
#   they need under that path themselves. The graph commands exercised here
#   are all CONDUCTING operations, so on the real (unoverridden) layout they'd
#   resolve under state/stage/ (CONTRACTS.md §4) — the deployed shellbridge
#   unit no longer overrides AOIDE_STAGE_DIR at all. This test still sets it
#   (both trees name one directory when the override is present) purely for
#   isolation from the real ~/Aoide tree; it is not a claim about where
#   conducting files belong in production.
{
  pkgs,
  inputs,
  lib,
  system ? "x86_64-linux",
}:
let
  # The constructor, exactly as a real host reaches it: the same module-list
  # assembly `lib/aoideos.nix` drives, with the VM's own record in place of
  # `hosts/<name>/`. `lib/mkHost.nix` used to hand-assemble this list; that file
  # is gone, so the test that exists to exercise the real assembly goes through
  # the real assembly.
  composition = import ../lib/composition.nix { inherit lib; };

  # The pkgs overlay injecting the discovered packages — literally the same
  # source `lib/aoideos.nix` hands a real host: both import lib/pkgs.nix's
  # overlay, which auto-discovers pkgs/<name> and guards each name against
  # shadowing a nixpkgs attribute. `aoide` itself is self-flaked
  # (pkgs/aoide/flake.nix) and named by neither aggregate — it arrives via
  # `aoideInputs.aoide.nixosModules.default`, imported by
  # `modules/nucleus/options.nix` and carrying `overlays.default` with it, the
  # same way a real host picks it up.
  overlay = (import ../lib/pkgs.nix { inherit lib; }).overlay {
    stock = inputs.nixpkgs.legacyPackages.${system};
  };

  # The nucleus lane as `lib/aoideos.nix` builds it for a real host — the same
  # value the flake exports as `nixosModules.nucleus`, and the ONE module that
  # sets `_module.args.aoideInputs` (which is how every lane reaches Aoide's
  # own inputs now; nothing threads them through specialArgs).
  aoideos = import ../lib/aoideos.nix {
    inherit inputs lib system;
    username = "khoa";
  };

  # The VM's host record — the same interface `hosts/<name>/default.nix`
  # answers, inline because this machine exists only for the test.
  #
  # `compositor` is selected with its provider named (hyprland, the only one),
  # and `greeter` with it: the test asserts the ly unit and wants
  # `programs.hyprland` wired. Everything else the desktop carries is trimmed —
  # see the trim notes above. The person is a real user DEFINITION rather than a
  # hand-written account, because the constructor wires Home Manager only where
  # a user asks for it and the tree's lanes write into that user's home.
  vmHost = {
    dendrites.compositor = {
      enable = true;
      provider = "hyprland";
    };
    dendrites.greeter.enable = true;

    users.khoa = {
      definition = ./vm-boot-user.nix;
      homeManager.enable = true;
    };
  };

  # The platform pass, resolved once: `modules` is the module list a
  # `nixosSystem` would get and `specialArgs` the args it would get them with.
  resolved = composition.mkNixosModules {
    hostName = "vm-test";
    registry = import ../modules;
    nucleus = aoideos.nucleusModule;
    hostModules = [ vmHost ];
    homeManagerModule = inputs.home-manager.nixosModules.home-manager;
    overlays = [ overlay ];
    # specialArgs mirror what lib/aoideos.nix passes: `username`, and NOT
    # `inputs` — the lanes reach Aoide's own inputs as `aoideInputs`, which the
    # nucleus lane above defines once. The node is named "vm-test".
    specialArgs = {
      username = "khoa";
    };
    inherit system;
  };
in
pkgs.testers.runNixOSTest {
  name = "aoide-vm-boot";

  # node.specialArgs is the per-node specialArgs seam exposed by the NixOS
  # test framework (nixos/lib/testing/nodes.nix). Modules receiving `host`,
  # `inputs`, `username`, `system` from specialArgs get the constructor's own
  # values, so what the node receives is what a real host receives.
  node.specialArgs = resolved.specialArgs;

  # Allow setting nixpkgs.overlays inside the test node — required so
  # overlay (and any other module) can inject packages.
  node.pkgsReadOnly = false;

  nodes.machine =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      imports = resolved.modules ++ [
        # ── This machine ────────────────────────────────────────────────
        # Inline rather than in `vmHost.nixos`, so the test's own settings sit
        # beside the test's own assertions. Mirrors yomi-strix with the lane set
        # trimmed.
        (
          { lib, ... }:
          {
            # ── Aoide ────────────────────────────────────────────────────
            # `aoide.compositor.enable` and `aoide.greeter.enable` are the
            # selected lanes' own mkDefault — selection is what turns them on,
            # and a line here would be a second answer to the same question.
            #
            # Quickshell/lyra/stylix are not SELECTED (heavy closure, and
            # quickshell cannot render on the virtual GPU), so nothing has to
            # switch them off: their facts default false in nucleus and their
            # lanes are never imported. With lyra off, shellbridge.service does
            # not exist at all, so the test neither starts nor asserts it.
            #
            # No song: the song half of the stack is lyra's (deploy, stage seed,
            # restart), and a song named with no lyra lane is refused by the
            # platform (`modules/nucleus/assertions.nix`). The livery the greeter
            # reads is then nucleus's own default palette — still palette-shaped
            # and non-stock, which is all the ly assertion below claims.
            aoide.enable = true;
            aoide.user = "khoa";

            # ── Baseline ────────────────────────────────────────────────
            networking.hostName = "vm-test";
            nixpkgs.hostPlatform = "x86_64-linux";

            # ── System packages on PATH ───────────────────────────────────
            # jq only (JSON validation). aoide comes from the
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
    import re
    import shlex

    # ── 1. multi-user.target reached ────────────────────────────────────────
    machine.wait_for_unit("multi-user.target")

    # ── 2. aoide on PATH ────────────────────────────────────────────────────
    machine.succeed("which aoide")

    # `aoide schema --json` must parse and report a non-empty command list —
    # proof the built binary runs in a booted system and emits parseable
    # schema JSON. The exact command-path set is pinned by
    # `crates/cli/src/registry.rs`'s golden snapshot test, not here.
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
    assert cmd_count > 0, (
        f"schema --json reported zero commands.  "
        f"schema output (first 500 chars): {schema_raw[:500]}"
    )

    # `aoide guide` exits 0.
    machine.succeed("aoide guide")

    # ── 3. ly (the greeter) ──────────────────────────────────────────────────
    # ly will attempt to spawn Hyprland on the virtual GPU and loop. We assert
    # the unit is *enabled* (exists in the service graph) rather than
    # asserting active/activating, so a respawn loop does not fail the test.
    machine.succeed("systemctl is-enabled display-manager.service")

    # The rice half. The greeter's colours are a livery read (the greeter lane,
    # `lyPlain`/`lyBold`), so assert the generated config carries palette-shaped
    # values AND that they are not ly's stock ones. Shape plus "not stock"
    # rather than a specific hex: pinning the song's palette here would turn
    # every re-rice into a test edit.
    ly_cfg = machine.succeed("cat /etc/ly/config.ini")
    livery_keys = (
        ("bg", "00"),
        ("fg", "00"),
        ("border_fg", "00"),
        ("error_fg", "01"),
        ("colormix_col1", "00"),
        ("colormix_col2", "00"),
    )
    for key, style in livery_keys:
        assert re.search(
            rf"^{key}=0x{style}[0-9a-fA-F]{{6}}$", ly_cfg, re.M
        ), f"ly config missing a livery-shaped {key}:\n{ly_cfg}"
    for stock in (
        "bg=0x00000000",
        "fg=0x00FFFFFF",
        "error_fg=0x01FF0000",
        "colormix_col1=0x00FF0000",
        "colormix_col2=0x000000FF",
    ):
        assert stock not in ly_cfg, (
            f"ly kept its stock {stock} — the palette read did not reach the greeter"
        )

    # The behaviour half, carried from dxflake's ly dendrite. Asserted as
    # literals because these ARE the decision, unlike the colours.
    for line in ("animation=colormix", "animation_timeout_sec=300", "clear_password=true"):
        assert line in ly_cfg, f"ly config missing {line}:\n{ly_cfg}"

    # ── 4. User service: aoided ──────────────────────────────────────────────
    # shellbridge.service does not exist in this VM at all: shellbridge.nix
    # gates the whole module on the lyra enable fact (it exists to feed the
    # painted shell), and this VM leaves that fact off — so only aoided is
    # started and asserted here, and the stage files shellbridge would seed are
    # not expected either.
    # Enable linger so the user slice persists, then start it explicitly.
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

    # Start aoided.
    machine.succeed(user_cmd("systemctl --user start aoided.service"))

    # Assert the service ran successfully.
    # The current skeleton binary exits cleanly with code 0 after its
    # self-check (daemon prints status JSON).  Because Restart=on-failure only
    # triggers on non-zero exits, the service lands in inactive(dead) with
    # Result=success rather than remaining active.
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

    # Stage dir: without shellbridge nothing pre-seeds sessions.json/hooks.json
    # here; the graph commands below create what they need under this path via
    # AOIDE_STAGE_DIR. Only conducting files (sessions.json, graph.json) are
    # ever touched under it — quickshell is disabled in this VM (see the
    # trims note at the top of this file), so nothing rice-shaped is
    # exercised — hence state/stage/, the tree those files resolve to on the
    # real (unoverridden) layout.
    stage_dir = "/home/khoa/Aoide/state/stage"

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

    # `aoide project add demo /tmp --json` exits 0 (ex-`graph project add`,
    # task #101 R1 — `project *` promoted to its own top-level group).
    add_raw = machine.succeed(khoa_graph("aoide project add demo /tmp --json"))
    add_doc = json.loads(add_raw)
    # Outcome envelope: ok == true or status == "ok".
    ok = add_doc.get("ok") is True or add_doc.get("status") == "ok"
    assert ok, f"project add failed: {add_raw[:300]}"

    # `aoide graph --json` parses with the demo project node present
    # (ex-`graph view`, R1 — the render IS the bare command now).
    view_raw = machine.succeed(khoa_graph("aoide graph --json"))
    view_doc = json.loads(view_raw)
    nodes = None
    if "nodes" in view_doc:
        nodes = view_doc["nodes"]
    elif "data" in view_doc and "nodes" in view_doc.get("data", {}):
        nodes = view_doc["data"]["nodes"]
    assert nodes is not None, f"graph --json missing nodes key: {view_raw[:300]}"
    demo_nodes = [n for n in nodes if n.get("kind") == "project" and n.get("name") == "demo"]
    assert demo_nodes, f"demo project node not found in the graph render: {view_raw[:300]}"

    # `aoide session prune --json` exits 0 (ex-`graph prune`, R1 — session
    # lifecycle promoted to `session *`) — the blessed manual resync now that
    # `graph emit` is gone (restage_graph already runs at every mutation
    # site; prune's own restage is the one this test exercises). Seed a
    # `done` session directly (no conduct session exists in this headless
    # VM) so prune has something to actually drop rather than taking its
    # no-op early return, then delete graph.json so its reappearance can
    # only be explained by THIS invocation (re)staging it.
    sessions_seed = json.dumps({
        "schemaVersion": "0",
        "sessions": [{"sessionId": "vmtest-prune-probe", "state": "done"}],
    })
    machine.succeed(f"printf '%s' {shlex.quote(sessions_seed)} > {stage_dir}/sessions.json")
    machine.succeed(f"rm -f {stage_dir}/graph.json")

    prune_raw = machine.succeed(khoa_graph("aoide session prune --json"))
    prune_doc = json.loads(prune_raw)
    ok = prune_doc.get("ok") is True or prune_doc.get("status") == "ok"
    assert ok, f"session prune failed: {prune_raw[:300]}"
    prune_data = prune_doc.get("data", prune_doc)
    assert "vmtest-prune-probe" in prune_data.get("removed", []), (
        f"session prune did not remove the seeded session: {prune_raw[:300]}"
    )

    # graph.json reappeared — (re)staged by that same `session prune`
    # invocation, since nothing else could have recreated it after the rm
    # above — and is valid JSON with a nodes key.
    graph_raw = machine.succeed(f"cat {stage_dir}/graph.json")
    graph_doc = json.loads(graph_raw)
    assert "nodes" in graph_doc, f"graph.json missing 'nodes': {graph_raw[:200]}"
    assert "schemaVersion" in graph_doc, f"graph.json missing 'schemaVersion': {graph_raw[:200]}"
  '';
}
