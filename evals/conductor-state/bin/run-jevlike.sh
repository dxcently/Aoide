#!/usr/bin/env bash
# jevlike lane: one option-attention scorer per field over the same lines.
#   bin/run-jevlike.sh tiny [train]   byte encoder from scratch (jevlike's default)
#   bin/run-jevlike.sh hf   [train]   frozen Qwen/Qwen2.5-0.5B encoder + the same head (the "qwen+jev" shape)
# → pred/jevlike-<enc>.jsonl. Runs inside the kit's python devshell:
#   nix develop path:$PWD -c bin/run-jevlike.sh tiny train
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here"); cd "$kit"
export OMP_NUM_THREADS=${OMP_NUM_THREADS:-16}
enc=${1:-tiny}; export JEVLIKE=${JEVLIKE:-$HOME/jevlike}; export PYTHONPATH=$JEVLIKE
mkdir -p runs pred
if [ "${2:-}" = train ]; then
  for field in condition decision risk; do
    train=corpus/jevlike/$field-train.jsonl   # TRAIN_ROWS=N bounds an expensive encoder to the first N rows
    if [ -n "${TRAIN_ROWS:-}" ]; then head -n "$TRAIN_ROWS" "$train" > "runs/jevlike-$enc-$field-train.jsonl"; train=runs/jevlike-$enc-$field-train.jsonl; fi
    python3 -m jevlike.train "$train" --validation "corpus/jevlike/$field-dev.jsonl" \
      --output "runs/jevlike-$enc-$field.pt" --encoder "$enc" --epochs "${EPOCHS:-8}" --device cpu \
      $([ "$enc" = hf ] && echo "--hf-model Qwen/Qwen2.5-0.5B --rank 256") 2>&1 | tail -3
  done
fi
for field in condition decision risk; do
  { cat "corpus/jevlike/$field-seed.jsonl" | jq -c '. + {split: "seed"}'; jq -c '. + {split: "synthetic-test"}' "corpus/jevlike/$field-test.jsonl"; } > "pred/jevlike-$enc-$field.in"
  python3 "$here/run-jevlike.py" "runs/jevlike-$enc-$field.pt" "pred/jevlike-$enc-$field.in" > "pred/jevlike-$enc-$field.raw"
done
echo "pred/jevlike-$enc.jsonl: $(wc -l < pred/jevlike-$enc.jsonl) rows"
jq -c -n --arg r "jevlike-$enc" --slurpfile s pred/jevlike-$enc-condition.in --slurpfile c pred/jevlike-$enc-condition.raw \
  --slurpfile d pred/jevlike-$enc-decision.raw --slurpfile k pred/jevlike-$enc-risk.raw '
  ($d | map({key: .id, value: .}) | from_entries) as $dd | ($k | map({key: .id, value: .}) | from_entries) as $kk
  | range(0; $c | length) as $i | $c[$i].id as $id
  | { id: $id, split: $s[$i].split, runner: $r,
      condition: $c[$i].pred, condition_prob: $c[$i].prob, condition_margin: $c[$i].margin,
      decision: $dd[$id].pred, decision_prob: $dd[$id].prob, decision_margin: $dd[$id].margin,
      risk: $kk[$id].pred, risk_prob: $kk[$id].prob, risk_margin: $kk[$id].margin,
      ms: ($c[$i].ms + $dd[$id].ms + $kk[$id].ms) }' > "pred/jevlike-$enc.jsonl"
echo "pred/jevlike-$enc.jsonl: $(wc -l < pred/jevlike-$enc.jsonl) rows"
