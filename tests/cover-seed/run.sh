#!/usr/bin/env bash
# The activation seed's cover rule (lib/cover-seed.sh), against a private root.
# Each case seeds one cover.json and checks what the file says afterwards.
#
#   ./tests/cover-seed/run.sh
set -uo pipefail
cd "$(dirname "$0")" || exit 1
seed=$(cd ../.. && pwd)/lib/cover-seed.sh
jq=${JQ:-jq}
work=$(mktemp -d "${TMPDIR:-/tmp}/cover-seed.XXXXXX")
trap 'rm -rf "$work"' EXIT
wall=/nix/store/fixture-barcarolle-desktop.jpg
fail=0

# case <name> <cover.json body or "-" for absent>
seedcase() {
  local name=$1 body=$2 root=$work/$1
  mkdir -p "$root/stage"
  [[ $body != - ]] && printf '%s\n' "$body" > "$root/stage/cover.json"
  bash "$seed" "$jq" "$root" barcarolle "$wall" || { echo "FAIL  $name: seed exited"; fail=1; return; }
  cat "$root/stage/cover.json"
}

expect_default() {
  local got; got=$(seedcase "$1" "$2")
  if [[ $("$jq" -c . <<<"$got") == "$("$jq" -cn --arg p "$wall" '{path:$p,song:"barcarolle"}')" ]]; then
    echo "ok    $1: default written"
  else echo "FAIL  $1: $got"; fail=1; fi
}

expect_kept() {
  local got; got=$(seedcase "$1" "$2")
  if [[ $got == "$2" ]]; then echo "ok    $1: pick kept byte-identical"; else echo "FAIL  $1: $got"; fail=1; fi
}

expect_default fresh -
expect_default stale-default '{"path":"/x/cadenza.png","song":"cadenza"}'
expect_kept same-song-pick '{"path":"/x/barcarolle.png","kind":"still","pick":true,"song":"barcarolle"}'
expect_kept other-song-pick '{"path":"/x/cadenza.png","kind":"still","pick":true,"song":"cadenza"}'
expect_kept legacy-pick '{"path":"/x/old.png","pick":true}'
expect_default malformed 'not json'
exit $fail
