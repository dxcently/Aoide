#!/usr/bin/env bash
# cases/seed.draft.jsonl + cases/seed.labels.tsv (id  condition  decision  note)
# → cases/seed.jsonl, only the rows a hand label exists for. The hand risk is
# the rule's risk function applied to the HAND decision (risk is derived from
# the decision plus the row's features, never labelled by hand).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here")
jq -c -L "$here" --rawfile tsv "$kit/cases/seed.labels.tsv" '
  include "lib";
  ($tsv | split("\n") | map(select(length > 0 and (startswith("#") | not)) | split("\t")) | map({key: .[0], value: {condition: .[1], decision: .[2], note: (.[3] // "")}}) | from_entries) as $h
  | select($h[.id] != null)
  | . + { hand: ($h[.id] + { risk: risk($h[.id].decision) }), hinge: ($h[.id].condition != .rule.condition or $h[.id].decision != .rule.decision) }
' "$kit/cases/seed.draft.jsonl"
