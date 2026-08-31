# tests/vm-boot.nix — NixOS integration test: headless boot of the Aoide
# desktop stack.
#
# Lives in tests/, not lib/: lib/ holds build/eval machinery (checks.nix,
# mkHost.nix, pkgs.nix, walk.nix); this is test content. See tests/README.md.
#
# Exercises the walked module tree (same assembly as mkHost), the aoide
# package, greeter wiring, the aoided user service, and the graph commands —
# without real hardware or external network access.  shellbridge is NOT
# exercised: its module gates on the quickshell facet (it exists to feed the
# quickshell UI), and this VM disables that facet — see the trims below.
#
# Wired in flake.nix as:
#   checks.<system>.vm-boot = import ./tests/vm-boot.nix { inherit pkgs inputs lib; };
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
#     the ly greeter + programs.hyprland.  This also removes shellbridge.service
#     entirely: shellbridge.nix gates on this facet, so the test neither
#     starts nor asserts it.
#
#   ly: will attempt to spawn Hyprland on the virtual GPU and loop.
#     Mitigation: the test asserts the unit exists and is enabled rather than
#     asserting active state, so a respawn loop does not fail the test.
#
#   The aoided user service anchors on a target that never fires in this VM
#     (graphical-session or default, facet-dependent).  The test enables-linger
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
  walk = import ../lib/walk.nix { inherit lib; };

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
  # `aoide` itself is self-flaked (pkgs/aoide/flake.nix) and skipped by the
  # walker — injected from the `aoide` input, exactly as mkHost does.
  overlayModule = _: {
    nixpkgs.overlays = [
      (import ../lib/pkgs.nix { inherit lib; }).overlay
      (_final: _prev: { aoide = inputs.aoide.packages.${system}.default; })
    ];
  };

  # home-manager defaults — same as mkHost.
  hmDefaultsModule =
    _:
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
              aoide.song = "sonata";

              # Compositor kept: wires the ly greeter so the unit exists + is enabled.
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

    # The rice half. The greeter's colours are a livery read (compositor facet,
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
    # gates the whole module on the quickshell facet (it exists to feed the
    # quickshell UI), and this VM disables that facet — so only aoided is
    # started and asserted here, and the stage files shellbridge would seed
    # are not expected either.
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
