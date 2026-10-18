#!/usr/bin/env bash
# The activation seed's cover rule (CONTRACTS.md §4, the writer table), as a
# file so the test in tests/cover-seed/run.sh runs the bytes the activation runs.
#
#   cover-seed.sh <jq> <aoide-root> <song> <wallpaper>
#
# A standing pick of ANY song is left byte-identical: the read side hides a
# pick for another song, and a seed that overwrote it would destroy it. Any
# other cover (absent, or a default) becomes the declared song's own picture.
set -euo pipefail
jq=$1 root=$2 song=$3 wallpaper=$4
cover="$root/song/stage/cover.json"
if [[ -f $cover ]] && "$jq" -e '.pick == true' "$cover" >/dev/null 2>&1; then
  exit 0
fi
ctmp=$(mktemp "$root/song/stage/.cover.json.XXXXXX")
"$jq" -n --arg p "$wallpaper" --arg s "$song" '{path: $p, song: $s}' > "$ctmp"
mv -f "$ctmp" "$cover"
