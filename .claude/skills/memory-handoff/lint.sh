#!/usr/bin/env bash
# Lint a Claude Code memory store. Report-only, never repairs — the same
# discipline `aoide soundcheck` holds for the working tree.
#
# usage: lint.sh [memory-dir]     (default: this project's store)
set -uo pipefail

dir="${1:-$HOME/.claude/projects/$(pwd | tr '/' '-')/memory}"
[ -d "$dir" ] || { echo "no memory store at $dir"; exit 2; }
index="$dir/MEMORY.md"
[ -f "$index" ] || { echo "error  MEMORY.md  the index is missing — every store has one"; exit 1; }

errors=0
infos=0
err() { echo "error  $1  $2"; errors=$((errors + 1)); }
inf() { echo "info   $1  $2"; infos=$((infos + 1)); }

names=""
for f in "$dir"/*.md; do
  base=$(basename "$f")
  [ "$base" = "MEMORY.md" ] && continue
  slug="${base%.md}"
  names="$names $slug"

  head -1 "$f" | grep -q '^---$' || { err "$base" "no frontmatter — the first line must be ---"; continue; }
  fm=$(awk 'NR>1 && /^---$/ {exit} NR>1' "$f")

  name=$(printf '%s\n' "$fm" | sed -n 's/^name: *//p' | head -1)
  [ -n "$name" ] || err "$base" "frontmatter has no name:"
  [ -z "$name" ] || [ "$name" = "$slug" ] || err "$base" "name: '$name' does not match the filename slug '$slug'"

  printf '%s\n' "$fm" | grep -q '^description:' || err "$base" "frontmatter has no description: — recall matches on it"

  type=$(printf '%s\n' "$fm" | sed -n 's/^ *type: *//p' | head -1)
  case "$type" in
    user | feedback | project | reference) ;;
    "") err "$base" "frontmatter has no metadata.type" ;;
    *) err "$base" "type '$type' is outside user|feedback|project|reference" ;;
  esac

  grep -q "($base)" "$index" || err "$base" "no pointer line in MEMORY.md — an unindexed memory is never recalled"

  [ "$type" = "feedback" ] && ! grep -q '^\*\*Why:\*\*' "$f" &&
    inf "$base" "feedback without a **Why:** line — the reason is what makes it applicable"

  grep -qiE '\b(yesterday|today|tomorrow|last (week|night|month)|next (week|month)|this morning)\b' "$f" &&
    inf "$base" "carries a relative date — memories are read months later, so dates go absolute"
done

# Index pointers that resolve to nothing.
grep -oE '\]\([a-z0-9-]+\.md\)' "$index" | tr -d '](){}' | while read -r target; do
  [ -f "$dir/$target" ] || echo "error  MEMORY.md  pointer to $target, which does not exist"
done | tee /dev/stderr | grep -c '^error' >/dev/null

# Wikilinks with no memory behind them. Not a defect — the instructions say a
# dangling link marks something worth writing — but the list is worth seeing.
dangling=$(grep -ohE '\[\[[a-z0-9-]+\]\]' "$dir"/*.md 2>/dev/null | tr -d '[]' | sort -u |
  while read -r l; do case " $names " in *" $l "*) ;; *) echo "$l" ;; esac; done)
[ -z "$dangling" ] || inf "wikilinks" "no memory yet behind: $(echo "$dangling" | tr '\n' ' ')"

# MEMORY.md is an index. Content living there is content nothing can recall.
awk 'NR>3 && !/^-/ && !/^#/ && !/^$/ && !/^\*\*/ {n++} END {exit !(n>12)}' "$index" &&
  inf "MEMORY.md" "carries prose beyond its pointer lines — index holds pointers, memories hold content"

echo
echo "$errors error, $infos info — $dir"
[ "$errors" -eq 0 ]
