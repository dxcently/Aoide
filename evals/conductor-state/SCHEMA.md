# Case schema

One case is one checkpoint of one node: what a conductor could see at that
instant, and what it should conclude. Three files carry it end to end —
`bin/features.awk` reads the journal, `bin/case.jq` (definitions in `bin/lib.jq`, shared with `merge-labels.sh`) writes the line and the
rule labels, `cases/seed.labels.tsv` holds the human answer.

## Fields

| field | from | meaning |
|---|---|---|
| `id` | harvest | `<journal>#<record>` for seeds (`…~<why>` when a later checkpoint lands on the same record), `syn-NNNNN` for synthetic |
| `why` | harvest | why this record is a checkpoint: `settled` · `cancelled` · `wrap-up` · `spent` · `steer` · `after-steer` · `error-streak` · `repeats` · `sample` (every 40th call) · `unsettled-end` (journal ends with the turn open, process gone) · `open-now` (same, process alive) |
| `node` | harvest | the node the line is about; the only value the model ever sees is its placeholder `$a` |
| features | harvest | `turn_open` `alive` `last_kind` `last_tool` `in_flight` `last_result_error` `calls` `calls_left` `budget_nudged` `spent` `settled` `stop_reason` `cancelled` `ask_open` `steer_recent` `errors_recent` (of the last 6 results) `repeats_recent` (max identical result prefix among the last 8, edits and empty output excluded) `writes_recent` / `reads_recent` (of the last 8 calls) `silent_min` (null from the text log; the trace's `ts_ms` fills it) `say` (last assistant text) `tail` (last ten records, for text models only) |
| `line` | case.jq | the delexicalized state line, words only — see grammar |
| `bind` | case.jq | `{"$a": node}` — the bind map travels beside the line, never through the model |
| `rule` | case.jq | `{condition, decision, risk}` from the rules below — the oracle |
| `hand` | labels.tsv | `{condition, decision, risk}` a human wrote (seeds only; `risk` is copied from the rule until someone disagrees) |
| `hinge` | merge | `hand ≠ rule` — the rows where judgment beat structure |

## The line

Nine phrases in a fixed order, then the say cue. A phrase is chosen by
quantizing a feature into a word: a digit would be lifted as a value by the
delexicalizer, so no magnitude survives as a number.

```
$a  <turn>  <last>  <tempo>  <errors>  <repeats>  <budget>  <silence>  <process>  [says <first ten words of the last say>]

turn      turn open | turn settled | turn cancelled | turn spent | question open
last      last <tool> ok | last <tool> failed | <tool> in flight | last said only | steer arrived | nudge arrived | just started
tempo     writing (≥3 writes of last 8) | mixed | reading | quiet
errors    no errors (0 of last 6) | some errors (1-2) | many errors (≥3)
repeats   no repeats | some repeats (2) | looping (≥3)
budget    budget fresh | budget low (≥100 calls) | budget nudged | budget spent
silence   silent briefly (<3 min) | silent a while (<10) | silent long | silence unknown
process   alive | gone
```

Example, a real one:

```
$a turn open nudge arrived mixed no errors no repeats budget nudged silence unknown alive says now let me verify the mode resolve path and the
```

## Labels

**condition** — what is going on (first rule that matches wins):

| condition | rule |
|---|---|
| exhausted | `spent` |
| settled | `settled` |
| cancelled | `cancelled` |
| blocked | `ask_open` |
| dead | turn open and not `alive` |
| misrouted | the say reads like "misaddressed", "no brief", "not for me", "wrong session" |
| wrapping-up | `budget_nudged` |
| looping | `repeats_recent ≥ 3` |
| failing | `errors_recent ≥ 3` |
| stalled | `silent_min ≥ 10` |
| progressing | otherwise |

**decision** — what the conductor does next: `wait` (progressing, wrapping-up) ·
`nudge` (stalled) · `steer` (looping, failing) · `escalate` (failing after a
steer) · `answer` (blocked) · `resume` (dead, exhausted) · `cancel` (misrouted)
· `collect` (settled, cancelled).

**risk** — which leg of the CIA triad the decision puts at stake, with the DAD
counterpart as the harm if the decision is wrong. The gate reads this field,
not the decision, to decide what a human must see before anything happens.

| risk | decisions / signals | DAD harm |
|---|---|---|
| availability | cancel, resume | denial / destruction — a turn stopped or restarted that should not have been |
| integrity | steer, or the node has written lately | alteration — a course or a file changed on a wrong reading |
| confidentiality | collect, answer, or the last tool was send / peers / fetch | disclosure — output or a question's answer leaves the node |
| none | wait, nudge with nothing written | — |

## Splits

- `cases/synthetic.jsonl` — sampled feature space through the same writer and
  rules; 70/15/15 train/dev/test by a stable hash of the line; this is what
  the trained runners learn from. Its test split is the floor: a runner that
  cannot recover the rule from the line has a pipeline problem.
- `cases/seed.jsonl` — real journals, hand labels, never trained on. The only
  number that means anything; its `hinge` rows are the reason a model is
  worth having over the rule.
