# conductor-state kit

Can a small model read the state of a node in Aoide's session graph and say
what the conductor should do next — and does it beat a rule, or a Qwen doing
the same thing in one forward pass? This kit is the bench: real eidolon
journals turned into checkpoints, a human answer per checkpoint, and four
runners scored the same way.

```
~/.local/share/eidolon/sessions/*.eid         real journals (40 of them, 4.8k records)
        │ bin/harvest.sh  (eidolon log → features.awk → case.jq, whose writer and rules live in lib.jq)
        ▼
cases/seed.draft.jsonl ──merge-labels.sh──▶ cases/seed.jsonl      ◀── cases/seed.labels.tsv  (hand labels)
cases/synthetic.jsonl   (gen-synthetic.sh: sampled feature space, same writer, rule labels)
        │ bin/make-corpus.sh
        ▼
corpus/{condition,decision,risk}/…   (verba-volantia, ATIS layout)      corpus/jevlike/*.jsonl
        │                                                                       │
   bin/run-vv.sh train             bin/run-jevlike.sh tiny|hf train        bin/run-rlcd.sh
        ▼                                   ▼                                   ▼
   pred/vv.jsonl                    pred/jevlike-*.jsonl                 pred/rlcd-{line,tail}.jsonl
        └──────────────────────── bin/score.sh ────────────────────────────────┘
                                        ▼
                                    report.md
```

Every runner answers three typed questions per case — `condition` (what is
going on), `decision` (what the conductor does), `risk` (which leg of the CIA
triad that decision puts at stake) — from the same delexicalized state line.
`SCHEMA.md` has the line grammar, the label vocabularies and the rules.

## Runners

| runner | what it is | sees | trained on |
|---|---|---|---|
| `vv` | verba-volantia joint intent+slot model, one checkpoint per field (`~/verba-volantia`, CPU build) | the line, `$a` bound outside | synthetic train; abstains below its dev-calibrated margin |
| `jevlike-tiny` | jevlike option-attention scorer, byte encoder from scratch (`~/jevlike`) | the line as context, the label vocabulary as options | synthetic train |
| `jevlike-hf` | same head over a frozen `Qwen/Qwen2.5-0.5B` encoder — the "qwen+jev" shape | same | synthetic train (head only) |
| `rlcd-line` | `Qwen/Qwen2.5-1.5B-Instruct`, no training, one prefill + every candidate of every field scored in one batched pass over the shared KV cache (the harshatheg/Qwen-2.5-1B-RLCD shape) | the line | — |
| `rlcd-tail` | the same, given the raw journal tail and the last say instead of the line — what only a text model can read | the tail | — |

## Run it

```sh
cd ~/Aoide/evals/conductor-state
nix develop path:$PWD                      # python (torch, transformers), jq, cargo

bin/harvest.sh ~/.local/share/eidolon/sessions/*.eid > cases/seed.draft.jsonl
bin/merge-labels.sh > cases/seed.jsonl     # joins cases/seed.labels.tsv; unlabeled drafts are dropped
bin/gen-synthetic.sh 8000 > cases/synthetic.jsonl
bin/make-corpus.sh                         # `make-corpus.sh seed` after relabelling: seed inputs only, the training corpus untouched

bin/run-vv.sh train                        # ~/verba-volantia/target/release/verba-volantia (VV=… to point elsewhere)
bin/run-jevlike.sh tiny train
bin/run-jevlike.sh hf train                # downloads Qwen2.5-0.5B on first use; EPOCHS=N, TRAIN_ROWS=N bound the CPU cost (a frozen 0.5B encoder re-encodes every epoch)
bin/run-rlcd.sh                            # downloads Qwen2.5-1.5B-Instruct on first use; RLCD_SEEDS=N (hinge rows first), RLCD_SYNTH=N bound the row count
bin/score.sh                               # → report.md
```

Labeling a new checkpoint: find its row in `cases/seed.draft.jsonl`, add a
line to `cases/seed.labels.tsv` (`id  condition  decision  note`), re-run
`merge-labels.sh`. A row whose hand label differs from the rule is a hinge
row and is reported separately — those are the cases that justify a model.

## Reading the report

- **synthetic-test** is the floor. The line is a closed grammar; a runner that
  misses here cannot recover the rule from the words.
- **seed** is the number. Real checkpoints, human answers, never trained on.
- **seed · hinge** is the question. The rule is wrong on these by
  construction; a runner that is right learned something the rule does not
  encode — or, for the Qwen lanes, read it off the words.
- `accepted` and `acc on accepted` are verba-volantia's abstention: below its
  margin threshold it declines instead of guessing. The other runners always
  answer; their `mean prob` is the only confidence they offer.

## What the seeds cannot show yet

No `silent_min` and no `blocked` rows: the text form of `eidolon log` carries
no timestamps and none of the real runs asked a question. Both fill in from
the trace file (`docs/architecture/EIDOLON-TRACE.md`) once it ships —
`features.awk` reads the text form today and swaps to the trace lines without
changing a field.
