#!/usr/bin/env bash
# Real journals → draft cases (features + line + rule labels, hand labels
# empty). Hand labels live in cases/seed.labels.tsv; `bin/merge-labels.sh`
# joins them into cases/seed.jsonl. Liveness is read the way Aoide reads it:
# a presence meta.json naming this journal whose pid is alive.
#   bin/harvest.sh ~/.local/share/eidolon/sessions/*.eid > cases/seed.draft.jsonl
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
run=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/eidolon
for eid in "$@"; do
  stem=$(basename "$eid" .eid)
  alive=0
  for meta in "$run"/*/meta.json; do
    [ -f "$meta" ] || continue
    if jq -e --arg log "$(readlink -f "$eid")" '.log == $log' "$meta" >/dev/null 2>&1; then
      pid=$(jq -r .pid "$meta"); kill -0 "$pid" 2>/dev/null && alive=1
    fi
  done
  (eidolon log "$eid" 2>/dev/null || true) | awk -v journal="$stem" -v alive_now="$alive" -f "$here/features.awk" | jq -c -L "$here" -f "$here/case.jq"
done
