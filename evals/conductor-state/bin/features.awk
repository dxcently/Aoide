# eidolon journal (as printed by `eidolon log <path>.eid`) → one JSON feature
# row per checkpoint. Reads the text form today; the fields are the trace
# contract's (docs/architecture/EIDOLON-TRACE.md) so a `--json` export swaps in
# without changing the rows. Silence is null here: the text form has no ts_ms.
#
# usage: eidolon log <eid> | awk -v journal=<stem> -f bin/features.awk
BEGIN { n = 0; RING = 8; ERR_WIN = 6; SAMPLE_EVERY = 40 }

function jstr(s) { gsub(/\\/, "/", s); gsub(/"/, "'", s); gsub(/⏎/, " ", s); gsub(/[[:cntrl:]]/, " ", s); return "\"" s "\"" }
function reset_turn() {
    calls = 0; nudged = 0; calls_left = -1; spent = 0; settled = 0; stop = ""; cancelled = 0
    askopen = 0; steer_at = 0; say = ""; pending = 0; last_tool = ""; last_result_err = 0
    nres = 0; ntool = 0; err_emitted = 0; rep_emitted = 0
}
function recent_count(arr, cnt, win, want,   i, c, lo) { c = 0; lo = cnt - win + 1; if (lo < 1) lo = 1; for (i = lo; i <= cnt; i++) if (arr[i] == want) c++; return c }
function errors_recent(   i, c, lo) { c = 0; lo = nres - ERR_WIN + 1; if (lo < 1) lo = 1; for (i = lo; i <= nres; i++) c += rerr[i]; return c }
function repeats_recent(   i, lo, best, k, seen) {
    best = 0; lo = nres - RING + 1; if (lo < 1) lo = 1; delete seen
    for (i = lo; i <= nres; i++) { k = rpre[i]; if (k == "" || k ~ /^\[no output\]/ || k ~ /^edited /) continue; seen[k]++; if (seen[k] > best) best = seen[k] }
    return best
}
function tool_class(t) { return (t == "edit" || t == "write") ? "write" : "read" }
function writes_recent(   i, c, lo) { c = 0; lo = ntool - RING + 1; if (lo < 1) lo = 1; for (i = lo; i <= ntool; i++) if (tool_class(tools[i]) == "write") c++; return c }
function reads_recent(   i, c, lo) { c = 0; lo = ntool - RING + 1; if (lo < 1) lo = 1; for (i = lo; i <= ntool; i++) if (tool_class(tools[i]) == "read") c++; return c }
function tail_text(   i, lo, s, t) { s = ""; lo = n - 9; if (lo < 1) lo = 1; for (i = lo; i <= n; i++) { t = kinds[i] " " substr(texts[i], 1, 120); s = s (s == "" ? "" : " | ") t } return s }
function emit(why, alive,   ck, id) {
    id = sprintf("%s#%d", journal, ids[n]); if (emitted[id]++) id = id "~" why   # one id per record; a later checkpoint of the same record is named by its reason
    printf "{\"id\":\"%s\",\"journal\":\"%s\",\"record\":%d,\"why\":\"%s\",\"node\":\"eidolon-%s\"", id, journal, ids[n], why, substr(journal, length(journal) - 3)
    printf ",\"turn_open\":%s,\"alive\":%s,\"last_kind\":\"%s\",\"last_tool\":%s,\"in_flight\":%s", ((settled || cancelled) ? "false" : "true"), (alive ? "true" : "false"), kinds[n], ((last_tool == "") ? "null" : jstr(last_tool)), ((pending > 0) ? "true" : "false")
    printf ",\"last_result_error\":%s,\"calls\":%d,\"calls_left\":%s,\"budget_nudged\":%s,\"spent\":%s", (last_result_err ? "true" : "false"), calls, ((calls_left < 0) ? "null" : calls_left), (nudged ? "true" : "false"), (spent ? "true" : "false")
    printf ",\"settled\":%s,\"stop_reason\":%s,\"cancelled\":%s,\"ask_open\":%s,\"steer_recent\":%s", (settled ? "true" : "false"), ((stop == "") ? "null" : jstr(stop)), (cancelled ? "true" : "false"), (askopen ? "true" : "false"), ((steer_at > 0 && n - steer_at <= 4) ? "true" : "false")
    printf ",\"errors_recent\":%d,\"repeats_recent\":%d,\"writes_recent\":%d,\"reads_recent\":%d,\"silent_min\":null", errors_recent(), repeats_recent(), writes_recent(), reads_recent()
    printf ",\"say\":%s,\"tail\":%s}\n", jstr(substr(say, 1, 160)), jstr(tail_text())
}
/^#[0-9]+ ← / {
    n++; ids[n] = substr($1, 2) + 0; kind = $4; sub(/:$/, "", kind); kinds[n] = kind
    rest = $0; sub(/^#[0-9]+ ← [0-9-]+ [^ ]+:? ?/, "", rest); texts[n] = rest
    if (kind == "user") { reset_turn(); next }
    if (n == 1 || kind == "start") { reset_turn(); next }
    if (kind == "assistant") {
        calls++
        tl = ""; txt = rest
        if (match(rest, /\[[a-z_, ]+\]$/)) { tl = substr(rest, RSTART + 1, RLENGTH - 2); txt = substr(rest, 1, RSTART - 1) }
        sub(/[ \t]+$/, "", txt); if (txt != "") say = txt
        if (tl != "") { m = split(tl, parts, /, */); for (i = 1; i <= m; i++) { ntool++; tools[ntool] = parts[i]; last_tool = parts[i] } pending = m }
        if (steer_at > 0 && n - steer_at <= 2) emit("after-steer", 1)
        else if (calls % SAMPLE_EVERY == 0) emit("sample", 1)
        next
    }
    if (kind == "result") {
        if (pending > 0) pending--
        nres++; rerr[nres] = (rest ~ /^\[call_[^\]]*\] ERROR/ || rest ~ /\[exit code [1-9]/) ? 1 : 0
        pre = rest; sub(/^\[call_[^\]]*\]:? ?/, "", pre); rpre[nres] = substr(pre, 1, 60)
        last_result_err = rerr[nres]
        if (!err_emitted && errors_recent() >= 3) { err_emitted = 1; emit("error-streak", 1) }
        else if (!rep_emitted && repeats_recent() >= 3) { rep_emitted = 1; emit("repeats", 1) }
        next
    }
    if (kind == "wrap-up") { nudged = 1; if (match(rest, /[0-9]+ calls left/)) calls_left = substr(rest, RSTART, RLENGTH) + 0; emit("wrap-up", 1); next }
    if (kind == "spent") { spent = 1; emit("spent", 0); next }
    if (kind == "settled") { settled = 1; stop = $5; emit("settled", 1); next }
    if (kind == "cancelled") { cancelled = 1; emit("cancelled", 1); next }
    if (kind == "external") { if (settled || cancelled) reset_turn(); steer_at = n; emit("steer", 1); next }
    if (kind == "ask") { askopen = 1; emit("ask", 1); next }
    next
}
/^head:/ { if ($4 == "false" && n > 1 && kinds[n] != "spent" && !settled && !cancelled) emit(alive_now ? "open-now" : "unsettled-end", alive_now) }
