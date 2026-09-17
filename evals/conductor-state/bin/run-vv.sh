#!/usr/bin/env bash
# verba-volantia lane: train one checkpoint per field on the synthetic corpus,
# then dispatch every case line (synthetic test + seeds) through it.
#   bin/run-vv.sh [train]        → pred/vv.jsonl   {id, split, condition, decision, risk, *_prob, *_margin, *_accept, ms}
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here"); cd "$kit"
vv=${VV:-$HOME/verba-volantia/target/release/verba-volantia}
mkdir -p weights pred
if [ "${1:-}" = train ]; then
  for field in condition decision risk; do
    "$vv" train --data "corpus/$field" --out "weights/$field" --epochs "${EPOCHS:-30}" --batch 64 --seed 42 --target-acc 0.95 2>&1 | tail -6
  done
fi
cases=$(mktemp); jq -c '. + {split: "seed"}' cases/seed.jsonl > "$cases"
jq -c 'def h: (.line | explode | reduce .[] as $c (7; (. * 31 + $c) % 1000003)); select((h % 100) >= 85) | . + {split: "synthetic-test"}' cases/synthetic.jsonl >> "$cases"
n=$(wc -l < "$cases"); ms_total=0
for field in condition decision risk; do
  start=$(date +%s%N)
  jq -r .line "$cases" | "$vv" dispatch --out "weights/$field" > "pred/vv-$field.raw" 2>/dev/null
  ms=$(( ($(date +%s%N) - start) / 1000000 )); ms_total=$(( ms_total + ms ))
  echo "$field: $(wc -l < "pred/vv-$field.raw") lines in $ms ms"
done
paste -d'\n' <(jq -c '{id, split}' "$cases") pred/vv-condition.raw pred/vv-decision.raw pred/vv-risk.raw \
| jq -c -n --argjson ms "$(( ms_total / n ))" 'def rows: [inputs]; rows as $r | range(0; $r | length; 4) as $i
    | { id: $r[$i].id, split: $r[$i].split, runner: "vv", ms: $ms,
        condition: $r[$i+1].intent, condition_prob: $r[$i+1].intent_prob, condition_margin: $r[$i+1].margin, condition_accept: $r[$i+1].accept,
        decision: $r[$i+2].intent, decision_prob: $r[$i+2].intent_prob, decision_margin: $r[$i+2].margin, decision_accept: $r[$i+2].accept,
        risk: $r[$i+3].intent, risk_prob: $r[$i+3].intent_prob, risk_margin: $r[$i+3].margin, risk_accept: $r[$i+3].accept }' > pred/vv.jsonl
echo "pred/vv.jsonl: $(wc -l < pred/vv.jsonl) rows (expected $n)"; rm -f "$cases"
