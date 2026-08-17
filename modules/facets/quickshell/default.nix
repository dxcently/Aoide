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
  t = config.aoide.livery;

  # Committed songs live here (CONTRACTS.md §5) — versioned score, legitimately
  # walked at build time (checks.no-song-read only bans song/ RUNTIME infixes,
  # never song/songbook/).
  songbook = ../../../song/songbook;

  # ── Active song's committed notes — also a legitimate build-time read ──────
  # `song/songbook/<name>/livery.json` is versioned score (checks.no-song-read
  # only bans song/{stage,auditions,catalog,index}/, never song/songbook/), so
  # naming the ACTIVE song's notes file here is allowed for the same reason
  # `songbook` above is. Path concatenation (not string-interpolating the
  # whole `songbook` dir) so only this one file gets copied into the store.
  activeSongNotes = songbook + "/${config.aoide.song}/livery.json";

  # ── Seed script for `home.activation.aoideSeedStage` (below) ───────────────
  # Reasserts the ACTIVE song's committed notes into the live stage twin
  # (`song/stage/livery.json`, CONTRACTS.md §4) on every activation, injecting
  # the same `"song"` field `aoide rice preview <name>` would (jq's
  # `. + {song: …}`; `-S` sorts keys to match serde_json::Value's BTreeMap
  # ordering) — byte-identical to what `rice preview ${config.aoide.song}`
  # would stage (verified by hand: `jq -S '. + {song:"sonata"}'` against
  # song/songbook/sonata/livery.json reproduces the current staged file
  # exactly). Write-temp-then-rename in the SAME directory (so the rename is
  # atomic) mirrors `shellbridge::atomic_write` (`aoide_storage::fs::atomic_write`)
  # so a hot-reloading FileView (LiveryState.qml) never reads a torn file.
  # The write goes to livery.json, the canonical stage note file. The whole
  # thing is one script
  # (not inline `run` commands) so a `--dry-run` activation either runs it in
  # full or not at all — never a half-applied mkdir/mktemp/jq/mv sequence.
  seedStageScript = pkgs.writeShellScript "aoide-seed-stage" ''
    set -euo pipefail
    mkdir -p "$HOME/Aoide/song/stage"
    tmp=$(mktemp "$HOME/Aoide/song/stage/.livery.json.XXXXXX")
    ${pkgs.jq}/bin/jq -S '. + {song: $song}' --arg song "${config.aoide.song}" \
      "${activeSongNotes}" > "$tmp"
    mv -f "$tmp" "$HOME/Aoide/song/stage/livery.json"
  '';

  # ── Component-tier fallback helpers ────────────────────────────────────────
  # Each is: use the component override when set, else fall back to the palette.
  # Facets apply the fallback here (CONTRACTS.md §1 rule: "Facets apply the
  # fallback, not the option system").
  barBg = if t.bar.bg != null then t.bar.bg else t.palette.bg;
  barFg = if t.bar.fg != null then t.bar.fg else t.palette.fg;
  barAccent = if t.bar.accent != null then t.bar.accent else t.palette.accent;

  notifBg = if t.notif.bg != null then t.notif.bg else t.palette.bg;
  notifFg = if t.notif.fg != null then t.notif.fg else t.palette.fg;
  notifUrgent = if t.notif.urgent != null then t.notif.urgent else t.palette.urgent;

  # ── QML root — the full skeleton config installed into run/qml/ ────────────
  # Each surface widget is a stub that reads its colors from notes. Deployed
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
  # (outside widgets/) are ignored. A generated manifest.json records which
  # songs authored which slots, so runtime QML (the staging engine,
  # StagingEngine.qml) can check availability without probing the filesystem
  # per-frame. The wired-slot catalog (which slots an anchor actually
  # resolves at runtime) lives in qml/slots.md, not here.
  #
  # ── Declared widget-type registry (Phase 2) ─────────────────────────────
  # The same per-song walk also generates $out/qml/songs/registry.json,
  # shaped `{ "<song>": { "<slot>": { …declaration… } } }` — which slots a
  # song declares as widget-TYPE registrations (a rarer, smaller set than
  # manifest.json's "which slot bodies exist"). Read from each committed
  # song's `livery.json` (`.widgets // {}`), never from any nix option:
  # `config.aoide.arrangement.widgets` (Phase 1) only exists for the ACTIVE song
  # at eval time (a song's rice.nix self-gates on `config.aoide.song ==
  # "<name>"`), so cross-song data has to come from committed FILES, same
  # reason manifest.json already reads the songbook off disk instead of
  # nix options. Every committed song gets an entry — `{}` when the song has
  # no `livery.json` or no `.widgets` key — never an error, never a skipped
  # song (same optional/empty-registry tolerance used elsewhere). This walk
  # stays permissive: whatever's under `.widgets` passes through unjudged;
  # shape validation is Phase 3's job (the Rust-side `rice lint` engine).
  quickshellConfig = pkgs.runCommand "aoide-quickshell-config" { nativeBuildInputs = [ pkgs.jq ]; } ''
    mkdir -p "$out/qml"
    cp -r ${./qml}/. "$out/qml/"

    # ── Carry over per-song flavor widgets + manifest ──────────────────────
    # Copy the WHOLE widgets/ dir per song (bring any helper .qml/asset
    # subdirs along — a multi-file widget like sonata's bar.qml needs its
    # WorkspaceRow.qml helper sitting right beside it), but only MANIFEST
    # top-level lowercase-kebab .qml files as slots. A file starting
    # uppercase is a helper component (QML's own type-file convention: only
    # an uppercase-first filename is a valid QML type name) — carried to
    # disk so the slot file's relative imports resolve, but never
    # independently resolvable as a slot itself. `.gitkeep` (present so an
    # empty widgets/ dir survives git) is always skipped.
    mkdir -p "$out/qml/songs"
    manifest="$out/qml/songs/manifest.json"
    echo '{}' > "$manifest"
    registry="$out/qml/songs/registry.json"
    echo '{}' > "$registry"
    for d in ${songbook}/*/; do
      name=$(basename "$d")
      slots=""
      if [ -d "$d/widgets" ]; then
        mkdir -p "$out/qml/songs/$name"
        cp -r "$d/widgets/." "$out/qml/songs/$name/"
        for f in "$d/widgets/"*; do
          [ -e "$f" ] || continue
          base=$(basename "$f")
          case "$base" in
            .gitkeep) continue ;;
          esac
          # lowercase-kebab .qml only: starts with [a-z0-9], ends .qml.
          case "$base" in
            [a-z0-9]*.qml) ;;
            *) continue ;;
          esac
          slot=$(basename "$base" .qml)
          slots="$slots $slot"
        done
      fi
      if [ -n "$slots" ]; then
        slotsJson=$(printf '%s\n' $slots | jq -R . | jq -s .)
        tmp=$(mktemp)
        jq --arg name "$name" --argjson slots "$slotsJson" \
          '.[$name] = $slots' "$manifest" > "$tmp"
        mv "$tmp" "$manifest"
      fi

      # ── Declared widget-type registry entry for this song ────────────────
      # `.widgets // {}` off the song's committed livery.json — `{}` when
      # the file is absent or carries no `.widgets` key. Every song gets an
      # entry (unlike manifest.json's slots, which only merge when
      # non-empty) since an empty registration set is itself meaningful
      # output, not an omission.
      widgets='{}'
      if [ -f "$d/livery.json" ]; then
        widgets=$(jq -c '.widgets // {}' "$d/livery.json")
      fi
      tmp=$(mktemp)
      jq --arg name "$name" --argjson widgets "$widgets" \
        '.[$name] = $widgets' "$registry" > "$tmp"
      mv "$tmp" "$registry"
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
    home-manager.users.${config.aoide.user} = { lib, ... }: {
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
      # from the active song. This reasserts the active song's committed notes
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
