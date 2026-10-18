#!/usr/bin/env bash
# The quickshell/strillix seam, evaluated: four hosts, and what each one owes.
#
#   ./tests/quickshell-seam/run.sh                 # this checkout's working tree
#   ./tests/quickshell-seam/run.sh path:/abs/dir      # some other checkout
#   ./tests/quickshell-seam/run.sh git+file:///abs/dir?rev=<sha>   # a frozen commit
#
# The ref is the TREE under test, not just a source of inputs: `fixtures.nix`
# reads the constructor, the catalogue, nucleus and the four host modules out
# of it, so a commit ref tests that commit. Every eval below carries the ref.
#
# `test-quickshell-only` — quickshell selected, `aoide.quickshell.config` = the
# fixture directory, no strillix: the host's own shell runs (one `aoide-quickshell`
# unit, on that directory), and nothing strillix would have brought is there (no
# rice binary, no `songs/`, no shellbridge unit, no healthcheck, no rice-reload
# unit, no deploy activation).
# `test-quickshell-bare` — the same selection with no config: package installed,
# no service, no session anchor claimed.
# `test-two-components` — the `bar` and `dock` slots filled by their strillix
# providers, no strillix and no quickshell dendrite: the runtime both import by path
# lands once (one shell unit, one shellbridge unit, no doubled environment, one
# declaration of its home half), both components are recorded, and both facts
# the runtime sets are true.
# `test-song-without-strillix` — a song and no performer: MUST fail, with the
# platform assertion's own message.
# `yomi-strix` — the positive control, from the same ref: the two absence
# checks above are only worth anything against a host that HAS the things,
# among them the `aoide-rice-reload` unit that brings a staged song back after a
# login or a switch, and the switch's one try-restart that runs it before the shell.
#
# A fixture that fails to evaluate is a failing test, not a skipped one; the
# runner keeps the real stderr so a vague error cannot pass. Every reading is
# one `nix eval` of one fixture, and stdout is JSON only (the flake-search-path
# warning is not part of an answer).
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)

# The checkout under test. The default is THIS one, working tree included — a
# change in progress is what these fixtures are for. Pass a flakeref to freeze it.
ref=${1:-path:$root}

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

pass=0; fail=0
check() { # name  expected  actual
  if [ "$2" = "$3" ]; then
    printf '%-46s PASS\n' "$1"; pass=$((pass+1))
  else
    printf '%-46s FAIL (want %s, got %s)\n' "$1" "$2" "$3"; fail=$((fail+1))
  fi
}

# One fixture in, its readings (or its trace) out. stderr lands in "$2" so a
# failure can be shown as it happened rather than summarized.
evalf() { # attr  errfile
  nix eval --impure --json --show-trace \
    --expr "(import ./fixtures.nix { flake = builtins.getFlake \"$ref\"; }).$1" 2>"$2"
}

printf '%-46s %s\n' "CHECK" "RESULT"
printf '%s\n' "-----------------------------------------------------------------"

# ── test-quickshell-only ─────────────────────────────────────────────────────
if ! only=$(evalf only "$tmp/only.err"); then
  printf '%-46s FAIL\n' "test-quickshell-only evaluates"
  grep -v '^ *$' "$tmp/only.err" | tail -12 | sed 's/^/    | /'
  fail=$((fail+1))
else
  o() { printf '%s' "$only" | jq -r "$1"; }
  cfg=$(o .config)
  check "only: quickshell package installed"    "1" \
    "$(o '[.packages[] | select(test("quickshell"))] | length')"
  check "only: exactly one shell unit"          "1" \
    "$(o '[.userServices[] | select(test("quickshell"))] | length')"
  check "only: the unit runs the host config"   "$cfg/shell.qml" \
    "$(o '(.service.execStart | if type == "array" then .[0] else . end) | sub(".* -p "; "")')"
  check "only: condition guards the same entry" "$cfg/shell.qml" \
    "$(o .service.condition)"
  check "only: graphical session anchored"      "graphical-session.target" \
    "$(o .sessionTarget)"
  check "only: no strillix fact"                    "false" "$(o .strillix)"
  check "only: no rice binary installed"        "0" \
    "$(o '[.packages[] | select(test("-strillix$"))] | length')"
  check "only: no song"                         "null" "$(o .song)"
  check "only: config holds only shell.qml"     "shell.qml" \
    "$(o '.configEntries | join(",")')"
  check "only: no shellbridge unit"             "0" \
    "$(o '[.userServices[], .systemdUserServices[], .systemServices[] | select(test("shellbridge"))] | length')"
  check "only: no healthcheck unit or timer"    "0" \
    "$(o '[.userServices[], .userTimers[], .systemdUserServices[], .systemServices[] | select(test("healthcheck"))] | length')"
  check "only: no rice-reload unit"             "0" \
    "$(o '[.userServices[], .systemdUserServices[], .systemServices[] | select(test("rice-reload"))] | length')"
  check "only: no lane activation"              "0" \
    "$(o '[.activation[] | select(test("^aoide"))] | length')"
  # No strillix, no reader: the units that would carry the staging path's own
  # templates variable must not carry it (paired with the control below).
  check "only: no templates var on any unit"    "[]" "$(o '.templates')"
