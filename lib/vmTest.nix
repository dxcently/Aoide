# lib/vmTest.nix — NixOS integration test: headless boot of the Aoide desktop
# stack.
#
# Exercises the walked module tree (same assembly as mkHost), the aoide
# package, greetd wiring, the aoided user service, and the graph commands —
# without real hardware or external network access.  shellbridge is NOT
# exercised: its module gates on the quickshell facet (it exists to feed the
# quickshell UI), and this VM disables that facet — see the trims below.
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
#     greetd + programs.hyprland.  This also removes shellbridge.service
#     entirely: shellbridge.nix gates on this facet, so the test neither
#     starts nor asserts it.
#
#   greetd: will attempt to spawn Hyprland on the virtual GPU and loop.
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
#   (%h/Aoide/song/stage's expansion, /home/khoa/Aoide/song/stage) explicitly
#   on each graph command invocation, and the commands create what they need
#   under that path themselves.
{
  pkgs,
  inputs,
  lib,
  system ? "x86_64-linux",
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
  # `aoide` itself is self-flaked (pkgs/aoide/flake.nix) and skipped by the
  # walker — injected from the `aoide` input, exactly as mkHost does.
  overlayModule = _: {
    nixpkgs.overlays = [
      (import ./pkgs.nix { inherit lib; }).overlay
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

    # ── 1. multi-user.target reached ────────────────────────────────────────
    machine.wait_for_unit("multi-user.target")

    # ── 2. aoide on PATH ────────────────────────────────────────────────────
    machine.succeed("which aoide")

    # `aoide schema --json` must parse and report exactly cmd_count commands.
    # This is a deliberate drift tripwire: adding or removing a command must
    # consciously update this count (it caught 8 commands that had landed
    # unrecorded — graph wrap/reap/send, conduct, conductor, and the graph session
    # write-commands; bumped by 4 for the a2a serve / agent add|list|remove stubs
    # (CONTRACTS.md §6); bumped by 1 for `usage` (CONTRACTS.md §4); bumped by 1
    # for `a2a agent send` — the Phase D client-side drive command;
    # bumped by 1 for `hooks install` — the generic hook-installer command;
    # bumped by 3 for `livery emit|resolve|lint` — the native note-engine
    # commands (LIVERY-MERGE Phase 1); bumped by 3 for `rice mode
    # status|stage|declarative` — the staging/declarative mode toggle
    # (reached 54, but this count was never bumped for it until now);
    # bumped by −3 for deleting a since-removed `rice design status/enter/exit`
    # group outright — it added nothing mechanically over `rice mode stage`
    # and was cut clean (landed at 51); bumped by 4 for a since-reworked `rice
    # draft save|list|stage|drop` — the durable-scratch-snapshot group (reached
    # 55); bumped by −1 for cutting `rice gen` outright — a speculative
    # prompt/wallpaper generator that was never built and had no design behind
    # it, not left as a permanent stub (landed at 54); net unchanged (−1, +1)
    # for replacing `rice draft stage` (copy-based) with `rice mode draft` —
    # symlink-routes stage/livery.json into a saved draft instead of
    # snapshotting into/out of it, superseding the copy-based command outright
    # (no-internal-aliases rule) — landed at 54; bumped by 5 for the new
    # `peer add|list|remove|pull|status` group (cross-device peer federation,
    # CONTRACTS.md §7) — reached 59; bumped by 1 for the new `shell reload`
    # command (Quickshell IPC hot-reload trigger) — reached 60; bumped by 2
    # for the new `screen info`/`screen shot` group (Phase 1 of the `screen`
    # command family, docs/architecture/PACKAGE-LAYOUT.md) — reached 62; bumped
    # by 6 for the new `screen point move|click|scroll|idle|save|restore`
    # group (Phase 2 of the `screen` command family — pointer synthesis via
    # wlrctl, ported from tools/pointer.sh) — reached 68; bumped by 1 for the
    # new `screen ocr` command (Phase 3 — tesseract text extraction) — reached
    # 69; bumped by 1 for the new `screen send` command (Phase 5 — hand a
    # capture to a conducted session or a registered A2A agent, routed
    # through the existing `graph send`/`a2a agent send` gates) — reached 70;
    # bumped by 1 for `graph permit` (the herald's approve/deny summons,
    # commit 717708a — landed in this same shared tree while Phase B below
    # was in flight; that commit updated the golden snapshot in
    # crates/cli/src/registry.rs but missed this tripwire, so this count was
    # briefly wrong on disk between the two — caught and fixed here rather
    # than left silently stale) — reached 71; bumped by 2 for the new
    # `screen point drag`/`screen point hover` commands (Phase B of the
    # pointer-emulation workstream, khoa 2026-08-17 — atomic
    # press-move-release drag, and a hover command that reports which layer
    # surfaces/windows appeared/disappeared/retitled while parked) — reached
    # 73; bumped by 1 for the new `screen diff` command (Phase E of the
    # pointer-emulation workstream, same day — mechanical act-verification:
    # re-shoot a prior capture's identical rect, pixel-diff the two images,
    # report a hyprctl inventory delta alongside it) — reached 74; bumped by
    # 1 for the new `screen point text` command (Phase F of the
    # pointer-emulation workstream, khoa 2026-08-17 — click a word/phrase an
    # earlier `screen ocr` pass already located, by name instead of a
    # picked-by-eye pixel) — reached 75; bumped by 1 for `herald push`, the
    # feed command of the herald retcon (khoa 2026-08-17 — dunst stops drawing
    # and becomes the daemon only, handing each notification to this command
    # through its `script` hook; the Quickshell herald draws the card from
    # the resulting stage/herald.json) — reached 76; bumped by 7 for the
    # self-ricing take tree and the flake integrity checker (`rice back`,
    # `rice take` and its `list`/`mark`/`diff`/`prune` leaves, and
    # `soundcheck`) — reached 83, though this tripwire had already drifted
    # to stale-83-vs-actual-87 by the time P-A5 (binary-split workstream)
    # landed — never bumped for whatever pushed the true count to 87
    # (task #71 territory). P-A5 removed the 39-path graphical bundle
    # (rice/draft/mode/cover/livery/shellbridge/quickshell/screen/herald/
    # take) from `aoide` outright — it now lives ONLY in the separate
    # `lyra` binary (docs/architecture/PACKAGE-LAYOUT.md, CONTRACTS.md §3)
    # — landing core at 48. The messaging workstream then added `who` (49)
    # and `inbox list|read|clear` (52), and the secrets workstream added
    # `secrets serve|exec|add|rm|grant|revoke` (58) and `secrets enroll` (59)
    # (spelled `vault ...` until the P-V4b rename — paths rename in place,
    # count holds); bumped by 1 for `secrets put` — the write half (backend
    # `set` templates + the built-in `file` backend, Workstream SECRETS
    # P-V4c) — reached 60; bumped by 1 for `secrets set-totp` — flips an
    # existing policy's requireTotp bit without hand-editing policy.json
    # (Workstream SECRETS P-V4e; the same phase also added `secrets enroll
    # --show` and a tty-hidden-input prompt for `secrets put`, neither of
    # which registers a new path) — reached 61; bumped by 2 for
    # `secrets automate` + `secrets expose` — the per-secret automation
    # and remote-reachability gates (Workstream SECRETS P-N1) — reached 63;
    # bumped by 3 for `secrets pending`/`approve`/`dismiss` — the parked-ask
    # lifecycle (P-N2) — reached 66; bumped by 1 for `secrets watch` — the
    # terminal surface over parked asks and broker events — reached 67;
    # bumped by 1 for `secrets migrate` — moves a secret's stored value
    # between backends (P-G2, task #72) — reached 68 (this assert was
    # updated to 68 with that landing, though this historical comment
    # wasn't extended to say so until now); bumped by 1 for `events tail`
    # — the aoided event bus's own terminal-reachable follow command (P-D3,
    # docs/architecture/AOIDED.md) — reached 69; bumped by 1 for `peer hub`
    # — designates at most one registered peer as the hub address
    # resolution prefers as a last-resort remote target (P-D5,
    # docs/architecture/AOIDED.md) — reached 70; bumped by 1 for `graph
    # resurrect` — revives a project's most recently-ended resumable
    # session off the durable session ledger, the ledger/resume phase of
    # harness summoning (P-D8, docs/architecture/AOIDED.md's "L5") —
    # reached 71; bumped by 1 for `identity` — this instance's lazily-minted
    # ed25519 identity show command (P-P1, docs/architecture/PAIRING.md) —
    # reached 72; bumped by 4 for `peer pair request|pending|approve|
    # reject` — the pairing ceremony's CLI half (P-P2,
    # docs/architecture/PAIRING.md) — reached 76; bumped by 1 for `peer
    # allow` — the closed-capability-set grant/revoke command backing the A2A
    # spawn arm's hard gate (P-P3, docs/architecture/PAIRING.md decisions
    # 5/6) — reached 77; bumped by 1 for `peer spawn` — the signed,
    # spawn-shaped message/send that actually reaches that gate from the
    # CLI (P-P5b, docs/architecture/PAIRING.md) — reached 78; bumped by 2
    # for `peer discover`/`peer invite` — the LAN discovery beacon's CLI
    # half, a read-only multicast sweep plus a sugar-over-the-ceremony
    # invite (P-P6, docs/architecture/PAIRING.md's "Discovery
    # (advertise-but-locked)" section) — reached 80; bumped by −4 for
    # deleting the legacy `a2a agent add|list|remove|send` family outright
    # (pre-pairing legacy — unsigned, ungated, structurally superseded by
    # the `peer` family) — reached 76.
    # This tripwire tracks `crates/cli/src/registry.rs`'s golden count —
    # bump BOTH in the same commit that registers a command.
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
    assert cmd_count == 76, (
        f"expected 76 commands, got {cmd_count}.  "
        f"schema output (first 500 chars): {schema_raw[:500]}"
    )

    # `aoide guide` exits 0.
    machine.succeed("aoide guide")

    # ── 3. greetd ────────────────────────────────────────────────────────────
    # greetd will attempt to spawn Hyprland on the virtual GPU and loop.
    # We assert the unit is *enabled* (exists in the service graph) rather than
    # asserting active/activating, so a respawn loop does not fail the test.
    machine.succeed("systemctl is-enabled greetd.service")

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
    # AOIDE_STAGE_DIR.
    stage_dir = "/home/khoa/Aoide/song/stage"

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
