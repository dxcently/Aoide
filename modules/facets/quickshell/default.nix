# modules/facets/quickshell/default.nix — Quickshell shell-surface facet.
#
# Renders the complete Aoide shell surface using Quickshell (QML runtime).
# Surfaces owned: bar, notifications, launcher, osd, lockscreen, greeter,
# wallpaper, agentWidgets, sessionGraph.
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.livery (palette + component tiers) and aoide.surfaces.
#   - Component-tier fallback (null → palette) applied locally, never pushed
#     back into the option system.
#   - NEVER reads song/ runtime paths at build time (checks.no-song-read
#     enforces this structurally).
#
# Communication discipline (entities/Quickshell):
#   - QML reads state files from song/stage/ at runtime (hot-reload).
#   - QML issues commands via the shellbridge unix socket.
#   - QML never speaks MCP or any agent protocol.
{
  config,
  lib,
  pkgs,
  inputs,
  ...
}:
let
  cfg = config.aoide.facets.quickshell;

  # Committed songs live here (CONTRACTS.md §5) — versioned score, legitimately
  # walked at build time (checks.no-song-read only bans song/ RUNTIME infixes,
  # never song/songbook/).
  songbook = ../../../song/songbook;

  # ── Active song's committed livery — also a legitimate build-time read ─────
  # `song/songbook/<name>/livery.json` is versioned score (checks.no-song-read
  # only bans song/{stage,auditions,catalog,index}/, never song/songbook/), so
  # naming the ACTIVE song's livery file here is allowed for the same reason
  # `songbook` above is. Path concatenation (not string-interpolating the
  # whole `songbook` dir) so only this one file gets copied into the store.
  activeSongLivery = songbook + "/${config.aoide.song}/livery.json";

  # ── Songbook manifest + registry, typed (W3a/W3b), generated once (C4) ──
  # `manifest.json`/`registry.json` source from eval-time nix: a `_widgets/`
  # shelf when present (sonata today) is authoritative and runs through
  # `composeSong`; a shelf-less song (fugue, etude, nocturne today)
  # synthesizes straight from a `widgets/` directory scan, with the registry
  # falling back to `livery.json`'s `.widgets // {}`. `manifest.json`'s SHAPE
  # is the owner map (`{ "<slot>": { "owner": "...", "file": "..." } }`,
  # W3b) — an OBJECT, so per-song key order is not semantically load-bearing
  # for it; `registry.json` keeps whatever field order its source produced.
  # Full reasoning (the shelf/scan split, the shelf-covers-scan guards, the
  # manifest/registry emptiness asymmetry, why `registry.json`'s type flip
  # landed a commit later than `manifest.json`'s) lives in `git log` on this
  # file up to W3b and in `lib/song.nix`'s header — not restated here.
  #
  # C4/W3: the computation itself now lives in `lib/songbook.nix`, shared
  # verbatim with the `songbookManifest` flake output that
  # `pkgs/aoide/crates/song/src/widgets.rs` shells out to at runtime (`nix
  # eval --json`) to regenerate `manifest.json`/`registry.json` WHOLE — the
  # hot-sync half of `rice stage`. One generator, two callers: this
  # derivation and the runtime shell-out can never drift apart because they
  # run the same function. See that file for the shelf/scan split and the
  # shelf-covers-scan guards; see its own comment for why the manifest/
  # registry emptiness asymmetry (below) is deliberate.
  songbookData = import ../../../lib/songbook.nix { inherit lib songbook; };

  manifestAttrs = songbookData.manifestAttrs;
  registryAttrs = songbookData.registryAttrs;

  manifestJsonFile = pkgs.writeText "aoide-quickshell-manifest.json" (builtins.toJSON manifestAttrs);
  registryJsonFile = pkgs.writeText "aoide-quickshell-registry.json" (builtins.toJSON registryAttrs);

  # ── Seed script for `home.activation.aoideSeedStage` (below) ───────────────
  # Reasserts the ACTIVE song's committed livery into the live stage twin
  # (`song/stage/livery.json`, CONTRACTS.md §4) on every activation, injecting
  # the same `"song"` field `aoide rice preview <name>` would (jq's
  # `. + {song: …}`; `-S` sorts keys to match serde_json::Value's BTreeMap
  # ordering) — byte-identical to what `rice preview ${config.aoide.song}`
  # would stage (verified by hand: `jq -S '. + {song:"sonata"}'` against
  # song/songbook/sonata/livery.json reproduces the current staged file
  # exactly). Write-temp-then-rename in the SAME directory (so the rename is
  # atomic) mirrors `shellbridge::atomic_write` (`aoide_storage::fs::atomic_write`)
  # so a hot-reloading FileView (LiveryState.qml) never reads a torn file.
  # The write goes to livery.json, the canonical stage livery file. The whole
  # thing is one script
  # (not inline `run` commands) so a `--dry-run` activation either runs it in
  # full or not at all — never a half-applied mkdir/mktemp/jq/mv sequence.
  seedStageScript = pkgs.writeShellScript "aoide-seed-stage" ''
    set -euo pipefail
    mkdir -p "$HOME/Aoide/song/stage"
    tmp=$(mktemp "$HOME/Aoide/song/stage/.livery.json.XXXXXX")
    ${pkgs.jq}/bin/jq -S '. + {song: $song}' --arg song "${config.aoide.song}" \
      "${activeSongLivery}" > "$tmp"
    mv -f "$tmp" "$HOME/Aoide/song/stage/livery.json"
  '';

  # ── QML root — the full skeleton config installed into run/qml/ ────────────
  # Each surface widget is a stub that reads its colors from livery. Deployed
  # to a gitignored root-runtime dir (run/, alongside catalog/index/log —
  # never the repo's checked-in tree) so the live copy can never be confused
  # with source (modules/facets/quickshell/qml/) or collide with it — this is
  # what CONTRACTS.md §2 and the deploy-path fix this comment accompanies are
  # about. At runtime Quickshell hot-reloads from song/stage/livery.json via
  # a FileView; the build only installs the structural QML, not the note
  # values themselves.
  #
  # ── Per-song flavor widgets (CONTRACTS.md §5) ───────────────────────────
  # Beyond the shared, song-blind QML tree above, this also carries per-song
  # widget QML from song/songbook/*/widgets/ — versioned score, not runtime —
  # for EVERY committed song at once, so a live `aoide rice preview <name>`
  # can hot-swap a widget's BODY (not just its colours) with no rebuild. The
  # WHOLE widgets/ dir is carried (helper components + asset subdirs travel
  # with the slot file that needs them), but only top-level LOWERCASE-KEBAB
  # `.qml` files become slots, named for their basename; an UPPERCASE-first
  # filename is a helper component (QML's own type-file convention — only an
  # uppercase-first name is a valid QML type), carried but never
  # independently a slot; `.gitkeep` is always skipped. A song's other files
  # (outside widgets/) are ignored. `manifest.json` records which songs
  # authored which slots (computed above, `manifestJsonFile` — typed nix, see
  # that comment for the shelf/scan split), so runtime QML (the staging
  # engine, StagingEngine.qml) can check availability without probing the
  # filesystem per-frame. The wired-slot catalog (which slots an anchor
  # actually resolves at runtime) lives in qml/slots.md, not here.
  #
  # ── Declared widget-type registry (Phase 2) ─────────────────────────────
  # `$out/qml/songs/registry.json`, shaped `{ "<song>": { "<slot>": {
  # …declaration… } } }` — which slots a song declares as widget-TYPE
  # registrations (a rarer, smaller set than manifest.json's "which slot
  # bodies exist"). Computed above (`registryAttrs`) from each committed
  # song's `livery.json` (`.widgets // {}`) for a shelf-less song, or
  # `composeSong`'s own `arrangement.widgets` for a shelf-present one — never
  # from any nix OPTION: `config.aoide.arrangement.widgets` (Phase 1) only
  # exists for the ACTIVE song at eval time (a song's rice.nix self-gates on
  # `config.aoide.song == "<name>"`), so cross-song data has to come from
  # committed FILES. Every committed song gets an entry — `{}` when the song
  # has no `livery.json`/no `.widgets` key, or (shelf-present) no declared
  # widget — never an error, never a skipped song. The shelf-less path stays
  # permissive: whatever's under `.widgets` passes through unjudged; shape
  # validation is Phase 3's job (the Rust-side `rice lint` engine). The
  # shelf-present path is exactly as permissive in effect — `composeSong`
  # types the fields but does not reject an undeclared (`kind = null`)
  # widget, it just excludes it, same as `.widgets` omitting a key.
  quickshellConfig = pkgs.runCommand "aoide-quickshell-config" { nativeBuildInputs = [ pkgs.jq ]; } ''
    mkdir -p "$out/qml"
    cp -r ${./qml}/. "$out/qml/"

    # ── Manifest + registry: typed nix, formatted through jq ───────────────
    # `manifestJsonFile`/`registryJsonFile` (above) are the validated
    # content — `builtins.toJSON` is compact and this pretty-prints it to
    # match the old jq-walk's formatting (2-space indent). `jq .` reorders
    # nothing it is handed; both files' key order is whatever nix's
    # `toJSON` already produced (alphabetic per level — see the manifest
    # comment above for why that's no longer load-bearing for either file's
    # per-song contents, only their per-song top-level keys, which were
    # already alphabetic before this commit too).
    mkdir -p "$out/qml/songs"
    manifest="$out/qml/songs/manifest.json"
    jq . ${manifestJsonFile} > "$manifest"
    registry="$out/qml/songs/registry.json"
    jq . ${registryJsonFile} > "$registry"

    # ── Carry over per-song flavor widgets ──────────────────────────────────
    # Copy the WHOLE widgets/ dir per song (bring any helper .qml/asset
    # subdirs along — a multi-file widget like sonata's bar.qml needs its
    # WorkspaceRow.qml helper sitting right beside it). A file starting
    # uppercase is a helper component (QML's own type-file convention: only
    # an uppercase-first filename is a valid QML type name) — carried to
    # disk so the slot file's relative imports resolve, but never
    # independently resolvable as a slot itself (nix already decided the
    # slot list, above — this loop no longer re-derives it).
    for d in ${songbook}/*/; do
      name=$(basename "$d")
      if [ -d "$d/widgets" ]; then
        mkdir -p "$out/qml/songs/$name"
        cp -r "$d/widgets/." "$out/qml/songs/$name/"

        # ── qmldir: register this song's helper types ────────────────────────
        # A slot body is loaded by WidgetSlot/SurfaceSlot through
        # `Qt.createComponent(Qt.resolvedUrl("songs/<song>/<slot>.qml"))`
        # (StagingEngine.qml's `source`), which yields a `qs:` URL. QML's
        # implicit "types in my own directory" resolution does NOT apply to a
        # component loaded from that scheme, and this subdirectory is not on
        # any import path — so without a qmldir an uppercase sibling is simply
        # not a type, and the body fails to load with `X is not a type`.
        #
        # This is not hypothetical: it is exactly what broke sonata's bar when
        # the helper components moved out of the facet's own qml/ directory
        # (which IS the shell's implicit import scope, and is why nobody had
        # to declare them before) and into the songbook. The failure is loud
        # in the log but silent on screen — the slot renders nothing.
        #
        # Uppercase-first only: lowercase-kebab files are SLOTS, resolved by
        # URL through the manifest and never by type name.
        for h in "$d/widgets/"[A-Z]*.qml; do
          [ -e "$h" ] || continue
          t=$(basename "$h" .qml)
          printf '%s 1.0 %s.qml\n' "$t" "$t" >> "$out/qml/songs/$name/qmldir"
        done
      fi
    done
  '';

  # Path used at runtime: ~/Aoide/run/qml/shell.qml is Quickshell's entry
  # point — the rsync-deployed copy (see the home.activation entry below),
  # not the repo's source tree.
  shellQmlEntry = "/home/${config.aoide.user}/Aoide/run/qml/shell.qml";

  # The Quickshell binary from the pre-declared flake input (flake.nix).
  quickshellPkg = inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;
in
{
  # ── Option: aoide.facets.quickshell.enable ─────────────────────────────────
  options.aoide.facets.quickshell = {
    enable = lib.mkEnableOption "Quickshell shell-surface facet";
  };

  # ── Config: only wired when the facet is enabled ───────────────────────────
  config = lib.mkIf cfg.enable {

    # ── Surface-ownership registry ──────────────────────────────────────────
    # Declare every Quickshell-owned surface. Stylix reads this registry and
    # stands down for these surfaces (concepts/Notes).
    aoide.surfaces = {
      bar.owner = "quickshell";
      # dunst owns notification DELIVERY — the bus name, history, stacking and
      # the pause levels — but it draws NOTHING (`skip_display` on every rule;
      # modules/dendrites/dunst.nix). Quickshell owns the SURFACE: the `herald`
      # popup slot and the dock's `herald-center` ledger, both drawn from
      # stage/herald.json. The stand-down below keys on "not stylix", so
      # Stylix's dunst/mako targets stay disabled either way — which is still
      # what we want, since a Stylix-generated dunstrc would fight the
      # dendrite's.
      notifications.owner = "quickshell";
      launcher.owner = "quickshell";
      osd.owner = "quickshell";
      lockscreen.owner = "quickshell";
      greeter.owner = "quickshell";
      wallpaper.owner = "quickshell";
      agentWidgets.owner = "quickshell";
      sessionGraph.owner = "quickshell";
    };

    # ── Quickshell package ──────────────────────────────────────────────────
    # The upstream Quickshell package lives in the pre-declared flake input
    # (flake.nix wires inputs.quickshell for exactly this).
    environment.systemPackages = [
      quickshellPkg
      # The colonnade's `[ mixer ]` tag launches pavucontrol, and its bluetooth
      # bay writes the A2DP ↔ headset profile with `pactl set-card-profile`
      # (neither Quickshell.Bluetooth nor Quickshell.Services.Pipewire exposes
      # a card profile — checked against both modules' compiled qmltypes).
      # Both live here rather than in the audio dendrite because it is THIS
      # facet's widget that shells out to them.
      pkgs.pavucontrol
      pkgs.pulseaudio # for `pactl` only; pipewire-pulse remains the server
    ];

    # ── Deploy QML config tree into run/qml/ (rsync, not a symlink tree) ────
    # The config tree (bar/notif/launcher/OSD/lockscreen/greeter/wallpaper)
    # lands at /home/<user>/Aoide/run/qml/ — a gitignored root-runtime dir,
    # never the repo's checked-in tree (CONTRACTS.md §2).
    #
    # `home.file` would manage this as a tree of symlinks into the nix
    # store, which is the RIGHT semantics for most home-manager-installed
    # config — but this facet's whole point is that agents/dev iteration
    # hot-edit the deployed QML directly to preview without a rebuild
    # (Quickshell live-reloads on file change). A symlink tree makes that
    # workflow permanently hostile to the NEXT switch: home-manager finds a
    # real file where it expects to manage a symlink and refuses to
    # activate until every stray file is hand-diffed against the fresh
    # store build and removed — exactly the failure this rsync replaces.
    #
    # `rsync -a --delete` makes the deployed tree self-healing instead: a
    # switch always reasserts the store's truth over whatever was hand-
    # edited, rather than refusing to proceed. This is deliberate — hot
    # edits under run/qml/ are previews ("the sketch"); the next switch is
    # what makes a change real ("the truth"), same discipline as every
    # other stage/preview seam in this project.
    # NOTE: this is a home-manager submodule FUNCTION (`{ lib, ... }:`), not a bare
    # attrset — so `lib` here is home-manager's EXTENDED lib (carrying `lib.hm.dag`),
    # not the outer NixOS-module lib (which lacks `hm`). `config`/`pkgs`/`quickshellConfig`
    # still resolve lexically to the outer module scope, unshadowed by the pattern.
    # ── No song, no service ──────────────────────────────────────────────────
    # Every entry below reaches for the ACTIVE song (activeSongLivery,
    # seedStageScript's jq --arg, or a shell entry deployed FROM the song's
    # QML) — with `aoide.song` null there is no active song to bake, so this
    # whole block is `lib.optionalAttrs`-gated on `config.aoide.song != null`
    # rather than `lib.mkIf`: optionalAttrs never forces its `attrs` argument
    # when the condition is false (plain nix laziness, no module-system
    # merging involved at this attrset-literal level), so a null-song host's
    # eval never touches `${config.aoide.song}` here — no deployed QML, no
    # seeded stage, no `aoide-quickshell` unit; not an empty surface, simply
    # absent. A host that names a song gets exactly today's behaviour.
    home-manager.users.${config.aoide.user} =
      { lib, ... }:
      lib.optionalAttrs (config.aoide.song != null) {
        home.activation.aoideDeployQml = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          run mkdir -p "$HOME/Aoide/run"
          run ${pkgs.rsync}/bin/rsync -a --delete --chmod=u+w \
            "${quickshellConfig}/qml/" "$HOME/Aoide/run/qml/"
        '';

        # ── Seed the live stage twin from the active song ──────────────────────
        # `song/stage/livery.json` is what LiveryState.qml hot-reloads
        # (CONTRACTS.md §4); until now nothing seeded it from the BAKED default,
        # so a host that never ran `aoide rice preview <name>` had a stale/absent
        # stage twin even though the compositor/Stylix/QML tree were all built
        # from the active song. This reasserts the active song's committed livery
        # into the stage file on every activation — `seedStageScript` (above)
        # does the actual write. Same "switch = truth resets the sketch"
        # discipline as `aoideDeployQml`'s rsync above: this OVERWRITES whatever
        # a live `rice preview`/`cover set` staged, which is intended — the next
        # `rice preview` can re-sketch over it again live.
        home.activation.aoideSeedStage = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          run ${seedStageScript}
        '';

        # ── Quickshell autostart via systemd user service ─────────────────────
        # The shell surface (bar/dock/wallpaper/notifications/OSD) is started by
        # a systemd user service rather than a Hyprland exec-once. A service is
        # the stronger session-assembly seam:
        #   - Restart=on-failure — a QML crash respawns the whole shell instead
        #     of leaving the desktop bare until the next login.
        #   - journald — `journalctl --user -u aoide-quickshell` gives real logs
        #     (an exec-once child's stderr is lost).
        #   - graphical-session.target ordering — it starts only once the
        #     compositor facet's env handoff (hyprland-session.target →
        #     graphical-session.target, WAYLAND_DISPLAY/HYPRLAND_INSTANCE_SIGNATURE
        #     imported) has fired, so Quickshell inherits a valid Wayland env.
        # ConditionPathExists guards the shell entry so the unit fails cleanly
        # (not crash-loops) if the QML tree hasn't landed in the clone yet. That
        # guard only checks *existence*, though — a shell.qml that exists but
        # fails to load (a QML parse/load error, or an ExecStart pointed elsewhere
        # by a stray drop-in) still exits 255 and, under Restart=on-failure, would
        # respawn every RestartSec forever. StartLimit* is the backstop: after 5
        # failures inside 60s systemd stops trying and parks the unit `failed`
        # instead of thrashing the desktop (and journald) indefinitely. Five tries
        # still absorbs a genuinely transient failure (e.g. Wayland not ready yet).
        # The same condition doubles as the runtime-compose seam: a later lane
        # phase that materializes `run/qml/shell.qml` straight from a staged
        # rice (no rebuild) only has to write that file and start this unit —
        # ConditionPathExists is already the gate that lets it, and with a
        # declared song the rebuild path already satisfies it, so nothing
        # about this unit needs to change to grow that second producer.
        systemd.user.services.aoide-quickshell = {
          Unit = {
            Description = "Aoide Quickshell — shell surface (bar/dock/wallpaper/notifications)";
            PartOf = [ "graphical-session.target" ];
            After = [ "graphical-session.target" ];
            ConditionEnvironment = "WAYLAND_DISPLAY";
            ConditionPathExists = shellQmlEntry;
            StartLimitIntervalSec = 60;
            StartLimitBurst = 5;
          };
          Service = {
            # `-p <path>` loads a config by PATH; `-c <name>` (used previously)
            # treats the argument as a config NAME and fails on a path in
            # quickshell 0.3.0.
            ExecStart = "${quickshellPkg}/bin/quickshell -p ${shellQmlEntry}";
            # Qt6's qtbase ships only jpeg/png/gif/ico imageformats plugins (plus
            # qtsvg); webp/tiff/etc. live in a SEPARATE qtimageformats plugin the
            # quickshell wrapper does not carry. A song cover may be any of those
            # formats (sonata's is .webp), and AoideWallpaper renders it through a
            # Qt Image — with no webp plugin the Image can't decode and the layer
            # falls back to the solid palette colour (the "wallpaper didn't run
            # after rebuild" symptom). The wrapper sets QT_PLUGIN_PATH via
            # makeWrapper --prefix, which PRESERVES an inherited value, so this
            # plugin dir is scanned alongside the wrapper's own. quickshell follows
            # this flake's nixpkgs, so this qtimageformats is the exact Qt ABI.
            Environment = [
              "QT_PLUGIN_PATH=${pkgs.qt6.qtimageformats}/lib/qt-6/plugins"
            ]
            # Export the song's baked wallpaper (immutable store path) so
            # AoideWallpaper always has the right cover on boot/rebuild — the live
            # stage/cover.json overrides it, but nothing re-seeded it from the
            # song before, so a rebuild lost the background. Null → no env.
            ++ lib.optionals (config.aoide.livery.wallpaper != null) [
              "AOIDE_WALLPAPER=${config.aoide.livery.wallpaper}"
            ];
            Restart = "on-failure";
            RestartSec = 3;
          };
          Install.WantedBy = [ "graphical-session.target" ];
        };
      };
  };
}
