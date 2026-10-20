#!/usr/bin/env bash
# Runs every case that is Aoide's own and checks it against its expectation:
# the rices, the packages walker, and Aoide's real registry composed through
# habit. Composition itself is habit's and is tested there. A negative case must
# fail AND say something useful: the runner greps the real stderr, so a vague
# error is a failing test, not a passing one.
#
#   ./tests/selection/run.sh            # all cases
#   ./tests/selection/run.sh riceDiscovery
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)

# lib and habit come from the flake's own lock, so the cases run against the lib
# every host evaluates with and the habit every host is built by.
rev=$(jq -r '.nodes.nixpkgs.locked.rev' "$root/flake.lock")
lib="(builtins.getFlake \"github:nixos/nixpkgs/$rev\").lib"
habitRev=$(jq -r '.nodes.habit.locked.rev' "$root/flake.lock")
habit="(builtins.getFlake \"github:dxcently/habit/$habitRev\")"

# case                              expect  substring the error must contain
cases=$(cat <<'EOF'
realRegistryResolvesStrillix         ok      "compositor,hyprland,stylix:hyprland"
realRegistryDendritesImport         ok      true
realRegistryInventory               ok      "agents,desktop,nucleus,strillix"
strillixWithoutAoide                ok      "true:false:true"
riceDiscovery                       ok      "alpha landmine noshelf"
riceLandmineUnread                  ok      "1:true"
riceLandmineFires                   throws  rices landmine: an unselected rice's rice.nix was imported
riceUnknownName                     throws  'nope' is not in the rices; discovered: alpha, landmine, noshelf
riceAvailableUnknown                throws  'nope2' is not in the rices; discovered: alpha, landmine, noshelf
riceNoRice                          throws  rice noshelf has no rice.nix
riceStrayEnabled                    throws  rice aggregation lender is enabled, but habit.rice.declared = "borrower"
riceNameClash                       throws  share a name with an aggregation
riceDeclaredBuilds                  ok      1
riceEscapeClean                     ok      ""
riceEscapeFound                     ok      "escaper/rice.nix"
riceBorrowClosure                   ok      "borrower lender"
riceBorrowOwnersResolve             ok      "all-resolve"
barRicemodeOmitted                  throws  bar-ricemode FAILED: bar bodies without `slot: "ricemode"`: mute/widgets/bar.qml
barRicemodeEmbedded                 ok      "built aoide-check-bar-ricemode"
barRicemodeNoFloor                  throws  bar-ricemode FAILED: sonata has no `ricemode` slot
walkerYieldsListedOverride          ok      "the lane's rices"
walkerRefusesUnlistedOverride       throws  listed in intentionalOverrides
EOF
)

only="${1:-}"
pass=0; fail=0
printf '%-42s %s\n' "CASE" "RESULT"
printf '%s\n' "------------------------------------------------------------"
while read -r name expect want; do
  [ -z "$name" ] && continue
  [ -n "$only" ] && [ "$only" != "$name" ] && continue
  out=$(nix eval --impure --json --show-trace \
          --expr "(import ./cases.nix { lib = $lib; habit = $habit; }).$name" 2>&1)
  rc=$?
  if [ "$expect" = ok ]; then
    if [ $rc -eq 0 ] && [ "$(printf '%s' "$out" | tail -1)" = "$want" ]; then
      printf '%-42s PASS\n' "$name"; pass=$((pass+1))
    else
      printf '%-42s FAIL (want %s, rc=%s)\n' "$name" "$want" "$rc"
      printf '%s\n' "$out" | tail -6 | sed 's/^/    | /'
      fail=$((fail+1))
    fi
  else
    if [ $rc -ne 0 ] && printf '%s' "$out" | grep -qF -- "$want"; then
      printf '%-42s PASS  (%s)\n' "$name" "$want"; pass=$((pass+1))
    else
      printf '%-42s FAIL (error missing %s, rc=%s)\n' "$name" "$want" "$rc"
      printf '%s\n' "$out" | grep -v '^ *$' | tail -8 | sed 's/^/    | /'
      fail=$((fail+1))
    fi
  fi
done <<< "$cases"

printf '%s\n' "------------------------------------------------------------"
printf '%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
