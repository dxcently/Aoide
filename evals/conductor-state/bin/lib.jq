# The one writer and the rules. Words only — a digit or punctuation would be
# lifted as a value by verba-volantia's delexicalizer, so every magnitude is
# quantized into a word. Included by case.jq and merge-labels.sh (jq -L bin).
def bucket(n; lo; hi; a; b; c): if n == null then null elif n < lo then a elif n < hi then b else c end;
def phr_turn:
  if .spent then "turn spent" elif .settled then "turn settled" elif .cancelled then "turn cancelled"
  elif .ask_open then "question open" else "turn open" end;
def phr_last:
  if .last_kind == "external" then "steer arrived"
  elif .last_kind == "wrap-up" then "nudge arrived"
  elif .last_tool == null then (if .say == "" then "just started" else "last said only" end)
  elif .in_flight then "\(.last_tool) in flight"
  elif .last_result_error then "last \(.last_tool) failed"
  else "last \(.last_tool) ok" end;
def phr_tempo:
  if .writes_recent >= 3 then "writing" elif .writes_recent > 0 then "mixed"
  elif .reads_recent > 0 then "reading" else "quiet" end;
def phr_errors: bucket(.errors_recent; 1; 3; "no errors"; "some errors"; "many errors");
def phr_repeats: bucket(.repeats_recent; 2; 3; "no repeats"; "some repeats"; "looping");
def phr_budget:
  if .spent then "budget spent" elif .budget_nudged then "budget nudged"
  elif .calls >= 100 then "budget low" else "budget fresh" end;
def phr_silence: (bucket(.silent_min; 3; 10; "silent briefly"; "silent a while"; "silent long")) // "silence unknown";
def phr_process: if .alive then "alive" else "gone" end;
def say_cue: (.say // "") | ascii_downcase | gsub("[^a-z ]"; " ") | gsub(" +"; " ") | split(" ") | map(select(length > 0)) | .[:10] | join(" ");
def delex:
  say_cue as $cue
  | (["$a", phr_turn, phr_last, phr_tempo, phr_errors, phr_repeats, phr_budget, phr_silence, phr_process] | join(" "))
  | if $cue == "" then . else . + " says " + $cue end;
def misrouted_say: (.say // "") | ascii_downcase | test("misaddressed|no brief|not for me|wrong session|no such (brief|build step)|nothing to do with");
def condition:
  if .spent then "exhausted" elif .settled then "settled" elif .cancelled then "cancelled"
  elif .ask_open then "blocked" elif (.alive | not) then "dead"
  elif misrouted_say then "misrouted"
  elif .budget_nudged then "wrapping-up"
  elif .repeats_recent >= 3 then "looping"
  elif .errors_recent >= 3 then "failing"
  elif (.silent_min // 0) >= 10 then "stalled"
  else "progressing" end;
def decision(c):
  if c == "progressing" or c == "wrapping-up" then "wait"
  elif c == "stalled" then "nudge"
  elif c == "looping" then "steer"
  elif c == "failing" then (if .steer_recent then "escalate" else "steer" end)
  elif c == "blocked" then "answer"
  elif c == "dead" or c == "exhausted" then "resume"
  elif c == "misrouted" then "cancel"
  else "collect" end;
# Risk: which property of the CIA triad the decision puts at stake, with its
# DAD counterpart (disclosure · alteration · destruction/denial) as the harm
# if the decision is wrong. The gate reads this, not the decision, to decide
# what a human must see.
def risk(d):
  if d == "cancel" or d == "resume" then "availability"
  elif d == "steer" or .writes_recent > 0 then "integrity"
  elif d == "collect" or d == "answer" or (.last_tool | IN("send", "peers", "fetch")) then "confidentiality"
  else "none" end;
