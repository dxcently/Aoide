# lib/checks.nix — the contractual coupling discipline, as flake checks.
#
# Seven checks ride as flake `checks` (see concepts/Governance and
# concepts/Notes in the wiki, and the mechanical-integrity design for fmt +
# discovery specifically):
#
#   1. surface-ownership — no render surface may have two owners. The
#      Quickshell facet declares `aoide.surfaces.<name>.owner`; Stylix reads
#      the same registry and disables derivation for owned surfaces. Two
#      modules claiming the same surface is a build-time error.
#
#   2. no-song-read — no module may make the nix build depend on a `song/`
#      runtime path (`stage/`, `auditions/`, …). `stage/` can
#      never become load-bearing for the frozen half. Enforced structurally:
#      an evaluated nixos config never imports/reads those paths, so we assert
#      over the module tree's source strings.
#
#   3. song-shape — a committed song under song/songbook/<name>/ is exactly
#      one rice.nix, never a stray extra module riding along.
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
#   6. phantom-commands — every backticked `aoide …` / `lyra …` invocation
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
#      walk of `[dependencies]`/`[build-dependencies]` starting at
#      `crates/cli` (never `[dev-dependencies]` — test-support never ships),
#      then asserts the closure never reaches `aoide-song`/`aoide-screen`/
#      `aoide-lyra` and that no closure crate's `src/` shells out to nix.
#
# 1–3 and 5 are written so they PASS TRIVIALLY where nothing populates the
# registry they inspect yet (1) and become real as Wave-1 facets/packages
# land. Each resolves to a trivial derivation: it either builds (assertion
# held) or the eval fails with a readable message (assertion broken). 4, 6,
# and 7 are real `runCommand`s — each has to actually run a binary, so it can
# only fail at build time, not eval time.
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
  # Consumes the evaluated `aoide.surfaces` registry. Duplicate detection is a
  # no-op today because attrsets cannot hold duplicate keys; the real teeth
  # arrive when a facet asserts, in its own module, that it is the sole owner
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

  # ── Check 2: no song/ RUNTIME read at build time ───────────────────────────
  # Asserts that no walked module path lives under a `song/` RUNTIME dir. The
  # ban is scoped to ephemeral runtime state (stage/ · auditions/ · catalog/ ·
  # index/) — `stage/` can never become load-bearing for the frozen half. It
  # deliberately does NOT list `song/songbook/` wholesale: committed songs
  # there are VERSIONED SCORE, legitimately walked at eval by lib/mkHost.nix
  # (each song's rice.nix self-gates on `aoide.song`). Walking the songbook
  # therefore never trips this check on its own — EXCEPT the `drafts/`
  # subfolder nested inside each song (`song/songbook/<name>/drafts/`,
  # `rice draft save`'s scratch tree): that one runtime dir sits INSIDE an
  # otherwise-legitimate songbook path, so a flat infix can't name it (the
  # song name varies) — matched by regex instead, scoped tightly to just the
  # `drafts/` subfolder, never the songbook entry itself.
  noSongRead =
    modulePaths:
    let
      runtimeInfixes = [
        "/song/stage/"
        "/song/auditions/"
        "/song/catalog/"
        "/song/index/"
      ];
      isSongDraft = s: builtins.match ".*/song/songbook/[^/]+/drafts/.*" s != null;
      offenders = builtins.filter (
        p:
        let
          s = toString p;
        in
        builtins.any (needle: lib.hasInfix needle s) runtimeInfixes || isSongDraft s
      ) modulePaths;
    in
    assertCheck "no-song-read" (
      offenders == [ ]
    ) "modules read song/ runtime paths at build time: ${builtins.toString offenders}";

  # ── Check 3: song shape (host-agnostic discipline) ─────────────────────────
  # A committed song under song/songbook/<name>/ carries ONLY notes: its
  # rice.nix sets aoide.notes (palette + component tiers) and — later —
  # cover/chime references inside song/. It must NEVER set host options
  # (monitors, hardware, services) or enable facets/dendrites: the VENUE (host)
  # decides its instruments, the SONG carries only the notes (CONTRACTS.md §5).
  #
  # A cheap STRUCTURAL slice of that discipline is enforced here: every walked
  # songbook path is a file named `rice.nix` (the song's module entry). This
  # catches stray `.nix` in a song folder that would silently join the module
  # merge and could set arbitrary host options.
  #
  # TODO(song-shape v1): the full "only defines aoide.notes" invariant needs
  # per-module isolated eval + option-definition diffing — disproportionate for
  # v0. Until then the invariant is a DOCUMENTED CONVENTION (CONTRACTS.md §5 /
  # docs/BUILD.md), backed by this structural rice.nix-only gate and code review.
  # Joins that convention: a song must never set `aoide.livery.override.*`
  # (CONTRACTS.md §5) — the override tier is HOST-set only.
  songShape =
    songbookPaths:
    let
      strays = builtins.filter (
        p:
        let
          s = toString p;
        in
        !lib.hasSuffix "/rice.nix" s
      ) songbookPaths;
    in
    assertCheck "song-shape" (
      strays == [ ]
    ) "songbook holds non-rice.nix modules (a song is rice.nix only): ${builtins.toString strays}";

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
  # The agent docs (root AGENTS.md + docs/agent/*.md) are the one place the
  # command surface is still taught in hand-written prose — both binaries'
  # guide texts are registry-derived, so they cannot drift and are not
  # re-checked here. Every backticked invocation spelling those docs teach
  # (`aoide send …`, `lyra rice compose <name>`, …) must resolve against
  # the matching binary's `schema --json`, built from this checked-out source:
  # the spelled words must be a registered command path, or a prefix of one
  # (`aoide send` validly teaches the `send` leaf; `aoide secrets`
  # alone validly names a group). A spelling that resolves to nothing fails
  # the build naming the file, line, and spelling.
  #
  # Extraction is precision-over-recall: only inline code spans STARTING with
  # `aoide `/`lyra ` followed by a lowercase word count as teachings (a span
  # may wrap across a line break); the path walk stops at the first non-path
  # token (`<args>`, `[flags]`, `--flags`, `--`, ellipses), and comment lines
  # inside fenced code blocks are skipped. `src` is the flake's own store
  # copy, so like Check 4 this scopes to the COMMITTED tree; `aoidePkg` is
  # the self-flaked core package — `aoide` from its default output, `lyra`
  # from its `rice` output.
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

        # Blank out comment lines inside fenced code blocks, preserving
        # offsets: fenced commentary may name a spelling without teaching it.
        my @lines = split /\n/, $text, -1;
        my $fence = 0;
        for my $i (0 .. $#lines) {
          my $l = $lines[$i];
          if ($l =~ /^\s*(```|~~~)/) { $fence = !$fence; next; }
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
          next unless $span =~ /^(aoide|lyra) [a-z]/;
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
        ${aoidePkg.rice}/bin/lyra schema --json \
          | jq -r '.commands[].path | join(" ")' > "$TMPDIR/lyra-paths"

        cd ${src}
        for doc in AGENTS.md docs/agent/*.md; do
          perl ${extract} "$doc"
        done > "$TMPDIR/candidates"

        status=0
        while IFS="$(printf '\t')" read -r file line bin words; do
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
  # NixOS assumption; only `lyra` (and the deployment modules) may depend on
  # nix (P-A5 of the binary-split workstream; cli/Cargo.toml:36-43 documents
  # the boundary this check makes real). The closure is DERIVED, never
  # hardcoded: `pkgs/aoide/Cargo.toml`'s `[workspace.dependencies]` gives an
  # aoide-* name -> `crates/` directory map, then a transitive walk of
  # `[dependencies]`/`[build-dependencies]` from `crates/cli` builds the
  # closure — `[dev-dependencies]` is never walked (test-support never
  # ships, same reasoning `aoide-cli`'s own manifest states for
  # `aoide-test-support`). Regex-based, like Check 6: any aoide-* entry
  # neither TOML file states as a plain single-line `{ path = "crates/…" }`
  # or `{ workspace = true }` (a multi-line inline table, a `package = "…"`
  # rename) fails the build naming the offending line instead of walking
  # past it unseen.
  #
  # Two assertions. First, the closure must not contain `aoide-song`,
  # `aoide-screen`, or `aoide-lyra` — a directory denylist would pass this
  # while core silently grew an `aoide-song` dependency and inherited its
  # nix-eval transitively; deriving the closure is what catches that.
  # Second, no closure crate's `src/` shells out to nix: a quoted `"nix"` /
  # `"nix-<word>"` literal, or a quoted string naming `nixos-rebuild` or
  # `nix eval|build|flake|develop|run|shell|store|copy`, on a line that also
  # spawns a process (`Command::new(`/`.arg(`/`.args(`) — gated on the
  # process-spawn call so a `--help` summary that merely mentions `nix flake
  # check`, or a test fixture mocking a `"sudo nixos-rebuild switch"`
  # activity label, is prose/data rather than a violation.
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

        # Full-line TOML comments (first non-whitespace char '#') stripped
        # before any aoide-* scanning below: both the workspace manifest and
        # every crate manifest carry long prose comments naming
        # aoide-song/aoide-screen/aoide-lyra that are not dependency edges.
        sub strip_comment_lines {
            my ($text) = @_;
            return join "\n", grep { !/^\s*#/ } split /\n/, $text, -1;
        }

        # One [section] block: from its header line to the next top-level
        # [..] header or EOF.
        sub section {
            my ($text, $name) = @_;
            my @lines = split /\n/, $text, -1;
            my $start;
            for my $i (0 .. $#lines) {
                if ($lines[$i] =~ /^\[\Q$name\E\]\s*$/) { $start = $i + 1; last; }
            }
            return "" unless defined $start;
            my @out;
            for my $i ($start .. $#lines) {
                last if $lines[$i] =~ /^\[/;
                push @out, $lines[$i];
            }
            return join "\n", @out;
        }

        # ── 1. workspace.dependencies: aoide-* name -> crates/ directory ────
        my $ws_text = read_file($ws_toml);
        my $ws_deps = strip_comment_lines(section($ws_text, "workspace.dependencies"));

        my %dir_of;
        my @ws_ambiguous;
        for my $line (split /\n/, $ws_deps) {
            next unless $line =~ /aoide-/;
            if ($line =~ /^(aoide-[\w-]+)\s*=\s*\{\s*path\s*=\s*"crates\/([\w-]+)"\s*\}\s*$/) {
                $dir_of{$1} = $2;
            } else {
                push @ws_ambiguous, $line;
            }
        }
        if (@ws_ambiguous) {
            die "aoide check nix-independence FAILED: workspace.dependencies has an aoide-* "
              . "entry this check cannot parse as a single-line { path = \"crates/...\" }:\n"
              . join("\n", @ws_ambiguous) . "\n";
        }

        # ── 2. walk [dependencies] + [build-dependencies] from crates/cli,
        # never [dev-dependencies] (test-support never ships) ───────────────
        my %visited = (cli => 1);
        my %parent;
        my @queue = ("cli");

        while (@queue) {
            my $dir = shift @queue;
            my $toml = "$root/$dir/Cargo.toml";
            die "aoide check nix-independence FAILED: no $toml (closure walk reached "
              . "an unresolved crate)\n" unless -f $toml;
            my $text = read_file($toml);
            for my $sect ("dependencies", "build-dependencies") {
                my $block = strip_comment_lines(section($text, $sect));
                for my $line (split /\n/, $block) {
                    next unless $line =~ /aoide-/;
                    if ($line =~ /^(aoide-[\w-]+)\s*=\s*\{\s*workspace\s*=\s*true\s*\}\s*$/) {
                        my $name = $1;
                        my $child = $dir_of{$name};
                        die "aoide check nix-independence FAILED: $toml [$sect] depends on "
                          . "$name, which workspace.dependencies never maps to a crates/ "
                          . "directory\n" unless defined $child;
                        unless ($visited{$child}) {
                            $visited{$child} = 1;
                            $parent{$child} = $dir;
                            push @queue, $child;
                        }
                    } else {
                        die "aoide check nix-independence FAILED: $toml [$sect] has an "
                          . "aoide-* entry this check cannot parse as a single-line "
                          . "{ workspace = true }:\n$line\n";
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

        # ── Assertion 1: the P-A5 boundary ──────────────────────────────────
        my @forbidden = grep { $visited{$_} } ("song", "screen", "lyra");
        if (@forbidden) {
            die "aoide check nix-independence FAILED: core closure reaches "
              . join(", ", map { "aoide-$_ (" . closure_path($_) . ")" } @forbidden)
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
in
{
  inherit
    assertCheck
    surfaceOwnership
    noSongRead
    songShape
    fmt
    discovery
    phantomCommands
    nixIndependence
    ;
}
