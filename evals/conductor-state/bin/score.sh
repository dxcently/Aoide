#!/usr/bin/env bash
# pred/*.jsonl + cases → report.md. Per runner and split: accuracy on each
# field, both-right, coverage (accepted / mean confidence), latency; the seed
# hinge rows (hand ≠ rule) get their own line — that is where judgment beats
# structure. Seed truth is the HAND label; synthetic truth is the rule.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here"); cd "$kit"
truth=$(mktemp)
jq -c '{id, split: "seed", hinge, truth: .hand, line, why}' cases/seed.jsonl > "$truth"
jq -c 'def h: (.line | explode | reduce .[] as $c (7; (. * 31 + $c) % 1000003)); select((h % 100) >= 85) | {id, split: "synthetic-test", hinge: false, truth: .rule, line, why}' cases/synthetic.jsonl >> "$truth"
{
  echo "# conductor-state kit — report"
  echo
  echo "Generated $(date -u +%Y-%m-%dT%H:%MZ) · seeds $(wc -l < cases/seed.jsonl) (hinge $(jq -s 'map(select(.hinge)) | length' cases/seed.jsonl)) · synthetic test $(jq -s 'length' <(grep -c . /dev/null; jq -c 'select(.split == "synthetic-test")' "$truth"))"
  echo
  echo "| runner | split | n | condition | decision | risk | all three | accepted | acc on accepted (cond) | mean prob (cond) | ms/case |"
  echo "|---|---|---|---|---|---|---|---|---|---|---|"
  for p in pred/*.jsonl; do
    [ -s "$p" ] || continue
    jq -r -n --slurpfile t "$truth" --slurpfile p "$p" '
      ($t | map({key: .id, value: .}) | from_entries) as $tt
      | [ ($p | unique_by(.id))[] | select($tt[.id] != null) | . as $r | $tt[.id] as $c
          | { runner: $r.runner, split: ($c.split + (if $c.hinge then " · hinge" else "" end)),
              c: ($r.condition == $c.truth.condition), d: ($r.decision == $c.truth.decision), k: ($r.risk == $c.truth.risk),
              acc: ($r.condition_accept // true), prob: ($r.condition_prob // 0), ms: ($r.ms // 0) } ]
      | (group_by([.runner, .split]) + (group_by(.runner) | map(map(select(.split | startswith("seed")) | .split = "seed · all"))))
      | .[] | select(length > 0)
      | { runner: .[0].runner, split: .[0].split, n: length,
          c: (map(select(.c)) | length), d: (map(select(.d)) | length), k: (map(select(.k)) | length),
          all: (map(select(.c and .d and .k)) | length), acc: (map(select(.acc)) | length),
          acc_c: ((map(select(.acc and .c)) | length) as $x | (map(select(.acc)) | length) as $y | if $y == 0 then null else $x / $y end),
          prob: (map(.prob) | add / length), ms: (map(.ms) | add / length) }
      | "| \(.runner) | \(.split) | \(.n) | \(.c * 100 / .n | round)% | \(.d * 100 / .n | round)% | \(.k * 100 / .n | round)% | \(.all * 100 / .n | round)% | \(.acc * 100 / .n | round)% | \(if .acc_c == null then "—" else "\(.acc_c * 100 | round)%" end) | \(.prob * 100 | round)% | \(.ms | round) |"'
  done
  echo
  echo "## Seed confusion (condition) — rows: hand, columns: predicted"
  for p in pred/*.jsonl; do
    [ -s "$p" ] || continue
    echo; echo "### $(jq -r '.runner' "$p" | head -1)"; echo
    jq -r -n --slurpfile t "$truth" --slurpfile p "$p" '
      ($t | map(select(.split == "seed")) | map({key: .id, value: .truth.condition}) | from_entries) as $tt
      | [ ($p | unique_by(.id))[] | select($tt[.id] != null) | "\($tt[.id]) → \(.condition)" ] | group_by(.) | map("\(.[0]) ×\(length)") | .[]' "$p" | sed 's/^/- /'
  done
  echo
  echo "## Seed misses, by runner (hand ≠ predicted; hinge rows marked ★)"
  for p in pred/*.jsonl; do
    [ -s "$p" ] || continue
    echo; echo "### $(jq -r '.runner' "$p" | head -1)"; echo
    jq -r -n --slurpfile t "$truth" --slurpfile p "$p" '
      ($t | map(select(.split == "seed")) | map({key: .id, value: .}) | from_entries) as $tt
      | ($p | unique_by(.id))[] | select($tt[.id] != null) | . as $r | $tt[.id] as $c
      | select($r.condition != $c.truth.condition or $r.decision != $c.truth.decision)
      | "- \(if $c.hinge then "★ " else "" end)`\(.id)` hand \($c.truth.condition)/\($c.truth.decision) · got \($r.condition)/\($r.decision) · `\($c.line[:110])`"' "$p"
  done
} > report.md
rm -f "$truth"; echo "report.md written"; sed -n '1,30p' report.md
