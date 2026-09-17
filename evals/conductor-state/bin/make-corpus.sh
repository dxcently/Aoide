#!/usr/bin/env bash
# cases → what each runner trains or reads on.
#   corpus/<field>/{train,dev,test}/{seq.in,seq.out,label} + label files   (verba-volantia, ATIS layout)
#   corpus/jevlike/<field>-{train,dev,test,seed}.jsonl                      (jevlike: context/options/label)
#   bin/make-corpus.sh seed   → only corpus/jevlike/<field>-seed.jsonl (after relabelling)
# Synthetic rows split 70/15/15 by a stable hash of the line; seeds are never
# trained on — they are the only number that means anything.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); kit=$(dirname "$here"); cd "$kit"
if [ "${1:-}" = seed ]; then   # `make-corpus.sh seed`: relabelled seeds only, the training corpus untouched
  for field in condition decision risk; do
    opts=$(tail -n +2 "corpus/$field/intent_label.txt" | jq -R . | jq -sc .)
    jq -c --arg f "$field" --argjson opts "$opts" '(.hand[$f]) as $lab | {id, context: .line, options: $opts, label: ($opts | index([$lab]))}' cases/seed.jsonl > "corpus/jevlike/$field-seed.jsonl"
    echo "$field: $(wc -l < corpus/jevlike/$field-seed.jsonl) seed"
  done; exit 0
fi
rm -rf corpus; mkdir -p corpus/jevlike
for field in condition decision risk; do
  for split in train dev test; do mkdir -p "corpus/$field/$split"; done
  jq -r --arg f "$field" '.rule[$f]' cases/synthetic.jsonl | sort -u | { echo UNK; cat; } > "corpus/$field/intent_label.txt"
  printf 'PAD\nUNK\nO\nB-node\n' > "corpus/$field/slot_label.txt"
  jq -r --arg f "$field" '
    def h: (.line | explode | reduce .[] as $c (7; (. * 31 + $c) % 1000003));
    (h % 100) as $b | (if $b < 70 then "train" elif $b < 85 then "dev" else "test" end) as $split
    | (.line | split(" ") | map(if . == "$a" then "B-node" else "O" end) | join(" ")) as $tags
    | "\($split)\t\(.line)\t\($tags)\t\(.rule[$f])"' cases/synthetic.jsonl \
  | awk -F'\t' -v d="corpus/$field" '{ print $2 > (d "/" $1 "/seq.in"); print $3 > (d "/" $1 "/seq.out"); print $4 > (d "/" $1 "/label") }'
  opts=$(tail -n +2 "corpus/$field/intent_label.txt" | jq -R . | jq -sc .)
  for split in train dev test; do
    jq -c --arg f "$field" --argjson opts "$opts" --arg split "$split" '
      def h: (.line | explode | reduce .[] as $c (7; (. * 31 + $c) % 1000003));
      (h % 100) as $b | (if $b < 70 then "train" elif $b < 85 then "dev" else "test" end) as $s
      | select($s == $split) | (.rule[$f]) as $lab | {id, context: .line, options: $opts, label: ($opts | index([$lab]))}' cases/synthetic.jsonl > "corpus/jevlike/$field-$split.jsonl"
  done
  jq -c --arg f "$field" --argjson opts "$opts" '(.hand[$f]) as $lab | {id, context: .line, options: $opts, label: ($opts | index([$lab]))}' cases/seed.jsonl > "corpus/jevlike/$field-seed.jsonl"
done
for field in condition decision risk; do echo "$field: $(wc -l < corpus/$field/train/label) train · $(wc -l < corpus/$field/dev/label) dev · $(wc -l < corpus/$field/test/label) test · $(wc -l < corpus/jevlike/$field-seed.jsonl) seed"; done
