#!/usr/bin/env bash
# The quickshell/lyra seam, evaluated: three hosts, and what each one owes.
#
#   ./tests/quickshell-seam/run.sh                 # this checkout's working tree
#   ./tests/quickshell-seam/run.sh path:/abs/dir      # some other checkout
#   ./tests/quickshell-seam/run.sh git+file:///abs/dir?rev=<sha>   # a frozen commit
#
# The ref is the TREE under test, not just a source of inputs: `fixtures.nix`
# reads the constructor, the catalogue, nucleus and the three host records out
# of it, so a commit ref tests that commit. Every eval below carries the ref.
#
# `test-quickshell-only` — quickshell selected, `aoide.quickshell.config` = the
# fixture directory, no lyra: the host's own shell runs (one `aoide-quickshell`
# unit, on that directory), and nothing lyra would have brought is there (no
# rice binary, no `songs/`, no shellbridge unit, no healthcheck, no deploy
# activation).
# `test-quickshell-bare` — the same selection with no config: package installed,
# no service, no session anchor claimed.
# `test-song-without-lyra` — a song and no performer: MUST fail, with the
# platform assertion's own message.
# `yomi-strix` — the positive control, from the same ref: the two absence
# checks above are only worth anything against a host that HAS the things.
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
  check "only: no lyra fact"                    "false" "$(o .lyra)"
  check "only: no rice binary installed"        "0" \
    "$(o '[.packages[] | select(test("-rice$"))] | length')"
  check "only: no song"                         "null" "$(o .song)"
  check "only: config holds only shell.qml"     "shell.qml" \
    "$(o '.configEntries | join(",")')"
  check "only: no shellbridge unit"             "0" \
    "$(o '[.userServices[], .systemdUserServices[], .systemServices[] | select(test("shellbridge"))] | length')"
  check "only: no healthcheck unit or timer"    "0" \
    "$(o '[.userServices[], .userTimers[], .systemdUserServices[], .systemServices[] | select(test("healthcheck"))] | length')"
  check "only: no lane activation"              "0" \
    "$(o '[.activation[] | select(test("^aoide"))] | length')"
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
  check "bare: no lyra fact"                    "false" "$(b .lyra)"
  check "bare: no shellbridge unit"             "0" \
    "$(b '[.systemdUserServices[] | select(test("shellbridge"))] | length')"
fi

# ── test-song-without-lyra: the eval that must fail ──────────────────────────
if evalf songWithoutLyra "$tmp/song.err" >/dev/null; then
  printf '%-46s FAIL (evaluated cleanly)\n' "song without lyra is refused"
  fail=$((fail+1))
elif grep -qF -- 'needs the lyra dendrite' "$tmp/song.err"; then
  printf '%-46s PASS  (%s)\n' "song without lyra is refused" \
    "$(tr -d '\n' < "$tmp/song.err" | grep -o -m1 -- 'aoide.song = "sonata" needs the lyra dendrite: select [^"]*')"
  pass=$((pass+1))
else
  printf '%-46s FAIL (wrong error)\n' "song without lyra is refused"
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
    "$(y '[.packages[] | select(test("-rice$"))] | length')"
fi

printf '%s\n' "-----------------------------------------------------------------"
printf '%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
