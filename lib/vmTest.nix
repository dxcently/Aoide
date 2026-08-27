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
    import shlex

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
    # (advertise-but-locked)" section) — reached 80; bumped by 1 for
    # `graph session carry` — the durable-session mark (task #96, 562c8f6;
    # this tripwire was never bumped with that landing and sat at
    # stale-80-vs-actual-81 until the next entry's review caught it) —
    # reached 81; bumped by −4 for deleting the legacy `a2a agent
    # add|list|remove|send` family outright (pre-pairing legacy — unsigned,
    # ungated, structurally superseded by the `peer` family) — reached 77;
    # bumped by −1 for deleting `graph wrap` outright — zero production
    # callers (superseded by `conduct`/`graph spawn`), command-defrag lane
    # (task #101) — reached 76; bumped by −1 for deleting `graph emit` —
    # restage_graph fires at every mutation site and `graph prune` is the
    # blessed manual resync, command-defrag lane (task #101) — reached 75;
    # bumped by −1 for deleting `graph focus` — conductor and shellbridge
    # call focus_session directly — reached 74; bumped by −1 for folding
    # `peer list` into `peer status --json` (full Peer row) — reached 73;
    # the graph-prefix cutover (task #101, Lane R, Phase R1) then renamed 18
    # of the 19 `graph.*` spellings IN PLACE — `graph view` -> bare `graph`
    # (the render; `graph link` alone survives the family), `graph
    # send`/`graph spawn`/`graph resurrect` -> bare `send`/`spawn`/
    # `resurrect`, `graph session *`/`graph permit`/`graph pending *`/`graph
    # reap`/`graph prune` -> `session *`, `graph project *` -> `project *` —
    # a hard cutover, no aliases; the PATH SET changed, the COUNT did not:
    # still 73.
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
    assert cmd_count == 73, (
        f"expected 73 commands, got {cmd_count}.  "
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
