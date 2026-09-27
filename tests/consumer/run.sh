#!/usr/bin/env bash
# The consumer fixture, evaluated: a stranger's flake whose ONLY contact with
# AoideOS is the root flake's exported surface.
#
#   ./tests/consumer/run.sh                 # this checkout's working tree
#   ./tests/consumer/run.sh git+file:///abs/dir?rev=<sha>   # a frozen commit
#
# Two proofs, in that order:
#
#   1. The reach-in grep. `flake.nix` and its neighbours must name NO path into
#      Aoide's source tree (`<aoide> + "/…"`) and no Aoide input threading
#      (`aoide.inputs.<x>`). This is the migration contract's own words: a
#      consumer consumes Aoide ONLY through public flake outputs.
#   2. The evaluation. `flake.nix`'s `outputs` is applied to the REF's own
#      inputs (so a commit ref really freezes the tree under test, the same
#      discipline tests/quickshell-seam uses), and the resulting host is read:
#      it selects the nucleus lane, lyra, the shell and stylix; it performs one
#      song and builds in a second that BORROWS the first's widgets; and the
#      song machinery a real host gets (deployed config, seeded stage, seeded
#      machine songbook, templates on the units) is there.
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)

ref=${1:-path:$root}

pass=0; fail=0
check() { # name  expected  actual
  if [ "$2" = "$3" ]; then
    printf '%-52s PASS\n' "$1"; pass=$((pass+1))
  else
    printf '%-52s FAIL (want %s, got %s)\n' "$1" "$2" "$3"; fail=$((fail+1))
  fi
}

printf '%-52s %s\n' "CHECK" "RESULT"
printf '%s\n' "--------------------------------------------------------------------"

# ── 1. No reach-in ──────────────────────────────────────────────────────────
# `<anything> + "/` on the aoide input is the shape being banned; `aoide.lib.…`
# and `aoide.nixosModules.…` are the shape being required. Both greps run over
# every file in this directory, so a new fixture file cannot opt out.
reachin=$(grep -rnE 'aoide *\+ *"/|aoide\.inputs\.' --include='*.nix' . || true)
if [ -z "$reachin" ]; then
  printf '%-52s PASS\n' "no path into Aoide, no input threading"; pass=$((pass+1))
else
  printf '%-52s FAIL\n' "no path into Aoide, no input threading"
  printf '%s\n' "$reachin" | sed 's/^/    | /'
  fail=$((fail+1))
fi

# The positive control for that grep: it must be able to fire. A pattern that
# matches nothing anywhere is not evidence.
if grep -qE 'aoide *\+ *"/' <(printf 'x = aoide + "/modules/default.nix";\n'); then
  printf '%-52s PASS\n' "the reach-in grep can match"; pass=$((pass+1))
else
  printf '%-52s FAIL\n' "the reach-in grep can match"; fail=$((fail+1))
fi

# ── 2. The consumer's host ──────────────────────────────────────────────────
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

expr="
  let
    flake = builtins.getFlake \"$ref\";
    out = (import ./flake.nix).outputs {
      nixpkgs = flake.inputs.nixpkgs;
      home-manager = flake.inputs.home-manager;
      aoide = flake;
    };
    cfg = out.nixosConfigurations.consumer.config;
    home = cfg.home-manager.users.\${cfg.aoide.user};
    envOf = units: unit: let u = units.\${unit} or { }; in u.serviceConfig.Environment or u.Service.Environment or [ ];
  in {
    drv = out.nixosConfigurations.consumer.config.system.build.toplevel.drvPath;
    song = cfg.aoide.song;
    builtIn = cfg.aoide.songbook.builtIn;
    bg = cfg.aoide.livery.palette.bg;
    lyra = cfg.aoide.lyra.enable;
    stylix = cfg.aoide.stylix.enable;
    quickshell = cfg.aoide.quickshell.enable;
    config = cfg.aoide.quickshell.config;
    activation = builtins.attrNames (home.home.activation or { });
    hmServices = builtins.attrNames (home.systemd.user.services or { });
    systemdUserServices = builtins.attrNames cfg.systemd.user.services;
    templates = map (e: builtins.replaceStrings [ \"AOIDE_SONG_TEMPLATES=\" ] [ \"\" ] e) (
      builtins.filter (e: builtins.match \"AOIDE_SONG_TEMPLATES=.*\" e != null)
        (envOf cfg.systemd.user.services \"aoided\" ++ envOf cfg.systemd.user.services \"shellbridge\")
    );
  }
"

if ! out=$(nix eval --impure --json --show-trace --expr "$expr" 2>"$tmp/eval.err"); then
  printf '%-52s FAIL\n' "the consumer host evaluates"
  grep -v '^ *$' "$tmp/eval.err" | tail -14 | sed 's/^/    | /'
  fail=$((fail+1))
  printf '%s\n' "--------------------------------------------------------------------"
  printf '%d passed, %d failed\n' "$pass" "$fail"
  exit 1
fi
printf '%-52s PASS\n' "the consumer host evaluates"; pass=$((pass+1))

o() { printf '%s' "$out" | jq -r "$1"; }
cfg=$(o .config)

check "performs the declared song"            "solo" "$(o .song)"
check "builds in declared + available"        "duet solo" "$(o '.builtIn | join(" ")')"
check "the available song's livery is inert"  "#14161c" "$(o .bg)"
check "the lyra lane is on"                   "true" "$(o .lyra)"
check "the stylix lane is on"                 "true" "$(o .stylix)"
check "the shell lane is on"                  "true" "$(o .quickshell)"
check "the shell runs the deployed config"    "/home/fixture/.aoide/run/qml" "$(o .config)"
check "the song's seed activation is there"   "1" \
  "$(o '[.activation[] | select(test("^aoideSeedSongbook$"))] | length')"
check "the shell's unit is there"             "1" \
  "$(o '[.hmServices[] | select(test("^aoide-quickshell$"))] | length')"
check "the bridge's unit is there"            "1" \
  "$(o '[.systemdUserServices[] | select(test("^shellbridge$"))] | length')"
check "the units carry the templates"         "2" "$(o '.templates | length')"
check "…and both name one directory"          "true" \
  "$(o '(.templates | unique | length) == 1 and (.templates[0] | endswith("-lyra-songbook-templates/share/lyra/songbook"))')"

printf '%-52s %s\n' "consumer toplevel drvPath" "$(o .drv)"

printf '%s\n' "--------------------------------------------------------------------"
printf '%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