fi

# ── test-quickshell-bare ─────────────────────────────────────────────────────
if ! bare=$(evalf bare "$tmp/bare.err"); then
  printf '%-46s FAIL\n' "test-quickshell-bare evaluates"
  grep -v '^ *$' "$tmp/bare.err" | tail -12 | sed 's/^/    | /'
  fail=$((fail+1))
else
  b() { printf '%s' "$bare" | jq -r "$1"; }
  check "bare: quickshell package installed"    "1" \
    "$(b '[.packages[] | select(test("quickshell"))] | length')"
  check "bare: no shell unit at all"            "0" \
    "$(b '[.userServices[] | select(test("quickshell"))] | length')"
  check "bare: no service recorded"             "null" "$(b .service)"
  # No `songs/` check here: with `config` null there is no directory to read, so
  # the reading would be false by construction. The `only` case above carries
  # it — there the directory exists and holds exactly its entry point.
  check "bare: config stays null"               "null" "$(b .config)"
  check "bare: no graphical session claimed"    "default.target" "$(b .sessionTarget)"
  check "bare: no strillix fact"                    "false" "$(b .strillix)"
  check "bare: no shellbridge unit"             "0" \
    "$(b '[.systemdUserServices[] | select(test("shellbridge"))] | length')"
fi

# ── test-two-components ──────────────────────────────────────────────────────
if ! two=$(evalf twoComponents "$tmp/two.err"); then
  printf '%-46s FAIL\n' "test-two-components evaluates"
  grep -v '^ *$' "$tmp/two.err" | tail -12 | sed 's/^/    | /'
  fail=$((fail+1))
else
  t() { printf '%s' "$two" | jq -r "$1"; }
  check "two: exactly one shell unit"           "1" "$(t .shellUnits)"
  check "two: exactly one shellbridge unit"     "1" "$(t .bridgeUnits)"
  check "two: no duplicate shell environment"   "0" "$(t .envDuplicates)"
  check "two: the home half is declared once"   "1" "$(t .homeDeclarations)"
  check "two: both components recorded"         "bar,dock" "$(t '.components | join(",")')"
  check "two: the strillix fact is set"             "true" "$(t .strillix)"
  check "two: the quickshell fact is set"       "true" "$(t .quickshell)"
fi

# ── test-song-without-strillix: the eval that must fail ──────────────────────────
if evalf songWithoutStrillix "$tmp/song.err" >/dev/null; then
  printf '%-46s FAIL (evaluated cleanly)\n' "song without the strillix runtime is refused"
  fail=$((fail+1))
elif grep -qF -- 'needs the strillix runtime' "$tmp/song.err"; then
  printf '%-46s PASS  (%s)\n' "song without the strillix runtime is refused" \
    "$(tr -d '\n' < "$tmp/song.err" | grep -o -m1 -- 'aoide.rice = "sonata" needs the strillix runtime: pick [^"]*')"
  pass=$((pass+1))
else
  printf '%-46s FAIL (wrong error)\n' "song without the strillix runtime is refused"
  grep -v '^ *$' "$tmp/song.err" | tail -8 | sed 's/^/    | /'
  fail=$((fail+1))
fi

# ── yomi-strix: the positive control ─────────────────────────────────────────
# Same ref, same namespaces. Without this, "no shellbridge" would stay green on
# a tree where nothing declares shellbridge anywhere — a check that cannot fail
# is not a check.
if ! yomi=$(evalf yomi "$tmp/yomi.err"); then
  printf '%-46s FAIL\n' "yomi-strix (control) evaluates"
  grep -v '^ *$' "$tmp/yomi.err" | tail -12 | sed 's/^/    | /'
  fail=$((fail+1))
