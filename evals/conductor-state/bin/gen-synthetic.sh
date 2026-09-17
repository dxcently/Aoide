#!/usr/bin/env bash
# Synthetic cases: sample the feature space with a seeded RNG, write each row
# through the SAME writer and rules as the harvest (bin/case.jq), drop
# duplicate lines. Says come from a small cue lexicon so the models see that
# the words after "says" carry signal; seed says are still mostly unknown to
# a word-vocab model — measured, not hidden.
#   bin/gen-synthetic.sh [N=6000] > cases/synthetic.jsonl
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
n=${1:-6000}
awk -v n="$n" 'BEGIN {
  srand(42)
  split("bash read edit write fetch peers send grep", TOOLS, " ")
  split("result assistant policy", KINDS, " ")
  SAYS[1] = ""; SAYS[2] = "let me read the remaining files first"; SAYS[3] = "now applying the edits to the module"
  SAYS[4] = "i will write the report now and stop"; SAYS[5] = "this looks misaddressed i have no brief here"
  SAYS[6] = "retrying the same command once more"; SAYS[7] = "tests pass the change is complete"
  SAYS[8] = "the command keeps failing with the same error"; SAYS[9] = "waiting for the user to confirm the overwrite"
  SAYS[10] = "done report written at the required path"; SAYS[11] = "acknowledged the correction continuing with the build"
  for (i = 0; i < n; i++) {
    ending = int(rand() * 10)          # 0..5 open, 6 settled, 7 cancelled, 8 spent, 9 ask
    turn_open = (ending <= 5 || ending == 9) ? "true" : "false"
    settled = (ending == 6) ? "true" : "false"; cancelled = (ending == 7) ? "true" : "false"
    spent = (ending == 8) ? "true" : "false"; ask = (ending == 9) ? "true" : "false"
    alive = (ending == 8) ? "false" : ((rand() < 0.15 && ending <= 5) ? "false" : "true")
    tool = TOOLS[1 + int(rand() * 8)]
    lk = (ending == 6) ? "settled" : (ending == 7) ? "cancelled" : (ending == 8) ? "spent" : (ending == 9) ? "ask" : (rand() < 0.08 ? "external" : (rand() < 0.05 ? "wrap-up" : KINDS[1 + int(rand() * 2)]))
    inflight = (lk == "assistant") ? "true" : "false"
    lerr = (lk == "result" && rand() < 0.25) ? "true" : "false"
    calls = int(rand() * 128)
    nudged = (calls >= 100 && rand() < 0.5) ? "true" : "false"; if (lk == "wrap-up") nudged = "true"
    steer = (lk == "external" || rand() < 0.1) ? "true" : "false"
    errs = int(rand() * 6) * (rand() < 0.5); reps = 1 + int(rand() * 4) * (rand() < 0.5)
    w = int(rand() * 6); r = int(rand() * 8)
    sil = (rand() < 0.3) ? "null" : int(rand() * 30)
    si = 1 + int(rand() * 11); say = SAYS[si]
    if (ending == 6 && rand() < 0.5) say = SAYS[7 + 3 * (rand() < 0.5)]
    printf "{\"id\":\"syn-%05d\",\"journal\":\"synthetic\",\"record\":%d,\"why\":\"synthetic\",\"node\":\"node-%04x\"", i, i, int(rand() * 65535)
    printf ",\"turn_open\":%s,\"alive\":%s,\"last_kind\":\"%s\",\"last_tool\":\"%s\",\"in_flight\":%s,\"last_result_error\":%s", turn_open, alive, lk, tool, inflight, lerr
    printf ",\"calls\":%d,\"calls_left\":null,\"budget_nudged\":%s,\"spent\":%s,\"settled\":%s,\"stop_reason\":null,\"cancelled\":%s,\"ask_open\":%s,\"steer_recent\":%s", calls, nudged, spent, settled, cancelled, ask, steer
    printf ",\"errors_recent\":%d,\"repeats_recent\":%d,\"writes_recent\":%d,\"reads_recent\":%d,\"silent_min\":%s,\"say\":\"%s\",\"tail\":null}\n", errs, reps, w, r, sil, say
  }
}' | jq -c -f "$here/case.jq" | awk -F'"line":"' '{ split($2, a, "\""); if (!seen[a[1]]++) print }'
