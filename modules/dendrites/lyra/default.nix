# modules/dendrites/lyra/default.nix — the lyra paint lane (Quickshell surface).
#
# Renders the complete Aoide shell surface using Quickshell (QML runtime).
# Surfaces owned: bar, notifications, launcher, osd, lockscreen, greeter,
# wallpaper, agentWidgets, sessionGraph.
#
# Reading discipline (CONTRACTS.md §1): a paint dendrite reads the dress, the
# structure, the surfaces and identity — nothing else (root AGENTS.md house
# rule 5). Here: `aoide.livery` (palette + component tiers), `aoide.surfaces`
# (the registry it declares its own surfaces in), `aoide.arrangement`, the
# core scalars (`aoide.enable`, `aoide.root`, `aoide.checkout`, `aoide.user`),
# the song selection (`aoide.song`) and the derived fact
# `aoide.songbook.builtIn` (the songs this host builds in — what this lane's
# deployed manifest, widget copy, installed packages, shipped templates and seed
# are built FROM) and its own fact.
#   - Component-tier fallback (null → palette) applied locally, never pushed
#     back into the option system.
#   - NEVER reads song/ runtime paths at build time (checks.song-shape and
#     checks.song-runtime-untracked are the structural half).
#
# Communication discipline (entities/Quickshell):
#   - QML reads state files from song/stage/ at runtime (hot-reload).
#   - QML issues commands via the shellbridge unix socket (shellbridge.nix
#     beside this file).
#   - QML never speaks MCP or any agent protocol.
let
  body =
    {
      config,
      lib,
      pkgs,
      aoideInputs,
      songbook,
      ...
    }:
    let

      # Committed songs live here (CONTRACTS.md §5) — versioned score, read at
      # build time (nothing here reads a song/ RUNTIME dir; the check that used
      # to assert that scanned module PATHS and is gone — what replaced it is
      # `checks.song-shape`'s `strayNixFiles` and `checks.song-runtime-untracked`).
      #
      # Handed in as a module argument by the same hook that sets
      # `aoide.songbook.builtIn` (lib/aoideos.nix, and every consumer's copy of
      # that wiring): a consumer's songs live in the CONSUMER's tree, so this
      # lane must not name a path inside Aoide's.

      # ── What this host BUILDS IN ────────────────────────────────────────────
      # `song.declared ∪ song.available` on its record, derived by the
      # constructor's hook into the fact `aoide.songbook.builtIn`. Everything below
      # that used to run over every committed song runs over these names
      # instead: the deployed manifest and registry, the widget bodies, the
      # installed packages, the shipped templates and the machine-songbook seed.
      #
      # A song that is NOT built in is still stageable from the machine's own
      # songbook (`aoide rice stage <n>`) — the runtime keeps a wider manifest
      # than this deployed tree carries, which is exactly the seam
      # `builtin.json`'s `packages` exists to check (§7.5).
      songbookLib = import ../../../lib/songbook.nix { inherit lib songbook; };

      # The `aoide.*` option doc list the shipped `aoide-options.json` carries
      # (`lib/options.nix`) — `lyra onboard`'s offline source, and the reason
      # the shipped `lyra-songbook` needs an argument `callPackage` cannot
      # fill. Derived here rather than handed down: this lane is what builds
      # `pkgs.lyra-songbook` for the host, and the value needs `aoideInputs`,
      # which is a module argument. Lazily forced — a host that never reads
      # `pkgs.lyra-songbook` never evaluates it.
      aoideOptions = import ../../../lib/options.nix {
        inherit lib aoideInputs;
        pkgs = aoideInputs.nixpkgs.legacyPackages.${pkgs.stdenv.hostPlatform.system};
      };

      builtIn = config.aoide.songbook.builtIn;

      # ── The songs this derivation reads, as their OWN store paths ────────────
      # The same contract `pkgs/lyra-songbook` keeps, for the same reason: a path
      # literal that names the songbook DIRECTORY makes every song in the repo
      # part of this derivation, and this derivation's path rides the deploy
      # activation — so editing a song this host does not even build in moved its
      # whole toplevel drvPath. `builtins.path` copies the folder, so the input
      # is exactly the built-in set. Same `name` as the package's copy of a song,
      # so the two derivations share one store path per song.
      songDirOf =
        name:
        builtins.path {
          path = songbook + "/${name}";
          name = "lyra-song-${name}";
        };

      # The deployed tree describes the songs the shell can actually resolve: a
      # slot record naming a song whose `widgets/` is not in this tree would
      # point at nothing.
      onlyBuiltIn = lib.filterAttrs (name: _: builtins.elem name builtIn);

      # ── Active song's committed livery — also a legitimate build-time read ─────
      # `song/songbook/<name>/livery.json` is versioned score
      # (checks.song-runtime-untracked only names song/{stage,auditions,declared}/,
      # never song/songbook/), so
      # naming the ACTIVE song's livery file here is allowed for the same reason
      # `songbook` above is. Path concatenation (not string-interpolating the
      # whole `songbook` dir) so only this one file gets copied into the store.
      activeSongLivery = songbook + "/${config.aoide.song}/livery.json";

      livery = import ../../../lib/livery.nix { inherit lib; };

      # The stage twin is the committed livery with the VENUE applied — the same
      # `aoide.livery.override` recolour the Stylix and compositor fan-outs get
      # (CONTRACTS.md §1, override tier), through the same `lib/livery.nix`. Not
      # a second rule: `stagePatch` is `resolve`'s two passes against the file's
      # own shape. Identity when the host sets no override, so a host without a
      # venue stages exactly the committed bytes it staged before.
      stageLivery = pkgs.writeText "aoide-stage-livery.json" (
        builtins.toJSON (
          livery.stagePatch config.aoide.livery (builtins.fromJSON (builtins.readFile activeSongLivery))
        )
      );

      # The active song's terminal opacity (`aoide.livery.geometry.terminalOpacity`,
      # CONTRACTS.md §1): `null` is "no opinion" and leaves the kitty dendrite's
      # baked value standing. The seed below publishes it as the one-line
      # `song/declared/terminal-opacity.conf`, which kitty includes BEFORE the
      # staged colours — so a host that has never staged a song still opens its
      # terminal at the song's opacity. This lane carries the read because it is
      # the lane that already reads the dress (root AGENTS.md rule 5); the kitty
      # lane names only the file's path under `aoide.root`.
      opacity = config.aoide.livery.geometry.terminalOpacity;

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
      manifestAttrs = onlyBuiltIn songbookLib.manifestAttrs;
      registryAttrs = onlyBuiltIn songbookLib.registryAttrs;

      manifestJsonFile = pkgs.writeText "aoide-quickshell-manifest.json" (builtins.toJSON manifestAttrs);
      registryJsonFile = pkgs.writeText "aoide-quickshell-registry.json" (builtins.toJSON registryAttrs);

      # ── Expected paint, published (v1 `aoide.arrangement.surfaces`) ─────────
      # The ACTIVE song's expected-paint declaration, published so a health
      # consumer can assert what SHOULD be mapped instead of only counting what
      # is (`lyra quickshell healthcheck`). Deliberately NOT keyed by song, unlike
      # the two files above: those describe the whole songbook because
      # `rice preview` can switch songs at runtime, while an expectation is about
      # what the ACTIVE song should have mapped right now — and
      # `config.aoide.arrangement.surfaces` already IS that (the song's rice.nix
      # self-gates on `config.aoide.song`). `song` names which song's declaration
      # this is; the keys are the RESOLVED layer-shell namespaces (`aoide-<slot>`
      # — the same derivation `widgetType.namespace` applies when null), so the
      # consumer compares them directly against the compositor's layer list and
      # derives nothing of its own. A song that declares none gets an empty
      # `surfaces` object — emitted rather than omitted, and read by the consumer
      # exactly as an absent file: no expectation, the old count decides
      # (CONTRACTS.md §5).
      surfacesJsonFile = pkgs.writeText "aoide-quickshell-surfaces.json" (
        builtins.toJSON {
          song = config.aoide.song;
          surfaces = lib.mapAttrs' (
            slot: entry: lib.nameValuePair "aoide-${slot}" entry
          ) config.aoide.arrangement.surfaces;
        }
      );

      # ── Seed script for `home.activation.aoideSeedStage` (below) ───────────────
      # Reasserts the ACTIVE song's committed livery, with the venue override
      # applied (`stageLivery`, above), into the live stage twin
      # (`song/stage/livery.json`, CONTRACTS.md §4) AND its declared twin
      # (`song/declared/livery.json`) on every activation, injecting
      # the same `"song"` field `aoide rice preview <name>` would (jq's
      # `. + {song: …}`; `-S` sorts keys to match serde_json::Value's BTreeMap
      # ordering) — byte-identical to what `rice preview ${config.aoide.song}`
      # would stage ONLY where the host sets no `aoide.livery.override` (verified
      # by hand: `jq -S '. + {song:"sonata"}'` against song/songbook/sonata/livery.json
      # reproduces the current staged file exactly). The declared twin is what the
      # runtime writers (`rice stage`, `rice mode stage`/`declarative`) read back
      # for the declared song — that song's notes with the venue recolour already
      # applied — so a re-stage after activation reproduces the venue rather than
      # reverting to the song's own colours.
      # Write-temp-then-rename in the SAME directory as each destination (so each
      # rename is atomic) mirrors `shellbridge::atomic_write`
      # (`aoide_storage::fs::atomic_write`) so a hot-reloading FileView
      # (LiveryState.qml) never reads a torn file. The stage write goes to
      # livery.json, the canonical stage livery file. The whole thing is one
      # script (not inline `run` commands) so a `--dry-run` activation either runs
      # it in full or not at all — never a half-applied mkdir/mktemp/jq/cp/mv
      # sequence.
      seedStageScript = pkgs.writeShellScript "aoide-seed-stage" ''
        set -euo pipefail
        mkdir -p "${config.aoide.root}/song/stage" "${config.aoide.root}/song/declared"
        tmp=$(mktemp "${config.aoide.root}/song/stage/.livery.json.XXXXXX")
        ${pkgs.jq}/bin/jq -S '. + {song: $song}' --arg song "${config.aoide.song}" \
          "${stageLivery}" > "$tmp"
        # The declared twin is the SAME bytes, published in its own directory
        # (CONTRACTS.md §4) — copied into a temp there rather than renamed across
        # directories, so the rename below can never cross a filesystem.
        declared=$(mktemp "${config.aoide.root}/song/declared/.livery.json.XXXXXX")
        cp "$tmp" "$declared"
        mv -f "$tmp" "${config.aoide.root}/song/stage/livery.json"
        mv -f "$declared" "${config.aoide.root}/song/declared/livery.json"
        # The active wallpaper setter's name — ONE word the CLI and the QML both
        # read (CONTRACTS.md §4), published here because this is the seed that
        # hands the runtime its facts and because choosing a provider is a
        # rebuild: the file follows the switch.
        provider=$(mktemp "${config.aoide.root}/song/stage/.wallpaper-provider.XXXXXX")
        printf '%s\n' ${lib.escapeShellArg config.aoide.wallpaper.provider} > "$provider"
        mv -f "$provider" "${config.aoide.root}/song/stage/wallpaper-provider"
        # No cover.json handling here: a cover carries the song it was staged
        # for, and the layer ignores one that names a song other than the
        # staged one (CONTRACTS.md §4) — so reseeding the staged song is enough.
        # The declared song's terminal opacity, as a one-line kitty fragment
        # (CONTRACTS.md §4). The kitty dendrite includes this BEFORE the staged
        # colours, and kitty's last-include-wins keeps a live stage authoritative
        # — so a host that has NEVER staged anything still opens its terminal at
        # the song's own opacity instead of the baked default, while a stage
        # still overrides it the moment one runs. `null` (no opinion) DELETES the
        # file rather than writing a default: absent is what makes kitty fall
        # through to the baked value, and a stale 0.7 from a previous song must
        # not outlive it.
        ${lib.optionalString (opacity == null) ''
          rm -f "${config.aoide.root}/song/declared/terminal-opacity.conf"
        ''}
        ${lib.optionalString (opacity != null) ''
          otmp=$(mktemp "${config.aoide.root}/song/declared/.terminal-opacity.conf.XXXXXX")
          printf 'background_opacity %s\n' "$(printf '%g' ${toString opacity})" > "$otmp"
          mv -f "$otmp" "${config.aoide.root}/song/declared/terminal-opacity.conf"
        ''}
      '';

      # ── QML root — the full skeleton config installed into run/qml/ ────────────
      # Each surface widget is a stub that reads its colors from livery. Deployed
      # to a gitignored root-runtime dir (run/, alongside catalog/index/log —
      # never the repo's checked-in tree) so the live copy can never be confused
      # with source (pkgs/lyra-shell/qml/) or collide with it — this is
      # what CONTRACTS.md §2 and the deploy-path fix this comment accompanies are
      # about. At runtime Quickshell hot-reloads from song/stage/livery.json via
      # a FileView; the build only installs the structural QML, not the note
      # values themselves.
      #
      # ── Per-song flavor widgets (CONTRACTS.md §5) ───────────────────────────
      # Beyond the shared, song-blind QML tree above, this also carries per-song
      # widget QML from song/songbook/<song>/widgets/ — versioned score, not
      # runtime — for the songs this host BUILT IN (`aoide.songbook.builtIn`, from
      # its record), so a live `aoide rice preview <name>` can hot-swap a
      # widget's BODY (not just its colours) with no rebuild. A song built in as
      # `available` but not declared is carried here too: that is what "can stage
      # without a rebuild" means. The
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
        cp -r ${pkgs.lyra-shell}/share/lyra/qml/. "$out/qml/"

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
        # The ACTIVE song's expected-paint declaration (CONTRACTS.md §5) —
        # `surfacesJsonFile` above. NOT keyed by song, unlike the two above it:
        # the expectation is about what the active song should have mapped now,
        # and the option it comes from already is that. A consumer finds this file
        # absent (an older host, or a lane that never deployed) or empty (a song
        # that declares none) and falls back to its previous behaviour.
        surfaces="$out/qml/songs/surfaces.json"
        jq . ${surfacesJsonFile} > "$surfaces"

        # ── Carry over per-song flavor widgets ──────────────────────────────────
        # Copy the WHOLE widgets/ dir per song (bring any helper .qml/asset
        # subdirs along — a multi-file widget like sonata's bar.qml needs its
        # WorkspaceRow.qml helper sitting right beside it). A file starting
        # uppercase is a helper component (QML's own type-file convention: only
        # an uppercase-first filename is a valid QML type name) — carried to
        # disk so the slot file's relative imports resolve, but never
        # independently resolvable as a slot itself (nix already decided the
        # slot list, above — this loop no longer re-derives it).
        # Each song's folder is a nix-built store path (`songDirOf`), handed to
        # the shell as `<name>:<dir>`; nothing here interpolates the songbook
        # directory, so this derivation reads the built-in songs and no others.
        for entry in ${lib.concatStringsSep " " (map (name: "\"${name}:${songDirOf name}\"") builtIn)}; do
          name="''${entry%%:*}"
          d="''${entry#*:}/"
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
            # the helper components moved out of the shell's own qml/ directory
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

      # The runtime root this lane deploys into: $AOIDE_ROOT/run/qml — the
      # rsync-deployed tree (see the home.activation entries below), never the
      # repo's source tree. L-C2 (task #107): the runtime root moved off
      # `~/Aoide` onto `aoide.root` (default `~/.aoide`).
      qmlDir = "${config.aoide.root}/run/qml";
    in
    {
      # shellbridge is this capability's other half (lyra owns the bridge,
      # docs/architecture/PACKAGE-LAYOUT.md): the file self-gates on the facts, so
      # importing it beside this lane is the whole wiring.
      imports = [ ./shellbridge.nix ];

      # ── Config: only wired when the lyra fact is on ────────────────────────────
      # `aoide.lyra.enable` is a FACT, declared once in
      # modules/nucleus/options.nix, and this lane's `nixos` half sets it
      # `mkDefault true`.
      config = lib.mkIf config.aoide.lyra.enable {

        # ── What this lane brings into existence ────────────────────────────────
        # One thing, and it is a FACT rather than this lane's own option: the
        # directory the shell runs. Quickshell itself is the `quickshell` lane's
        # (package and the `aoide-quickshell` service); no module reads another
        # module (root AGENTS.md house rule 5), so this lane NAMES the config it
        # deploys into and the shell lane reads that name. Lyra paints a song —
        # deploy the tree, seed the stage, restart the rice — and the shell lane
        # is what knows how to run a shell on the result.
        #
        # The shell lane spells `<dir>/shell.qml` for quickshell's entry point,
        # so `$AOIDE_ROOT/run/qml` here is the same command line as the day the
        # service lived in this file.
        aoide.quickshell.config = qmlDir;

        # ── The templates this host ships ───────────────────────────────────────
        # `pkgs.lyra-songbook` is the baseline `AOIDE_SONG_TEMPLATES`: what
        # `lyra rice compose` and the repo-less manifest regeneration fall back
        # to. Its default is the WHOLE committed songbook (the flake's own
        # `packages` output); a host overrides it with what it actually built
        # in, so a `_server` ships an empty templates dir and yomi ships
        # sonata's folder and no other. `builtin.json` records the selection
        # itself — `{ declared, songs, packages }` — which is what runtime
        # staging checks a non-built-in song's needs against before refusing to
        # stage it (the machine owns its songbook; the baseline is the fallback).
        #
        # An overlay rather than a reference, because the templates dir is read
        # by units that resolve `pkgs.lyra-songbook` themselves (`aoided`'s
        # session variables, shellbridge's `AOIDE_SONG_TEMPLATES`) — house rule
        # 5: neither of those names this lane.
        nixpkgs.overlays = [
          (_final: prev: {
            # A FRESH `callPackage`, never `.override` on whatever the walker's
            # overlay left in `prev`: application order across the base
            # (lib/aoideos.nix) and this lane is not a contract, so
            # `prev.lyra-songbook` may not exist yet. A fresh call also names
            # every argument here rather than inheriting them — including
            # `aoideOptions`, which this lane derives itself because the
            # walker's own `extra` (lib/pkgs.nix) is that overlay's business,
            # not this lane's. Both overlays may define the name; the walker's
            # step-aside makes whichever is applied second yield, so this
            # lane's answer wins in either order.
            lyra-songbook = prev.callPackage ../../../pkgs/lyra-songbook {
              songs = builtIn;
              builtin = {
                declared = config.aoide.song;
                songs = builtIn;
                packages = songbookLib.packagesFor builtIn;
              };
              inherit aoideOptions songbook;
            };
          })
        ];

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

        # ── What the shell needs to speak to hardware ───────────────────────────
        # The Quickshell PACKAGE and its service belong to the `quickshell` lane
        # (it is the lane that knows how to run a shell, with or without lyra);
        # these two live here because it is THIS lane's widget that shells out to
        # them.
        #
        #   pkgs.pavucontrol — the colonnade's `[ mixer ]` tag launches it.
        #   pkgs.pulseaudio  — for `pactl` only; pipewire-pulse remains the
        #                      server. The bluetooth bay writes the A2DP ↔
        #                      headset profile with `pactl set-card-profile`
        #                      (neither Quickshell.Bluetooth nor
        #                      Quickshell.Services.Pipewire exposes a card
        #                      profile — checked against both modules'
        #                      compiled qmltypes).
        #
        # Sonata's `_widgets/bar.nix` declares those same two names (the record
        # is what a song says it needs), so the hard-coded pair and
        # `packagesFor builtIn` agree today — `lib.unique` is what makes that a
        # checked coincidence rather than an assumption: the union is a set, not
        # a concatenation, and a song declaring a package the lane already
        # carries adds nothing.
        environment.systemPackages = lib.unique (
          [
            pkgs.pavucontrol
            pkgs.pulseaudio # for `pactl` only; pipewire-pulse remains the server
          ]
          # …plus what the built-in songs' own widgets declare. `composeSong`
          # reads each `_widgets/` record's `packages` (lib/song.nix); until this
          # slice nothing installed them. A shelf-less song declares none —
          # there is no record to declare them in.
          ++ map (name: pkgs.${name}) (songbookLib.packagesFor builtIn)
        );

        # ── Deploy QML config tree into run/qml/ (rsync, not a symlink tree) ────
        # The config tree (bar/notif/launcher/OSD/lockscreen/greeter/wallpaper)
        # lands at /home/<user>/Aoide/run/qml/ — a gitignored root-runtime dir,
        # never the repo's checked-in tree (CONTRACTS.md §2).
        #
        # `home.file` would manage this as a tree of symlinks into the nix
        # store, which is the RIGHT semantics for most home-manager-installed
        # config — but this lane's whole point is that agents/dev iteration
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
        # ── The machine's own songbook first (house rule 10) ─────────────────────
        # The machine OWNS `~/.aoide/song/songbook` (CONTRACTS.md §5): its
        # built-in songs plus whatever it made itself, never a link to or a sync
        # of the repo. Each built-in song's folder is copied in ONLY when that
        # folder does not exist — no comparison, no merge, never an overwrite.
        # `sonata/takes/`, `sonata/drafts/` and edited `design/` notes on a
        # machine that already holds them survive every rebuild.
        #
        # Gated on the BUILT-IN set, not on the active song: a host that builds a
        # song in as `available` without performing it still gets seeded, and a
        # host that built none in gets no seed step at all. Its OWN definition of
        # the user's home — two definitions of one submodule MERGE (that is the
        # module system's job), while a single `//` between them would take the
        # right-hand `home` whole and silently drop the other's activation
        # entries.
        #
        # The source is the deployed baseline (`pkgs.lyra-songbook`, which this
        # host already restricted to its built-in set). `[ ! -e ]` is a read,
        # evaluated even under `--dry-run`, while `run cp` is not: a dry run over
        # an existing songbook writes nothing, and one over an absent song prints
        # the copy it would make.
        home-manager.users.${config.aoide.user} =
          { lib, ... }:
          {
            # Two gates, two MODULES — merged by the module system, not by `//`
            # (which merges one level deep: the second `home` would take the
            # whole key and drop the first one's activation entries), and not by
            # two assignments to this option path, which one module file may not
            # make.
            imports = [
              (lib.optionalAttrs (builtIn != [ ]) {
                home.activation.aoideSeedSongbook = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
                  songbookDir="${config.aoide.root}/song/songbook"
                  run ${pkgs.coreutils}/bin/mkdir -p "$songbookDir"
                  ${lib.concatMapStrings (name: ''
                    if [ ! -e "$songbookDir/${name}" ] && [ ! -L "$songbookDir/${name}" ]; then
                      run ${pkgs.coreutils}/bin/cp -r "${pkgs.lyra-songbook}/share/lyra/songbook/${name}" "$songbookDir/${name}"
                    fi
                  '') builtIn}
                '';
              })

              # ── No song, nothing to paint ──────────────────────────────────────
              # Every entry below reaches for the ACTIVE song (activeSongLivery,
              # seedStageScript's jq --arg, or a shell entry deployed FROM the song's
              # QML) — with `aoide.song` null there is no active song to bake, so this
              # whole block is `lib.optionalAttrs`-gated on `config.aoide.song != null`
              # rather than `lib.mkIf`: optionalAttrs never forces its `attrs` argument
              # when the condition is false (plain nix laziness, no module-system
              # merging involved at this attrset-literal level), so a null-song host's
              # eval never touches `${config.aoide.song}` here — no deployed QML, no
              # seeded stage, no restart of the rice; not an empty surface, simply
              # absent. A host that names a song gets exactly today's behaviour.
              #
              # The `aoide-quickshell` unit is NOT part of this gate: it belongs to the
              # `quickshell` lane, which starts a shell on whatever config this lane
              # deployed — or on a directory the host brought itself, song or no song.
              # A null-song host with no such directory still runs no shell (that lane's
              # own bare case).
              (lib.optionalAttrs (config.aoide.song != null) {
                home.activation.aoideDeployQml = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
                  run mkdir -p "${config.aoide.root}/run"
                  run ${pkgs.rsync}/bin/rsync -a --delete --chmod=u+w \
                    "${quickshellConfig}/qml/" "${config.aoide.root}/run/qml/"
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
                # a live `rice preview` staged, which is intended — the next
                # `rice preview` can re-sketch over it again live. It reseeds
                # `song/stage/livery.json` ONLY: `song/stage/cover.json` is left alone
                # (CONTRACTS.md §4).
                home.activation.aoideSeedStage = lib.hm.dag.entryBetween [ "reloadSystemd" ] [ "writeBoundary" ] ''
                  run ${seedStageScript}
                '';

                # ── Reassert the paint on EVERY activation ─────────────────────────────
                # A rebuild only restarts units whose definitions changed, so a
                # long-running quickshell survives every no-diff switch — including one
                # that has silently lost its Wayland outputs and moved the whole scene
                # onto a placeholder screen (live incident 2026-08-28: "There are no
                # outputs - creating placeholder screen"; unit active, desktop bare).
                # House ruling: activation always brings the rice elements back up.
                # try-restart bounces a running shell onto the freshly rsynced tree and
                # re-acquired outputs, no-ops when the unit is stopped (headless/
                # session-less activation must not start or fail anything), and never
                # fails the switch. Ordered after both writes above so the restarted
                # shell reads the new tree, never the old one.
                #
                # This is the REBUILD-side half of the recovery, not the whole of it:
                # the identical lockup recurring live, mid-session, with no rebuild in
                # sight is what `aoide-quickshell-healthcheck.timer` (below) exists to
                # catch — of the two, that timer is the only one that ever runs
                # unprompted.
                home.activation.aoideRestartRice =
                  lib.hm.dag.entryAfter
                    [
                      "aoideDeployQml"
                      "aoideSeedStage"
                    ]
                    ''
                      run env XDG_RUNTIME_DIR=/run/user/$(${pkgs.coreutils}/bin/id -u) \
                        ${pkgs.systemd}/bin/systemctl --user try-restart aoide-quickshell.service || true
                    '';

                # ── Confirm the restart above actually landed ──────────────────────────
                # `aoideRestartRice` is fire-and-forget: `try-restart ... || true` means
                # a switch reports success whether the shell came back painted or landed
                # straight in the placeholder-screen lockup its own restart was meant to
                # fix. `aoide-quickshell-healthcheck.timer` (below) WOULD catch that
                # within ~15s regardless — this entry doesn't close a hole the timer
                # leaves open, it exists so the confirmation is immediate and visible in
                # the rebuild output itself, instead of waiting on the first timer tick
                # (or on someone noticing a bare desktop). Sleep 5s first: quickshell
                # logs "Configuration Loaded" ~1s after launch and its layer surfaces
                # follow shortly after (live journal), so checking instantly would just
                # race the shell's own startup — not a false positive (health.rs's
                # journal-line gate means a fresh clean start always reads healthy,
                # never falsely stuck), just a wasted check. Gated on `aoide.lyra.enable`
                # like the healthcheck units below, since it execs the same lyra binary
                # they do — folded into the script body (an `''${lib.optionalString …}''`
                # around the run lines) rather than `lib.mkIf` on the whole DAG-entry
                # value or `lib.optionalAttrs` around this binding, since a plain `if`
                # inside the script is the one idiom that never has to ask whether
                # `mkIf` composes through `hm.dag.entryAfter`'s attrset shape.
                home.activation.aoideVerifyRice = lib.hm.dag.entryAfter [ "aoideRestartRice" ] ''
                  ${lib.optionalString config.aoide.lyra.enable ''
                    run ${pkgs.coreutils}/bin/sleep 5
                    run env XDG_RUNTIME_DIR=/run/user/$(${pkgs.coreutils}/bin/id -u) \
                      ${pkgs.aoide.rice}/bin/lyra quickshell healthcheck || true
                  ''}
                '';

                # ── Live mid-session watchdog: the placeholder-screen lockup ───────────
                # `aoideRestartRice` (above) only reasserts the paint on a REBUILD.
                # Nothing caught the SAME failure live, mid-session, with no rebuild in
                # sight — the incident it documents recurred twice more the next day
                # (2026-08-29), the last one unnoticed for ~9 hours. `Restart=always`
                # is structurally blind to this lockup: the process never exits, it just
                # sits `active` painted onto Qt's internal placeholder screen, so
                # systemd has nothing to restart on. `lyra quickshell healthcheck`
                # (crates/song/src/health.rs) is the periodic check that closes the gap
                # — it confirms BOTH the journal's placeholder-screen line AND a live
                # `hyprctl layers` zero-surface reading before acting (health.rs's own
                # module doc carries the two-signal reasoning), then restarts the unit
                # on a retry ladder rather than immediately every tick: immediate on
                # the first restart, then 15s/60s/5m, settling at a 15-minute floor it
                # never drops below — but never gives up either, so a genuinely
                # flapping output still gets restarted forever instead of eventually
                # being abandoned. A unit that is installed but stopped or parked
                # `failed` (StartLimit) is reported as not running and left alone:
                # restarting it would undo a deliberate stop or a backstop.
                #
                # `quickshell` is a lyra-only command family (left core's registry at
                # P-A5), so this execs `pkgs.aoide.rice` — lyra's own droppable output
                # (P-A8) — the same reference shellbridge.nix's `shellbridge` service
                # uses for the same reason. Gated on `aoide.lyra.enable` like that
                # service too: a host that flips it off while leaving this lane on
                # must not start a unit that execs a binary this build left uninstalled.
                #
                # Unlike `shellbridge`/`aoide-graph-reap` (NixOS-level `systemd.user.
                # services`, whose module gives a flat `path`/`description` sugar),
                # this pair is nested inside `home-manager.users.${config.aoide.user}`
                # like `aoide-quickshell` right above — home-manager's OWN systemd
                # module has no such sugar (checked against its `modules/systemd.nix`:
                # only `Unit`/`Service`/`Install` freeform sections exist, and no
                # `path` option at all), so this follows `aoide-quickshell`'s own
                # Unit/Service/Install shape instead, with `hyprctl`/`notify-send`
                # (needed by health.rs's `hyprctl_json`/`notify_still_flapping`,
                # neither of which is guaranteed present on this manager's PATH) added
                # via an explicit `PATH=` `Environment` entry — the only knob this
                # schema offers for it. `systemctl`/`journalctl` ride the same
                # `lib.makeBinPath` list (health.rs's `active_enter_timestamp`/
                # `journal_tail_since`/`restart_service` all shell out to them) rather
                # than leaning on an ambient default the way `aoide-graph-reap` does
                # under NixOS's richer default environment.
                systemd.user.services.aoide-quickshell-healthcheck = lib.mkIf config.aoide.lyra.enable {
                  Unit = {
                    Description = "Aoide Quickshell healthcheck — detect and recover a placeholder-screen lockup Restart=always cannot catch";
                    PartOf = [ "graphical-session.target" ];
                    After = [ "graphical-session.target" ];
                  };
                  Service = {
                    Type = "oneshot";
                    ExecStart = "${pkgs.aoide.rice}/bin/lyra quickshell healthcheck";

                    Environment = [
                      "PATH=${
                        lib.makeBinPath [
                          pkgs.systemd
                          pkgs.hyprland
                          pkgs.libnotify
                        ]
                      }"
                      # Same "a configured aoide.root must win" reasoning as
                      # aoide-quickshell's own Environment above — health.rs's restart
                      # backoff marker lives under $AOIDE_ROOT/state.
                      "AOIDE_ROOT=${config.aoide.root}"
                    ];

                    NoNewPrivileges = true;
                    StandardOutput = "journal";
                    StandardError = "journal";
                  };
                };

                systemd.user.timers.aoide-quickshell-healthcheck = lib.mkIf config.aoide.lyra.enable {
                  Unit = {
                    Description = "Aoide Quickshell healthcheck timer — periodic placeholder-screen sweep (~15s)";
                    PartOf = [ "graphical-session.target" ];
                  };
                  Timer = {
                    # First check 20s after the session comes up (let the shell finish
                    # its own startup/output-negotiation first); thereafter every 15s
                    # since the previous run finished — cheap enough (one systemctl
                    # show, a short journalctl tail, two hyprctl calls) to run often,
                    # and every check speeds up how quickly a live lockup is noticed.
                    OnActiveSec = "20s";
                    OnUnitActiveSec = "15s";
                    AccuracySec = "2s";
                  };
                  Install.WantedBy = [ "graphical-session.target" ];
                };
              })
            ];
          };
      };
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.lyra.enable = lib.mkDefault true;
    };
}
