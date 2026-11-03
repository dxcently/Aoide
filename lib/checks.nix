# lib/checks.nix — the contractual coupling discipline, as flake checks.
#
# These checks ride as flake `checks` (see concepts/Governance and
# concepts/Notes in the wiki, and the mechanical-integrity design for fmt +
# discovery specifically):
#
#   1. surface-ownership — no render surface may have two owners. The
#      strillix lane declares `strillix.surfaces.<name>.owner`; the stylix lane reads
#      the same registry and disables derivation for owned surfaces. Two
#      modules claiming the same surface is a build-time error.
#
#   2. rice-runtime-untracked — a rice's RUNTIME dirs (`$STRILLIX_ROOT/stage/`,
#      `rice/auditions/`, `$STRILLIX_ROOT/declared/`) must not exist in the source tree at
#      all. This replaces `no-song-read`, which scanned module PATHS for those
#      names and was vacuously true — a module never reads through its own path.
#      What actually keeps runtime state out of evaluation is that pure flake
#      eval reads only TRACKED files and `.gitignore` covers those three; this
#      check is what fails if one is ever committed or left behind.
#
#   3. rice-shape — a committed rice under rices/<name>/ carries its
#      body (`rice.nix`), its `theme.json` and its `hyprland.conf`, the body
#      is the aggregation shape, the `hyprland.conf` keeps the theme's keys
#      out, and a rice's module values sit under lib.mkDefault. No `.nix`
#      anywhere else: an extra module in a rice folder would join the merge
#      silently and could set arbitrary host options.
#
#   4. fmt — `nixfmt --check` over every `.nix` file in the flake's own
#      COMMITTED source (flake.nix's declared `formatter`, run by nothing
#      until this check existed).
#
#   5. discovery — every `pkgs/<name>/` directory is a recognised shape
#      (shelved, callPackage target, or self-flaked) per lib/pkgs.nix's own
#      discovery rule; a fourth shape is a half-created package that vanishes
#      from `packages`/`pkg-<name>` silently today.
#
#   6. phantom-commands — every backticked `aoide …` / `strillix …` invocation
#      taught by the agent docs (root AGENTS.md + docs/agent/*.md) resolves
#      against the right binary's `schema --json`, built from this checked-out
#      source. A doc teaching a command the registry no longer carries turns
#      the check red naming the file, line, and spelling — the gate that stops
#      doc copies of the command surface from growing back.
#
#   7. nix-independence — AGENTS.md's claim that core (`aoide`/`aoided`) is
#      cargo-buildable, no nix shell-outs, no NixOS assumption, made real
#      instead of discipline-only. Derives the crate closure from
#      `pkgs/aoide/Cargo.toml`'s `[workspace.dependencies]` and a transitive
#      walk of every `[dependencies]`/`[build-dependencies]` table, target
#      tables included, from `crates/cli` (never `[dev-dependencies]` —
#      test-support never ships), then asserts the closure never reaches
#      `crates/strillix-rice`, `crates/screen` or `crates/strillix`, and that
#      no closure crate's `src/` shells out to nix.
#
#   8. portability — the static artifact (`packages.aoide-static`, Phase C)
#      is genuinely free of nix, not merely built with the right cargo flags:
#      `aoide`/`aoided` carry zero `PT_INTERP` segments, bake in no real
#      `/nix/store/<hash>-…` path, and run `schema --json` / `guide` with no
#      nix on PATH. Content lives in tests/portability.nix — this file only
#      invokes it (the lib/ = wiring, tests/ = content split, see
#      tests/README.md).
#
#   9. nix-lint — `statix check` over the committed tree, the style half
#      Check 4 does not cover. Pairs with `statix.toml` at the repo root,
#      which exempts the flat-option-assignment house style; every other
#      lint is a hard failure naming file, line, and lint code.
#
#   10. theme-fanout — the two fan-outs (baked Stylix, `$STRILLIX_ROOT/stage/
#       theme.json` seed) resolve an `strillix.theme.override` identically.
#       Proves the general law against a fixed in-file fixture (never a real
#       host or committed rice): `lib/theme.nix`'s `stagePatch`'s palette
#       output matches `resolve`'s under a one-anchor override,
#       `stagePatch` is the identity with no override set, `venueDelta`
#       is exactly the one recoloured slot (`{ }` with no override), and a
#       host-set geometry field is laid over the document's own — created when
#       the document has none, shown in `venueDelta` only where it differs.
#
#   11, 12. generator-offline, generator-relocatable — the shipped rices
#       generator reproduces `ricesManifest` offline, and from a relocated
#       copy of the rices (their own comment, below).
#
#   13. bar-ricemode — every bar body embeds the shared rice-mode control:
#       the literal `slot: "ricemode"` is in the body of every distinct bar the
#       rices's manifest names (a borrowed bar is checked once, under its
#       owner; a rice with no bar of its own renders sonata's), and sonata
#       carries the `ricemode` record every bar's slot falls back to.
#
#   14. boundary — Aoide and Strillix share this repo and are kept from naming
#       each other, and this is the ratchet on it. `tests/boundary/scan.pl`
#       counts, per file, the lines one product's files spend on the other's
#       name and every Cargo edge from a crate on one side to a crate on the
#       other, then holds those rows against `tests/boundary/crossings.golden`:
#       a crossing the golden lacks fails the build, and so does a golden row
#       whose crossing is gone, until `scan.pl tighten` shrinks it. Content
#       lives in tests/boundary/ — this file only invokes it (the same split
#       as 8).
#
#   15. generator-runtime — the built `strillix` stages through the real
#       `nix-instantiate`, as a host does. `rice refresh` and `rice mode stage`
#       must succeed. A built-in rice's machine copy, edited, must stage with
#       status ok, and its marker must end the staged bar.qml: that proves the
#       machine copy, not the shipped one, is what stages. Both run-tree JSON
#       files must hold that rice's key. The baked baseline also satisfies that,
#       so it proves only that the files exist. A full copy of that rice under
#       a new name, `runtime-copy`, must be absent from both shipped JSON files
#       (checked first), stage with status ok, and leave a `runtime-copy` key in
#       both run-tree files. Only the generator can supply that key, so this is
#       the assertion that proves the generator's answer reaches the run tree.
#       Checks 11 and 12 call the generator themselves, so only this one
#       exercises the call the runtime makes.
#
# 1–3 and 5 are written so they PASS TRIVIALLY where nothing populates the
# registry they inspect yet (1) and become real as Wave-1 lanes/packages
# land. Each resolves to a trivial derivation: it either builds (assertion
# held) or the eval fails with a readable message (assertion broken). 10 is
# the same eval-time-assert shape against a fixed fixture instead of a
# registry — always real, never trivially-empty. 13 is that shape over the
# committed rices, one `readFile` per bar body. 4, 6, 7, 8, 9, 11, 12, 14,
# and 15 are real `runCommand`s — each has to actually run a binary, so it
# can only fail at build time, not eval time.
{ lib, pkgs }:
let
  # A check that succeeds as a buildable derivation, or throws at eval time
  # with `msg` when `cond` is false. Throwing at eval (not build) is deliberate:
  # `nix flake check` should fail fast and legibly on a contract violation.
  assertCheck =
    name: cond: msg:
    if cond then
      pkgs.runCommand "aoide-check-${name}" { } ''
        printf 'aoide check %s: ok\n' "${name}" > "$out"
      ''
    else
      throw "aoide check ${name} FAILED: ${msg}";

  # ── Check 1: surface ownership ────────────────────────────────────────────
  # Consumes the evaluated `strillix.surfaces` registry. Duplicate detection is a
  # no-op today because attrsets cannot hold duplicate keys; the real teeth
  # arrive when a lane asserts, in its own module, that it is the sole owner
  # of a surface (Wave-1 wires the mkMerge/last-wins guard). Here we assert the
  # registry is well-formed: every declared surface names a non-empty owner.
  surfaceOwnership =
    surfaces:
    let
      names = builtins.attrNames surfaces;
      badOwner = builtins.filter (n: (surfaces.${n}.owner or "") == "") names;
    in
    assertCheck "surface-ownership" (
      badOwner == [ ]
    ) "surfaces with no owner: ${builtins.toString badOwner}";

  # ── Check 2: rice RUNTIME state stays out of the source tree ──────────────
  # `stage/`, `auditions/` and `declared/` are live state — what was last staged,
  # what was auditioned, what was declared. They live under `$STRILLIX_ROOT` at
  # run time, never in the checkout, and `.gitignore` covers all three. The
  # legacy `song/` spelling of the same dirs is named too, so a checkout that
  # still carries the pre-rename tree fails here. Pure flake eval reads only TRACKED files, so an untracked
  # runtime dir cannot reach a build; this check is what catches one that was
  # committed anyway, or left behind in a store copy that was taken from a dirty
  # tree. (The old `no-song-read` scanned module PATHS for those names and could
  # never fire: a module never reads through its own path.)
  riceRuntimeUntracked =
    src:
    let
      runtimeDirs = [
        "stage"
        "auditions"
        "declared"
      ];
      present = builtins.filter (
        d: builtins.pathExists (src + "/${d}") || builtins.pathExists (src + "/song/${d}")
      ) runtimeDirs;
    in
    assertCheck "rice-runtime-untracked" (
      present == [ ]
    ) "the source tree holds gitignored rice runtime state: ${builtins.toString present}";

  # ── Check 3: rice shape (CONTRACTS.md §5, rice shape v1) ───────────────────
  # A committed rice under rices/<name>/ is a folder of notes and one
  # body. Four things are checked, each because a failure is silent elsewhere:
  #
  #   - `rice.nix` is an attrset of `system` and/or `home` halves, and a half
  #     takes only `members`, `providers` and `module` (habit's readBody refuses
  #     anything else, but only for a SELECTED rice; this runs for every rice);
  #   - `theme.json` and `hyprland.conf` exist. `hyprland.conf` may be empty,
  #     and the staging engine links to a staging-owned empty file when a rice
  #     ships none, so a rice with no binds never keeps the previous one's;
  #   - `hyprland.conf` carries no key the THEME owns (gaps, rounding, borders,
  #     blur, colours). The theme renders those into the same rice, so a second
  #     writer would race it: the rice file keeps binds, layout and animations;
  #   - every value a rice module sets, other than `rice.hyprland.*`, is under
  #     lib.mkDefault, and no module writes wayland.windowManager.hyprland.settings
  #     (`moduleProblemList`, below);
  #   - no stray `.nix` joins the module merge, and no `.nix` names a path out of
  #     its own folder (`strayNixFiles`, `escapingNixFiles`).
  #   - a declared `strillix.theme.wallpaper` is the rice's own `wallpaper/<rice>-desktop.<ext>`,
  #     the one name the activation seed and `rice stage` both resolve.
  #
  # The body, the hyprland.conf and the rice modules are the reads here that look
  # inside a file. A module is CALLED, and its values are walked for their
  # priority only: attrset shape and `_type` tags are forced, a leaf value is not.
  riceShape =
    {
      ricesRoot,
      discoveredNames,
      strayNixFiles,
      escapingNixFiles,
      borrow,
    }:
    let
      riceLib = import ./rice.nix { inherit lib; };
      bodyKeys = [
        "description"
        "system"
        "home"
      ];
      halfKeys = [
        "members"
        "providers"
        "module"
      ];
      bodyProblem =
        name:
        let
          body = import (ricesRoot + "/${name}/rice.nix");
          halves = lib.filter (h: body ? ${h}) [
            "system"
            "home"
          ];
        in
        if !(builtins.isAttrs body) then
          "not an attrset"
        else if lib.subtractLists bodyKeys (builtins.attrNames body) != [ ] then
          "unknown field(s) ${builtins.toString (lib.subtractLists bodyKeys (builtins.attrNames body))}"
        else if halves == [ ] then
          "neither a system nor a home half"
        else
          lib.findFirst (p: p != null) null (
            map (
              h:
              if !(builtins.isAttrs body.${h}) then
                "`${h}` is not an attrset"
              else if lib.subtractLists halfKeys (builtins.attrNames body.${h}) != [ ] then
                "`${h}` has unknown field(s) ${
                  builtins.toString (lib.subtractLists halfKeys (builtins.attrNames body.${h}))
                }"
              else
                null
            ) halves
          );

      # Keys the theme owns, as hyprlang writes them: `key = …` beside any
      # `general:`/`decoration:` prefix or inside a block, and the block heads
      # themselves. A comment line never counts.
      themeKey = "(gaps_in|gaps_out|gaps_workspaces|rounding|border_size|col\\.[A-Za-z_0-9]+|blur:[A-Za-z_]+)[[:space:]]*=.*";
      themeBlock = "(general|decoration|blur)[[:space:]]*\\{.*";
      themeLine =
        line:
        builtins.match "[[:space:]]*#.*" line == null
        && (
          builtins.match "(.*[[:space:]{;:])?${themeKey}" line != null
          || builtins.match "(.*[[:space:]{;:])?${themeBlock}" line != null
        );
      themeLeaks =
        name:
        let
          file = ricesRoot + "/${name}/hyprland.conf";
        in
        lib.filter themeLine (lib.splitString "\n" (builtins.readFile file));

      incomplete = builtins.filter (
        name:
        !(builtins.pathExists (ricesRoot + "/${name}/rice.nix"))
        || !(builtins.pathExists (ricesRoot + "/${name}/theme.json"))
        || !(builtins.pathExists (ricesRoot + "/${name}/hyprland.conf"))
      ) discoveredNames;
      badBodies = builtins.filter (
        name: builtins.pathExists (ricesRoot + "/${name}/rice.nix") && bodyProblem name != null
      ) discoveredNames;
      leaking = builtins.filter (
        name: builtins.pathExists (ricesRoot + "/${name}/hyprland.conf") && themeLeaks name != [ ]
      ) discoveredNames;

      # ── A rice module sets its values under lib.mkDefault ─────────────────────
      # CONTRACTS.md §5, "The body". A rice is a default for its slots and its
      # paint: a host's plain value outranks it, and a rice's plain value would
      # outrank the host's mkDefault. So every value a rice module sets, other than
      # `rice.hyprland.*` (the rice's own options, sourced after the base), is
      # priority 1000, and `wayland.windowManager.hyprland.settings` is refused
      # outright: the rice's hyprland keys go through `rice.hyprland`, where the
      # theme and the host can see them.
      #
      # The walk reads each value's shape and its priority from the override
      # wrapper it carries. A list or a string is never forced past its shape, and
      # an attrset is opened only to find its own leaves, so a `pkgs` attribute
      # inside a list costs nothing even under the fixture's stub package set.
      moduleOffer = {
        inherit lib pkgs;
        config = { };
        options = { };
        modulesPath = "";
        rice = riceLib;
        inherit borrow;
      };
      tagged = v: builtins.isAttrs v && builtins.hasAttr "_type" v;
      leavesOf =
        prio: path: v:
        if tagged v && v._type == "override" then
          leavesOf v.priority path v.content
        else if tagged v && v._type == "merge" then
          lib.concatMap (leavesOf prio path) v.contents
        else if
          builtins.isAttrs v && !(tagged v) && !(builtins.hasAttr "type" v && v.type == "derivation")
        then
          lib.concatLists (lib.mapAttrsToList (k: x: leavesOf prio (path ++ [ k ]) x) v)
        else
          [
            {
              inherit path prio;
            }
          ];
      rawOf =
        module:
        if builtins.isFunction module then
          module (lib.intersectAttrs (builtins.functionArgs module) moduleOffer)
        else
          module;
      unwrapOverride = v: if tagged v && v._type == "override" then unwrapOverride v.content else v;
      # A rice's picture is its own `wallpaper/<rice>-desktop.<ext>` (CONTRACTS.md §4):
      # the seed publishes the declared path, and `rice stage` finds the same
      # file by that name, so a declared picture under any other name would
      # show after a rebuild and vanish on a stage.
      wallpaperProblems =
        name: half: module:
        let
          raw = rawOf module;
          picked = unwrapOverride (
            lib.attrByPath [ "strillix" "theme" "wallpaper" ] null (unwrapOverride (raw.config or raw))
          );
          path = if picked == null then null else toString picked;
        in
        if path == null then
          [ ]
        else if
          baseNameOf (dirOf path) == "wallpaper"
          && builtins.match "${name}-desktop\\.(webp|png|jpg|jpeg)" (baseNameOf path) != null
        then
          [ ]
        else
          [
            "rice ${name}'s ${half}.module sets strillix.theme.wallpaper to ${path}; the picture must be wallpaper/${name}-desktop.<ext> (CONTRACTS.md §4)"
          ];
      moduleProblems =
        name: half: module:
        let
          raw = rawOf module;
          where = "rice ${name}'s ${half}.module";
          extraKeys =
            if raw ? config then
              lib.subtractLists [ "config" ] (builtins.attrNames raw)
            else
              lib.intersectLists [ "imports" "options" ] (builtins.attrNames raw);
          leaves = leavesOf 100 [ ] (raw.config or raw);
          shown = path: lib.concatStringsSep "." path;
          baseHyprland = lib.filter (
            l:
            lib.take 3 l.path == [
              "wayland"
              "windowManager"
              "hyprland"
            ]
          ) leaves;
          unwrapped = lib.filter (l: l.prio != 1000 && lib.head l.path != "rice") leaves;
        in
        if extraKeys != [ ] then
          [ "${where} declares ${lib.concatStringsSep ", " extraKeys}; a rice module only sets values" ]
        else if baseHyprland != [ ] then
          [
            "${where} writes ${shown (lib.take 4 (lib.head baseHyprland).path)}; a rice's hyprland keys are rice.hyprland.* (CONTRACTS.md §5)"
          ]
        else if unwrapped != [ ] then
          [
            "${where} sets ${shown (lib.head unwrapped).path} outside lib.mkDefault; wrap the module's values in `config = lib.mkDefault { … };` (rice.hyprland.* excepted)"
          ]
        else
          [ ];
      moduleProblemList = lib.concatMap (
        name:
        if !(builtins.elem name badBodies) && builtins.pathExists (ricesRoot + "/${name}/rice.nix") then
          let
            body = import (ricesRoot + "/${name}/rice.nix");
          in
          lib.concatMap
            (
              half:
              lib.optionals (body ? ${half} && body.${half} ? module) (
                moduleProblems name half body.${half}.module ++ wallpaperProblems name half body.${half}.module
              )
            )
            [
              "system"
              "home"
            ]
        else
          [ ]
      ) discoveredNames;
    in
    assertCheck "rice-shape"
      (
        strayNixFiles == [ ]
        && incomplete == [ ]
        && escapingNixFiles == [ ]
        && badBodies == [ ]
        && leaking == [ ]
        && moduleProblemList == [ ]
      )
      "rices shape is wrong — ${
        if strayNixFiles != [ ] then
          "stray .nix outside rice.nix/_widgets/: ${builtins.toString strayNixFiles}"
        else if escapingNixFiles != [ ] then
          "a rice's `.nix` TEXT carries a `../` path literal (CONTRACTS.md §5): ${builtins.toString escapingNixFiles}"
        else if badBodies != [ ] then
          "rice.nix is not a body (system/home halves, CONTRACTS.md §5) in: ${builtins.toString badBodies}: ${bodyProblem (builtins.head badBodies)}"
        else if leaking != [ ] then
          "hyprland.conf sets a theme-owned key (gaps, rounding, borders, blur, colours) in: ${builtins.toString leaking}"
        else if moduleProblemList != [ ] then
          builtins.head moduleProblemList
        else
          "no rice.nix, theme.json or hyprland.conf in: ${builtins.toString incomplete}"
      }";

  # ── Check 4: nixfmt --check over the committed source ──────────────────────
  # `src` is the flake's own store copy (flake.nix passes `self`), which nix
  # already git-filters — this scopes the check to the COMMITTED tree, never
  # the working tree. Uncommitted drift is `aoide soundcheck`'s job, not
  # this one's; the two are disjoint by construction (`nix flake check`
  # cannot see gitignored/uncommitted files, full stop). A real `runCommand`
  # because it has to run the `nixfmt` binary — no purely-eval way to check
  # formatting.
  fmt =
    src:
    pkgs.runCommand "aoide-check-fmt" { nativeBuildInputs = [ pkgs.nixfmt ]; } ''
      set -euo pipefail
      cd ${src}
      nixfmt --check $(find . -name '*.nix' -type f)
      touch "$out"
    '';

  # ── Check 5: package discovery completeness ─────────────────────────────────
  # `strays` is lib/pkgs.nix's own `strayEntries` — every `pkgs/<name>/`
  # directory that is neither shelved (`_`-prefixed), a callPackage target
  # (carries default.nix), nor self-flaked (carries flake.nix, e.g.
  # pkgs/aoide). The predicate lives in lib/pkgs.nix, next to the discovery
  # rule it enforces; this check only asserts the list it returns is empty —
  # it invents no second copy of the rule.
  discovery =
    strays:
    assertCheck "discovery" (
      strays == [ ]
    ) "pkgs/ entries neither a package, shelved, nor self-flaked: ${builtins.toString strays}";

  # ── Check 6: no phantom commands in the agent docs ──────────────────────────
  # The docs — root AGENTS.md, docs/agent/*.md, docs/INSTALL.md and the wiki
  # (docs/Aoide-Wiki/**, minus the ledgers/history/roadmap paths the file
  # list below names and explains) — are the one place the command surface is
  # still taught in hand-written prose; both binaries' guide texts are
  # registry-derived, so they cannot drift and are not re-checked here. Every
  # backticked invocation spelling those docs teach
  # (`aoide send …`, `strx rice compose <name>`, …) must resolve against
  # the matching binary's `schema --json`, built from this checked-out source:
  # the spelled words must be a registered command path, or a prefix of one
  # (`aoide send` validly teaches the `send` leaf; `aoide secrets`
  # alone validly names a group). A spelling that resolves to nothing fails
  # the build naming the file, line, and spelling.
  #
  # Extraction is precision-over-recall: only inline code spans STARTING with
  # `aoide `/`strx ` followed by a lowercase word count as teachings (a span
  # may wrap across a line break); the path walk stops at the first non-path
  # token (`<args>`, `[flags]`, `--flags`, `--`, ellipses), and both fenced
  # delimiter lines and comment lines inside fences are blanked before pairing
  # (a fence's own backticks are not span edges — left in, they shifted every
  # span after them and hid real teachings). `src` is the flake's own store
  # copy, so like Check 4 this scopes to the COMMITTED tree; `aoidePkg` is
  # the self-flaked core package — `aoide` from its default output, `strillix`
  # from its `strillix` output.
  phantomCommands =
    src: aoidePkg:
    let
      extract = pkgs.writeText "phantom-commands-extract.pl" ''
        use strict;
        use warnings;

        my $file = shift or die "usage: extract.pl FILE\n";
        open my $fh, '<', $file or die "$file: $!\n";
        my $text = do { local $/; <$fh> };
        close $fh;

        # Fenced code blocks. Both halves matter for span pairing:
        #  - a DELIMITER line (````json`) is blanked wholesale, because its
        #    own three backticks are not a span edge — left in, they pair
        #    with the next backtick anywhere in the file and shift every
        #    span after them, hiding real teachings from this check;
        #  - comment lines INSIDE a fence are blanked too: fenced
        #    commentary may name a spelling without teaching it.
        # Offsets are preserved either way, so line numbers stay true.
        my @lines = split /\n/, $text, -1;
        my $fence = 0;
        for my $i (0 .. $#lines) {
          my $l = $lines[$i];
          if ($l =~ /^\s*(```|~~~)/) {
            $lines[$i] = ' ' x length($l);
            $fence = !$fence;
            next;
          }
          $lines[$i] = ' ' x length($l) if $fence && $l =~ /^\s*#/;
        }
        $text = join "\n", @lines;

        # Inline code spans; a span may wrap across a line break.
        while ($text =~ /`([^`]+)`/g) {
          my $span = $1;
          my $at   = pos($text) - length($span) - 2;
          my $line = 1 + (substr($text, 0, $at) =~ tr/\n//);
          $span =~ s/\s+/ /g;
          # Only spans starting with the binary name and a lowercase command
          # word are invocation teachings; `aoide <cmd>` and prose are not.
          next unless $span =~ /^(aoide|strx) [a-z]/;
          my ($bin, @toks) = split ' ', $span;
          my @words;
          for my $t (@toks) {
            # Stop at the first non-path token: <args>, [flags], --flags,
            # `--`, ellipses — everything after is invocation tail, not path.
            last unless $t =~ /^[a-z][a-z-]*$/;
            push @words, $t;
          }
          next unless @words;
          print join("\t", $file, $line, $bin, "@words"), "\n";
        }
      '';
    in
    pkgs.runCommand "aoide-check-phantom-commands"
      {
        nativeBuildInputs = [
          pkgs.jq
          pkgs.perl
        ];
      }
      ''
        set -euo pipefail
        export HOME="$TMPDIR"

        # The registered command paths, one per line, straight from each
        # binary — the same source of truth `schema --json` contracts.
        ${aoidePkg}/bin/aoide schema --json \
          | jq -r '.commands[].path | join(" ")' > "$TMPDIR/aoide-paths"
        ${aoidePkg.strillix}/bin/strillix schema --json \
          | jq -r '.commands[].path | join(" ")' > "$TMPDIR/strillix-paths"

        cd ${src}
        # The doc set: the agent docs (root AGENTS.md + docs/agent/*.md) plus
        # INSTALL.md and the whole wiki — every hand-written place the command
        # surface is still taught in prose.
        #
        # Excluded BY PATH, never by weakening the extraction above:
        #   - docs/Aoide-Wiki/ingest/      the ingest log + index (a ledger,
        #                                  append-only: it records what was
        #                                  true then, spellings included)
        #   - docs/Aoide-Wiki/references/  dated handoffs, audit reports and
        #                                  design notes — history, same reason
        #   - docs/Aoide-Wiki/concepts/desktop/Feature-Set.md
        #                                  the Strillix roadmap: its spans name
        #                                  commands that do not exist yet and
        #                                  every one is labelled "planned …
        #                                  not in `aoide schema --json`", so
        #                                  the matcher would read intent as a
        #                                  teaching
        #   - docs/Aoide-Wiki/concepts/desktop/Controls.md
        #                                  the keybind table: three of its rows
        #                                  report a DEAD CLI form as the point
        #                                  ("no `shell` group exists in either
        #                                  registry", "a retired command, so
        #                                  the keybind is a no-op") — the dead
        #                                  spelling is the content there
        find AGENTS.md docs/agent docs/INSTALL.md docs/Aoide-Wiki -name '*.md' \
          -not -path 'docs/Aoide-Wiki/ingest/*' \
          -not -path 'docs/Aoide-Wiki/references/*' \
          -not -path 'docs/Aoide-Wiki/concepts/desktop/Feature-Set.md' \
          -not -path 'docs/Aoide-Wiki/concepts/desktop/Controls.md' \
          | sort > "$TMPDIR/docs"
        while IFS= read -r doc; do
          perl ${extract} "$doc"
        done < "$TMPDIR/docs" > "$TMPDIR/candidates"

        status=0
        while IFS="$(printf '\t')" read -r file line bin words; do
          [ "$bin" = strx ] && bin=strillix
          # Valid iff the spelled words are a registered path or a prefix of
          # one (word-boundary; the path lists are [a-z- ] so regex-safe).
          if ! grep -Eq "^$words( |$)" "$TMPDIR/$bin-paths"; then
            printf 'phantom command: %s:%s teaches `%s %s`, which resolves to no registered command path of %s\n' \
              "$file" "$line" "$bin" "$words" "$bin" >&2
            status=1
          fi
        done < "$TMPDIR/candidates"
        [ "$status" -eq 0 ]

        printf 'aoide check phantom-commands: ok (%d spellings checked)\n' \
          "$(wc -l < "$TMPDIR/candidates")" > "$out"
      '';
  # ── Check 7: core is nix-independent ────────────────────────────────────
  # AGENTS.md's own claim — core is cargo-buildable, no nix shell-outs, no
  # NixOS assumption; only `strillix` (and the deployment modules) may depend on
  # nix (P-A5 of the binary-split workstream; cli/Cargo.toml:36-43 documents
  # the boundary this check makes real). The closure is DERIVED, never
  # hardcoded. `pkgs/aoide/Cargo.toml`'s `[workspace.dependencies]` maps each
  # path crate's name to its `crates/<dir>`; a walk from `crates/cli` follows
  # each `[dependencies]` and `[build-dependencies]` table, plain or under
  # `[target.<cfg>.]`, through every entry that names a mapped crate.
  # `[dev-dependencies]` is never walked (test-support never ships, same
  # reasoning `aoide-cli`'s own manifest states for `aoide-test-support`).
  # The manifests are read by regex, not a TOML parser, so where a form is not
  # one it can read, the build fails naming the line or table instead of
  # walking past it: a path entry in a walked dependency table, before a walked
  # crate's first header, or in the root outside the single-line
  # `{ path = "crates/<dir>" }` map form; a dependency written as a root key
  # (`dependencies.<name>`, `target = { ... }`) rather than under a header; a
  # dotted table (`[dependencies.<name>]` and its build and target forms, or
  # `[workspace.dependencies.<name>]`); a renamed mapped crate; and a mapped
  # name not written as the single-line `{ workspace = true }`.
  #
  # Two assertions. First, the closure must not reach `crates/strillix-rice`,
  # `crates/screen` (package `aoide-screen`) or `crates/strillix`; the walk
  # keys crates by directory, so the message names each one with the closure
  # path that reached it. Second, no closure crate's `src/` shells out to nix:
  # a quoted `"nix"` / `"nix-<word>"` literal, or a quoted string naming
  # `nixos-rebuild` or `nix eval|build|flake|develop|run|shell|store|copy`, on
  # a line that also spawns a process (`Command::new(`/`.arg(`/`.args(`) —
  # gated on the process-spawn call so a `--help` summary that merely mentions
  # `nix flake check`, or a test fixture mocking a `"sudo nixos-rebuild
  # switch"` activity label, is prose/data rather than a violation.
  nixIndependence =
    src:
    let
      script = pkgs.writeText "nix-independence.pl" ''
        use strict;
        use warnings;

        my $root = "crates";
        my $ws_toml = "Cargo.toml";

        sub read_file {
            my ($path) = @_;
            open my $fh, '<', $path or die "cannot open $path: $!\n";
            local $/;
            my $t = <$fh>;
            close $fh;
            return $t;
        }

        # Every table of a manifest as [name, [body lines]], in file order.
        # The first entry has the empty name and holds the lines before the
        # first header. A header is a line holding only [name] or [[name]] and
        # an optional comment; its name loses blanks and quotes, so
        # `[ dependencies ]` is `dependencies`. A body runs to the next header.
        sub tables {
            my ($text) = @_;
            my @out = (["", []]);
            for my $line (split /\n/, $text, -1) {
                if ($line =~ /^\s*\[\[?([^\[\]]+)\]\]?\s*(?:#.*)?$/) {
                    (my $name = $1) =~ s/[\s"']+//g;
                    push @out, [ $name, [] ];
                } else {
                    push @{ $out[-1][1] }, $line;
                }
            }
            return @out;
        }

        # The lines of a body that can hold an entry: not blank, and not a
        # full-line TOML comment (the manifests carry prose comments naming
        # these crates).
        sub entries {
            my ($body) = @_;
            return grep { /\S/ && !/^\s*#/ } @$body;
        }

        # [dependencies] and [build-dependencies], plain or under
        # [target.<cfg>.]. The dev tables are never read: test-support never
        # ships.
        sub is_dep_table { return $_[0] =~ /^(?:target\..+\.)?(?:build-)?dependencies$/ }

        # A dependency written as its own table: [dependencies.<name>] and its
        # forms. Its entry is not a line of a table this check reads: it fails.
        sub is_dotted_dep { return $_[0] =~ /^(?:target\..+\.)?(?:build-)?dependencies\..+$/ }

        # A path entry, whatever the key's spelling: `path =`, `"path" =`,
        # `x.path =`. A key that only ends in -path (`json-path = "0.7"`) is a
        # crate name.
        sub is_path_entry { return $_[0] =~ /(?<![\w-])path["']?\s*=/ }

        # A dependency written as a root key, outside every table header:
        # `dependencies.<n>`, `dependencies = { ... }`, `target = { ... }` and
        # the build forms. Cargo reads each as the table it spells, so it is
        # refused rather than skipped.
        sub is_root_dep_key {
            my ($line) = @_;
            my ($key) = $line =~ /^\s*([^=]+?)\s*=/ or return 0;
            (my $n = $key) =~ s/[\s"']+//g;
            return $n =~ /^(?:target|dependencies|build-dependencies)(?:\.|$)/ ? 1 : 0;
        }

        # ── 1. the map: name -> crates/<dir>, from the root's path entries ──
        # Each is one line, `{ path = "crates/<dir>" }`, under
        # [workspace.dependencies].
        my %dir_of;
        my $ws_seen = 0;
        my @ws_bad;
        for my $t (tables(read_file($ws_toml))) {
            my ($name, $body) = @$t;
            die "aoide check nix-independence FAILED: $ws_toml has the table [$name], which this "
              . "check cannot read (write the entry under [workspace.dependencies])\n"
              if $name =~ /^workspace\.dependencies\..+$/;
            $ws_seen = 1 if $name eq "workspace.dependencies";
            for my $line (entries($body)) {
                next unless is_path_entry($line);
                if ($name eq "workspace.dependencies"
                    && $line =~ /^\s*([\w-]+)\s*=\s*\{\s*path\s*=\s*"crates\/([\w-]+)"\s*\}\s*$/) {
                    $dir_of{$1} = $2;
                } else {
                    push @ws_bad, $line;
                }
            }
        }
        die "aoide check nix-independence FAILED: $ws_toml has no [workspace.dependencies] table\n"
          unless $ws_seen;
        die "aoide check nix-independence FAILED: $ws_toml has a path entry this check cannot parse "
          . "as a single-line { path = \"crates/<dir>\" } under [workspace.dependencies]:\n"
          . join("\n", @ws_bad) . "\n"
          if @ws_bad;

        # ── 2. the closure: from crates/cli, walk each dependency table
        # of the kinds above, following every entry that names a mapped
        # crate; a path entry or a dotted table fails ──
        my %visited = (cli => 1);
        my %parent;
        my @queue = ("cli");

        while (@queue) {
            my $dir = shift @queue;
            my $toml = "$root/$dir/Cargo.toml";
            die "aoide check nix-independence FAILED: no $toml (closure walk reached "
              . "an unresolved crate)\n" unless -f $toml;
            for my $t (tables(read_file($toml))) {
                my ($name, $body) = @$t;
                die "aoide check nix-independence FAILED: $toml has the table [$name], which this "
                  . "check cannot read (write each dependency as a line of [dependencies])\n"
                  if is_dotted_dep($name);
                if ($name eq "") {
                    my @bad = grep { is_path_entry($_) || is_root_dep_key($_) } entries($body);
                    die "aoide check nix-independence FAILED: $toml has a dependency or path entry "
                      . "outside every table (before its first header):\n" . join("\n", @bad) . "\n"
                      if @bad;
                }
                next unless is_dep_table($name);
                for my $line (entries($body)) {
                    die "aoide check nix-independence FAILED: $toml [$name] has a path dependency "
                      . "the workspace.dependencies map cannot see:\n$line\n"
                      if is_path_entry($line);
                    die "aoide check nix-independence FAILED: $toml [$name] renames a mapped crate, "
                      . "which this check cannot follow:\n$line\n"
                      if $line =~ /(?<![\w-])package["']?\s*=\s*["']([\w-]+)["']/ && exists $dir_of{$1};
                    my ($q, $key) = $line =~ /^\s*(["']?)([\w-]+)\1\s*[.=]/ or next;
                    next unless exists $dir_of{$key};
                    die "aoide check nix-independence FAILED: $toml [$name] has an entry this check "
                      . "cannot parse as a single-line { workspace = true }:\n$line\n"
                      unless $line =~ /^\s*\Q$q$key$q\E\s*=\s*\{\s*workspace\s*=\s*true\s*\}\s*$/;
                    my $child = $dir_of{$key};
                    unless ($visited{$child}) {
                        $visited{$child} = 1;
                        $parent{$child} = $dir;
                        push @queue, $child;
                    }
                }
            }
        }

        sub closure_path {
            my ($node) = @_;
            my @chain = ($node);
            while (exists $parent{$chain[0]}) {
                unshift @chain, $parent{$chain[0]};
            }
            return join(" -> ", @chain);
        }

        # ── Assertion 1: the P-A5 boundary, by crates/ directory ──────
        my @forbidden = grep { $visited{$_} } ("strillix-rice", "screen", "strillix");
        if (@forbidden) {
            die "aoide check nix-independence FAILED: core closure reaches "
              . join(", ", map { "crates/$_ (" . closure_path($_) . ")" } @forbidden)
              . " — the P-A5 split is broken\n";
        }

        # ── Assertion 2: no nix shell-out in any closure crate's src/ ───────
        # A quoted nix-flavored literal only counts on a line that also spawns
        # a process (`Command::new(`/`.arg(`/`.args(`) — otherwise it is prose
        # (a `--help` summary describing this very check) or fixture data (a
        # mocked "sudo nixos-rebuild switch" activity label in a test), never
        # an invocation.
        my @nix_subs = qw(eval build flake develop run shell store copy);
        my $sub_alt = join("|", @nix_subs);

        sub find_rs_files {
            my ($dir) = @_;
            my @out;
            return @out unless -d $dir;
            opendir(my $dh, $dir) or die "cannot opendir $dir: $!\n";
            for my $entry (readdir $dh) {
                next if $entry eq "." || $entry eq "..";
                my $path = "$dir/$entry";
                if (-d $path) {
                    push @out, find_rs_files($path);
                } elsif ($entry =~ /\.rs$/) {
                    push @out, $path;
                }
            }
            closedir $dh;
            return @out;
        }

        my @violations;
        for my $dir (sort keys %visited) {
            my @files = sort(find_rs_files("$root/$dir/src"));
            for my $file (@files) {
                my $text = read_file($file);
                my @lines = split /\n/, $text, -1;
                for my $i (0 .. $#lines) {
                    my $line = $lines[$i];
                    next if $line =~ /^\s*(\/\/|\*)/;
                    next unless $line =~ /\b(?:Command::new|\.arg|\.args)\s*\(/;
                    if ($line =~ /"nix(-\w+)?"/
                        || $line =~ /"[^"]*nixos-rebuild[^"]*"/
                        || $line =~ /"[^"]*nix (?:$sub_alt)[^"]*"/) {
                        my $lineno = $i + 1;
                        push @violations,
                          sprintf("%s:%d: %s  (closure: %s)", $file, $lineno, $line, closure_path($dir));
                    }
                }
            }
        }
        if (@violations) {
            die "aoide check nix-independence FAILED: nix shell-out pattern found in "
              . "the core closure's src/:\n" . join("\n", @violations) . "\n";
        }

        printf "aoide check nix-independence: ok (%d crates: %s)\n",
          scalar(keys %visited), join(", ", sort keys %visited);
      '';
    in
    pkgs.runCommand "aoide-check-nix-independence" { nativeBuildInputs = [ pkgs.perl ]; } ''
      set -euo pipefail
      cd ${src}/pkgs/aoide
      perl ${script} > "$out"
    '';

  # ── Check 8: static artifact is genuinely portable ──────────────────────
  # Thin wiring only — the assertions live in tests/portability.nix (see that
  # file's header, and this file's own header above).
  portability = pkg: (import ../tests/portability.nix { inherit lib pkgs; }) pkg;

  # ── Check 9: nix lint ───────────────────────────────────────────────────
  # `statix check` over the same COMMITTED tree Check 4 formats — style's
  # other half, and the half nothing ran. statix shipped in the devShell
  # from the start, so it only fired when someone remembered to type it; as
  # a check it fires for every agent, in the place `nix flake check` already
  # reports formatting. `errfmt` output is `file>line:col:W:code:msg`, one
  # finding per line, so a failure reads as a fix list rather than prose.
  #
  # `statix.toml` sits at the repo root and is discovered from the cwd, so it
  # rides along in `src` and the house style stays exempt: `repeated_keys`
  # (W20) would otherwise flag every flat option assignment the style writes
  # deliberately (modules/AGENTS.md, "Flat option assignment").
  nixLint =
    src:
    pkgs.runCommand "aoide-check-nix-lint" { nativeBuildInputs = [ pkgs.statix ]; } ''
      set -euo pipefail
      cd ${src}
      statix check . -o errfmt
      touch "$out"
    '';

  # ── Check 10: the two fan-outs agree under a venue override ────────────────
  # `lib/theme.nix`'s `resolve` (the baked Stylix/compositor fan-out) and
  # `stagePatch` (the `$STRILLIX_ROOT/stage/theme.json` seed) must apply the identical
  # override rule (CONTRACTS.md §1). Proved here against a FIXED fixture —
  # never a real host's `strillix.theme` or a committed rices file, so this
  # check never depends on which rices or hosts are in the tree. One case (a
  # single overridden anchor recolours a matching palette slot) plus the
  # no-override identity `stagePatch` promises callers. `venueDelta` (the
  # `$STRILLIX_ROOT/declared/venue.json` the runtime stage overlays) is proved on the
  # same fixture: the one recoloured slot and nothing else, `{ }` without an
  # override. Geometry is the third case: a value the host sets, laid over the
  # document whether or not it holds the tier, and in `venueDelta` only where
  # it differs from the document's own.
  themeFanout =
    let
      theme = import ./theme.nix { inherit lib; };
      fixturePalette = {
        bg = "#111111";
        fg = "#eeeeee";
        accent = "#222222";
        urgent = "#333333";
        hot = null;
      };
      fixtureTheme = {
        palette = fixturePalette;
        base16 = null;
        bar = {
          bg = null;
          fg = null;
          accent = null;
        };
        notif = {
          bg = null;
          fg = null;
          urgent = null;
        };
        window = {
          border = null;
          borderInactive = null;
        };
        override = {
          bg = "#ff0000";
        };
      };
      fixtureBare = fixtureTheme // {
        override = { };
      };
      # A two-key document, mirroring the committed theme.json shape
      # (schemaVersion + palette) without reading one.
      fixtureDoc = {
        schemaVersion = "0";
        palette = fixturePalette;
      };
      # A host that sets one geometry field (null fields are no opinion), against
      # a document with no geometry and against one whose own geometry it
      # partly shares.
      fixtureGeo = fixtureBare // {
        geometry = {
          terminalOpacity = 1.0;
          gapsOut = null;
        };
      };
      fixtureDocGeo = fixtureDoc // {
        geometry = {
          terminalOpacity = 0.7;
          gapsIn = 4;
        };
      };
      resolvedPalette = (theme.resolve fixtureTheme).palette;
      stagedPalette = (theme.stagePatch fixtureTheme fixtureDoc).palette;
      identity = theme.stagePatch fixtureBare fixtureDoc == fixtureDoc;
      deltaOne =
        theme.venueDelta fixtureTheme fixtureDoc == {
          palette = {
            bg = "#ff0000";
          };
        };
      deltaBare = theme.venueDelta fixtureBare fixtureDoc == { };
      geoCreated =
        (theme.stagePatch fixtureGeo fixtureDoc).geometry or null == {
          terminalOpacity = 1.0;
        }
        &&
          theme.venueDelta fixtureGeo fixtureDoc == {
            geometry = {
              terminalOpacity = 1.0;
            };
          };
      geoMerged =
        (theme.stagePatch fixtureGeo fixtureDocGeo).geometry or null == {
          terminalOpacity = 1.0;
          gapsIn = 4;
        }
        &&
          theme.venueDelta fixtureGeo fixtureDocGeo == {
            geometry = {
              terminalOpacity = 1.0;
            };
          };
      geoEqual =
        let
          same = fixtureBare // {
            geometry = {
              terminalOpacity = 0.7;
            };
          };
        in
        theme.stagePatch same fixtureDocGeo == fixtureDocGeo && theme.venueDelta same fixtureDocGeo == { };
    in
    assertCheck "theme-fanout"
      (
        stagedPalette == resolvedPalette
        && identity
        && deltaOne
        && deltaBare
        && geoCreated
        && geoMerged
        && geoEqual
      )
      "stagePatch and resolve disagree on the override recolour, stagePatch is not the identity with no override set, venueDelta is not exactly the recoloured slots, or a host-set geometry field is not laid over the document (created when absent, merged when present, no delta when equal)";
  # ── Checks 11 and 12: the shipped rices generator is offline ───────────
  # §9's claim, as a build: `share/strillix/nix/manifest.nix` (shipped by
  # `pkgs/strillix-rices`) is the runtime's generator, and a plain
  # `nix-instantiate --eval --strict --json --expr` on it — the exact argv
  # `strillix-rice::widgets` runs, with `NIX_PATH=` empty, no flake, no network, no
  # checkout — must answer what the flake's own `ricesManifest` answers.
  #
  # Check 11 (`generator-offline`): over the repo's committed rices.
  # Check 12 (`generator-relocatable`): over a COPY of it in `$TMPDIR`, outside
  # the repo — §7.4's constraints 1 and 5 (a rice folder is self-contained and
  # holds no repo-absolute path), which is what lets a machine evaluate its OWN
  # rices.
  #
  # `expected` is `builtins.toJSON { inherit (ricesLib) manifestAttrs
  # registryAttrs; }` — the same value the `ricesManifest` flake output
  # exposes, so the comparison is against the flake, never a hand-kept golden.
  generatorCommands =
    { strillixRices, ricesArg }:
    ''
      export XDG_CACHE_HOME="$TMPDIR/cache" HOME="$TMPDIR"
      nix-instantiate --eval --strict --json --expr \
        "import ${strillixRices}/share/strillix/nix/manifest.nix { ricesRoot = \"${ricesArg}\"; }" \
        | jq -c 'del(.packages)'
    '';

  generatorOffline =
    {
      strillixRices,
      ricesRoot,
      expected,
    }:
    pkgs.runCommand "aoide-check-generator-offline"
      {
        nativeBuildInputs = [
          pkgs.nix
          pkgs.jq
          pkgs.diffutils
        ];
        expectedJson = pkgs.writeText "aoide-rices-manifest.json" expected;
      }
      ''
        got=$(${
          generatorCommands {
            inherit strillixRices;
            ricesArg = "${ricesRoot}";
          }
        })
        want=$(cat "$expectedJson")
        if [ "$got" != "$want" ]; then
          echo "the shipped generator's answer over ${ricesRoot} is not #ricesManifest:" >&2
          diff <(printf '%s' "$want" | jq -S .) <(printf '%s' "$got" | jq -S .) >&2 || true
          exit 1
        fi
        printf 'aoide check generator-offline: ok\n' > "$out"
      '';

  generatorRelocatable =
    {
      strillixRices,
      ricesRoot,
      expected,
    }:
    pkgs.runCommand "aoide-check-generator-relocatable"
      {
        nativeBuildInputs = [
          pkgs.nix
          pkgs.jq
          pkgs.diffutils
        ];
        ricesCopy = builtins.path {
          path = ricesRoot;
          name = "aoide-rices-copy";
        };
        expectedJson = pkgs.writeText "aoide-rices-manifest.json" expected;
      }
      ''
        export XDG_CACHE_HOME="$TMPDIR/cache" HOME="$TMPDIR"
        # A copy that shares no directory with any checkout — the shape a
        # machine's own `$STRILLIX_ROOT/rices` has.
        mkdir -p "$TMPDIR/relocated"
        cp -r "$ricesCopy"/. "$TMPDIR/relocated"/
        in_repo=$(${
          generatorCommands {
            inherit strillixRices;
            ricesArg = "${ricesRoot}";
          }
        })
        relocated=$(${
          generatorCommands {
            inherit strillixRices;
            ricesArg = "$TMPDIR/relocated";
          }
        })
        want=$(cat "$expectedJson")
        if [ "$in_repo" != "$want" ] || [ "$relocated" != "$want" ]; then
          echo "the shipped generator answers differently for a relocated rices:" >&2
          diff <(printf '%s' "$want" | jq -S .) <(printf '%s' "$relocated" | jq -S .) >&2 || true
          exit 1
        fi
        printf 'aoide check generator-relocatable: ok\n' > "$out"
      '';

  # ── Check 13: every bar embeds the shared rice-mode control ────────────────
  # The rice-mode control is the slot `ricemode`: a bar embeds it as
  # `WidgetSlot { slot: "ricemode" }`, and the slot resolves through the
  # baseline chain to the rice's own body or sonata's. A bar body without the
  # literal has no rice-mode control, so a gesture added to the slot never
  # reaches it; a rices without sonata's record has no floor to resolve to.
  #
  # `manifest` is `ricesLib.manifestAttrs`, which names each rice's bar by
  # its owner: a borrowed bar is read once, under its owner, and a rice with no
  # `bar` entry (nocturne) renders sonata's, which sonata's own entry covers. Only the committed rices is read, so a
  # rice that exists only in the runtime rices is judged when `rice declare`
  # copies it into the checkout.
  barRicemode =
    { ricesRoot, manifest }:
    let
      bars = lib.unique (
        lib.concatMap (m: lib.optional (m ? bar) "${m.bar.owner}/widgets/${m.bar.file}") (
          builtins.attrValues manifest
        )
      );
      # `builtins.split`, not `lib.hasInfix`: that one is a `builtins.match` over
      # `.*needle.*`, which overflows the stack on a body the size of cadenza's bar.
      embeds = text: builtins.length (builtins.split ''slot: "ricemode"'' text) > 1;
      omitting = builtins.filter (rel: !(embeds (builtins.readFile (ricesRoot + "/${rel}")))) bars;
      noFloor = !((manifest.sonata or { }) ? ricemode);
    in
    assertCheck "bar-ricemode" (omitting == [ ] && !noFloor) (
      lib.concatStringsSep "; " (
        lib.optional (
          omitting != [ ]
        ) ''bar bodies without `slot: "ricemode"`: ${lib.concatStringsSep ", " omitting}''
        ++ lib.optional noFloor "sonata has no `ricemode` slot, the floor every bar's slot resolves to"
      )
    );

  # ── Check 14: Aoide and Strillix cross only where the golden says ──────────
  # Thin wiring only — the scan, the sides it sorts every file into and the
  # golden it compares against live in tests/boundary/ (scan.pl's header and
  # README.md), the way Check 8's assertions live in tests/portability.nix.
  # `src` is the flake's own store copy, which nix already git-filters, so like
  # Check 4 this scopes to the COMMITTED tree: the files a build sees are the
  # files counted.
  boundary =
    src:
    pkgs.runCommand "aoide-check-boundary" { nativeBuildInputs = [ pkgs.perl ]; } ''
      set -euo pipefail
      perl ${src}/tests/boundary/scan.pl check ${src} ${src}/tests/boundary/crossings.golden > "$out"
    '';

  # ── Check 15: the built strillix stages a rice the way a host does ─────────
  # Checks 11 and 12 call the shipped generator themselves, so nothing else
  # exercises the call the runtime makes: `strillix rice stage` and `reload`
  # pass the generator its argument by name. This drives the BUILT `strillix`
  # as a host does. `rice refresh` seeds the machine's rices (the activation's
  # `aoideSeedRices`), one built-in rice's machine copy is edited, staging is
  # unlocked as `rice mode stage` unlocks it, and `rice stage` runs over the
  # real `nix-instantiate`. The run tree must then hold the edited file and both
  # generated JSON files. A full copy of that machine rice under a name the
  # shipped JSON lacks must reach both files too: the baked baseline cannot
  # supply that key, so only the generator's answer can. The best-effort steps
  # (hyprctl, kitty, the quickshell IPC, the wallpaper provider) have no binary
  # here and cannot fail a stage.
  generatorRuntime =
    {
      strillix,
      strillixRices,
      rice,
    }:
    pkgs.runCommand "aoide-check-generator-runtime"
      {
        nativeBuildInputs = [
          pkgs.nix
          pkgs.jq
        ];
      }
      ''
        export HOME="$TMPDIR/home" XDG_CACHE_HOME="$TMPDIR/cache"
        export XDG_STATE_HOME="$TMPDIR/state" XDG_RUNTIME_DIR="$TMPDIR/runtime"
        export AOIDE_ROOT="$TMPDIR/aoide" STRILLIX_ROOT="$TMPDIR/strillix"
        export STRILLIX_TEMPLATES="${strillixRices}/share/strillix/rices"
        mkdir -p "$HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME" "$XDG_RUNTIME_DIR"
        mkdir -p "$AOIDE_ROOT/run/qml"
        strillix="${strillix}/bin/strillix"
        marker='// generator-runtime: a machine copy the stage must carry'
        fail() {
          echo "aoide check generator-runtime FAILED: $1" >&2
          cat "$TMPDIR/stage.err" "$TMPDIR/stage.json" >&2 2>/dev/null || true
          exit 1
        }

        "$strillix" rice refresh || fail "rice refresh, the activation seed, failed"

        bar="$STRILLIX_ROOT/rices/${rice}/widgets/bar.qml"
        chmod u+w "$bar"
        printf '%s\n' "$marker" >> "$bar"

        "$strillix" rice mode stage || fail "rice mode stage, the unlock, failed"
        if ! "$strillix" rice stage "${rice}" --json >"$TMPDIR/stage.json" 2>"$TMPDIR/stage.err"; then
          fail "rice stage exited non-zero"
        fi
        if [ "$(jq -r .status "$TMPDIR/stage.json")" != ok ]; then
          fail "rice stage did not report status ok"
        fi
        for file in manifest.json registry.json; do
          if ! jq -e --arg rice "${rice}" 'has($rice)' "$AOIDE_ROOT/run/qml/rices/$file" >/dev/null; then
            fail "run/qml/rices/$file is not JSON with a ${rice} key"
          fi
        done
        staged="$AOIDE_ROOT/run/qml/rices/${rice}/bar.qml"
        if [ "$(tail -n 1 "$staged")" != "$marker" ]; then
          fail "the staged bar.qml does not end with the machine copy's marker"
        fi
        # A full copy of the machine rice under a name the shipped JSON lacks:
        # only the generator can give the run tree that key.
        copy="$STRILLIX_ROOT/rices/runtime-copy"
        cp -r "$STRILLIX_ROOT/rices/${rice}" "$copy"
        chmod -R u+w "$copy"
        for file in manifest.json registry.json; do
          if jq -e 'has("runtime-copy")' "$STRILLIX_TEMPLATES/$file" >/dev/null; then
            fail "the shipped $file already has a runtime-copy key, so the check proves nothing"
          fi
        done
        if ! "$strillix" rice stage runtime-copy --json >"$TMPDIR/stage.json" 2>"$TMPDIR/stage.err"; then
          fail "rice stage runtime-copy exited non-zero"
        fi
        if [ "$(jq -r .status "$TMPDIR/stage.json")" != ok ]; then
          fail "rice stage runtime-copy did not report status ok"
        fi
        for file in manifest.json registry.json; do
          if ! jq -e 'has("runtime-copy")' "$AOIDE_ROOT/run/qml/rices/$file" >/dev/null; then
            fail "run/qml/rices/$file is not JSON with a runtime-copy key"
          fi
        done
        printf 'aoide check generator-runtime: ok\n' > "$out"
      '';
in
{
  inherit
    assertCheck
    surfaceOwnership
    riceRuntimeUntracked
    riceShape
    barRicemode
    fmt
    discovery
    phantomCommands
    nixIndependence
    boundary
    portability
    nixLint
    themeFanout
    generatorOffline
    generatorRelocatable
    generatorRuntime
    ;
}