else
  y() { printf '%s' "$yomi" | jq -r "$1"; }
  check "control: yomi HAS the shellbridge unit" "1" \
    "$(y '[.systemdUserServices[] | select(test("shellbridge"))] | length')"
  check "control: yomi HAS the rice binary"     "1" \
    "$(y '[.packages[] | select(test("-strillix$"))] | length')"
  # The staging path's session-sourced variable, declared on every unit a
  # staged song can descend from — and all three naming ONE directory, so a
  # unit cannot go stale on its own (found at the S9 yomi switch: an inherited
  # login-environment value survived a switch until the operator relogged).
  check "control: aoided declares the templates" "1" "$(y '.templates.aoided | length')"
  check "control: shellbridge declares it"      "1" "$(y '.templates.shellbridge | length')"
  check "control: the shell declares it"        "1" "$(y '.templates.quickshell | length')"
  check "control: the rice reload declares it"  "1" "$(y '.templates.riceReload | length')"
  check "control: every unit names one directory" "true" "$(y '.templatesAgree')"
  check "control: it is the shipped rices"   "true" \
    "$(y '.templates.aoided[0] | endswith("-strillix-rices-templates/share/strillix/rices")')"
  # The unit that brings a staged or drafted song back after a login or a
  # switch: ordered before the shell (its first frame is the staged song), run in
  # the user manager once the session is up, one shot that stays active even when
  # `strillix reload` refuses or is cut off (the `-`), bounded by `timeout` because a
  # oneshot has no start timeout of its own, carrying the runtime root and the
  # templates but no PATH (the manager's own reaches the tools).
  check "control: the rice reload is before the shell" "1" \
    "$(y '[(.riceReload.Unit.Before // [])[] | select(. == "aoide-quickshell.service")] | length')"
  check "control: the rice reload follows the session" "1" \
    "$(y '[(.riceReload.Unit.After // [])[] | select(. == "graphical-session.target")] | length')"
  check "control: the rice reload is part of it" "1" \
    "$(y '[(.riceReload.Unit.PartOf // [])[] | select(. == "graphical-session.target")] | length')"
  check "control: the rice reload starts with it" "1" \
    "$(y '[(.riceReload.Install.WantedBy // [])[] | select(. == "graphical-session.target")] | length')"
  check "control: the rice reload is one shot"  "oneshot" "$(y '.riceReload.Service.Type')"
  check "control: the rice reload stays active" "true" "$(y '.riceReload.Service.RemainAfterExit')"
  check "control: a refused reload is not a failure" "true" \
    "$(y '.riceReload.Service.ExecStart | if type == "array" then .[0] else . end | startswith("-") and endswith("/bin/strillix reload")')"
  check "control: the rice reload is bounded"   "true" \
    "$(y '.riceReload.Service.ExecStart | if type == "array" then .[0] else . end | test("^-[^ ]+/bin/timeout [0-9]+ [^ ]+/bin/strillix reload$")')"
  check "control: the rice reload has both roots, templates" "3" \
    "$(y '[(.riceReload.Service.Environment // [])[] | select(startswith("AOIDE_ROOT=") or startswith("STRILLIX_ROOT=") or startswith("STRILLIX_TEMPLATES="))] | length')"
  check "control: the rice reload names no PATH" "0" \
    "$(y '[(.riceReload.Service.Environment // [])[] | select(startswith("PATH="))] | length')"
  # The switch's half: after home-manager has linked its files and reloaded the
  # user manager, ONE try-restart names both units, so systemd orders the reload
  # before the shell.
  check "control: restart follows hm's file step" "1" \
    "$(y '[(.restartRice.after // [])[] | select(. == "onFilesChange")] | length')"
  check "control: restart follows hm's systemd step" "1" \
    "$(y '[(.restartRice.after // [])[] | select(. == "reloadSystemd")] | length')"
  check "control: one restart runs both units"  "true" \
    "$(y '(.restartRice.data // "") | contains("try-restart aoide-rice-reload.service aoide-quickshell.service")')"
fi

printf '%s\n' "-----------------------------------------------------------------"
printf '%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
