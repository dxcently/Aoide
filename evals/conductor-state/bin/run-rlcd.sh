#!/usr/bin/env bash
# RLCD lane over the seeds (both contexts) and the synthetic test (line only).
#   nix develop path:$PWD -c bin/run-rlcd.sh      → pred/rlcd-line.jsonl, pred/rlcd-tail.jsonl
set -euo pipefail
export OMP_NUM_THREADS=${OMP_NUM_THREADS:-16}
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here"); cd "$kit"; mkdir -p pred
{ jq -c -n --argjson n "${RLCD_SEEDS:-999}" '[inputs] | sort_by(.hinge | not) | .[:$n] | .[] | . + {split: "seed"}' cases/seed.jsonl
  jq -c 'def h: (.line | explode | reduce .[] as $c (7; (. * 31 + $c) % 1000003)); select((h % 100) >= 85) | . + {split: "synthetic-test"}' cases/synthetic.jsonl | jq -c -n --argjson n "${RLCD_SYNTH:-60}" '[inputs] | .[:$n] | .[]'
} > pred/rlcd.in
python3 "$here/run-rlcd.py" pred/rlcd.in line > pred/rlcd-line.jsonl
jq -c 'select(.split == "seed")' pred/rlcd.in > pred/rlcd-tail.in
python3 "$here/run-rlcd.py" pred/rlcd-tail.in tail > pred/rlcd-tail.jsonl
wc -l pred/rlcd-line.jsonl pred/rlcd-tail.jsonl
