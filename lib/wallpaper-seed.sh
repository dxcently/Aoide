#!/usr/bin/env bash
# The activation seed's wallpaper rule (CONTRACTS.md §4, the writer table), as a
# file so the test in tests/wallpaper-seed/run.sh runs the bytes the activation runs.
#
#   wallpaper-seed.sh <jq> <strillix-root> <song> <wallpaper>
#
# A standing pick of ANY song is left byte-identical: the read side hides a
# pick for another song, and a seed that overwrote it would destroy it. Any
# other wallpaper (absent, or a default) becomes the declared song's own picture.
set -euo pipefail
jq=$1 root=$2 song=$3 wallpaper=$4
stage="$root/stage/wallpaper.json"
if [[ -f $stage ]] && "$jq" -e '.pick == true' "$stage" >/dev/null 2>&1; then
  exit 0
fi
wtmp=$(mktemp "$root/stage/.wallpaper.json.XXXXXX")
"$jq" -n --arg p "$wallpaper" --arg s "$song" '{path: $p, song: $s}' > "$wtmp"
mv -f "$wtmp" "$stage"
