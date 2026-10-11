# AOIDE-VV-JEV — the oversight integration contract

> **Status: nothing in this page runs.** JEV is ruled and unbuilt: this is the contract
> and the artifact list that does not exist yet. No latency, memory, accuracy or budget
> number here is measured or agreed.

Oversight rides on a local classifier, and it is advisory: a classifier never executes
and never holds a credential — it selects from a closed set, writes a row, and `aoided`
decides what happens next.

**JEV** turns one plain-prose brief plus the child's trace into a drift verdict and a
completion-evidence verdict, upward only; its candidate runtime is a local
reimplementation that does not exist yet, torch being training/export only.

Ruled line: a model only where a rule cannot be written — prose → the brief row, the
ordering among many claims. Everything else stays code: ring/wake/skip/defer, the
awaiting guard, escalation chains, mail batching, tool-path → seam, the ping-back's own
line selection. The test is that rule's operational form: ~99% on rule-labelled rows
means ship the rule.

## JEV — brief oversight

**Two duties, closed verdicts.** *Drift* — is the work inside the brief? The ruling fixed
four names: `on_goal` · `drifting` · `blocked` · `done`; `abstain` is added here.
*Completion evidence* — does the requested result exist? `evidenced` · `partial` ·
`unevidenced` · `contradicted` · `abstain`. Drift `abstain` and the whole completion set are
**this page's evaluation vocabulary**, a candidate for the runtime rather than a ruling. The
two duties can disagree on one trace. `unevidenced` means a usable paused-or-finished
snapshot carries no evidence of the requested result, whether or not it claims success;
`abstain` means the snapshot cannot be assessed at all — contradictory, unreadable, or still
actively in flight.

**The brief row.** The brief enters **as plain prose** and is delexicalized into `{seam,
deliverable, bounds}`, with no structured header required of its author. `seam` and the
enumerated bound flags are closed sets selected over candidates; `deliverable` is free
text and stays out of any scoring head. Showing the row back to its author — on the spawn
ledger and in the first ping-back line — is the **ruled design intent**, so a misread stays
visible; neither the ledger nor the line exists yet (see *The row's home is not agreed*).

**Upward only, never a correction.** Drift verdicts are shared upward through the existing
ping-back: never corrections or instructions sent down, never triggering an action, never
carrying a credential. The ping-back's own `choose()` table remains the authority on
delivery, and `done` and `blocked` are already rule-derivable there, so `drifting` plus
the completion-evidence read are the whole model justification.

**The row's home is not agreed.** Its designed but uncontracted home is register §22,
which is contract-stage: **this page creates no ledger contract**, and promises no
`briefed:` literal — the delivery path exists, the line does not.

**Runtime.** The ruled candidate is candle + rune (backbone via candle, head as a candle
module, readouts as rune scripts), and it **does not exist**: no candle option-attention
module, no rune dependency in core, and no export path out of the torch experiment, so
today's weights cannot load in it. Torch is training and export only, never a node
runtime; upstream JEV is hosted, so "JEV" here means Aoide's own reimplementation.

## Measurement

**No performance claim is available**, and the honest statement is narrower than
"unbenchmarked": a small scoring head does not make a large backbone cheap, so no cheap
CPU numbers may be implied. CPU-only operation is a required acceptance target, not a
fallback. The consumer eval hardware/OS matrix is **Linux and native Windows**, **macOS
later**. Aoide core must not require WSL; native Windows runtime compatibility and CPU
measurements remain unverified.

Before any number is quoted, record together, on that hardware: model and backbone identity
plus **parameter count and on-disk weight size** (and whether the backbone is frozen);
quantization and thread/context settings; **the bounded input** — the bytes and tokens a
brief plus its trace may occupy, with the maximum actually scored; cold and warm p50/p95
latency; peak resident memory; and false-completion and abstention rates on representative
oversight inputs.

**The numerical acceptance budgets are undecided.** No default SLO, latency or memory
ceiling is set here — a fabricated target makes an unmeasured number look agreed.

## The fixture and what remains

`evals/oversight-readiness/` holds 16 hand-written synthetic rows: each one a **plain
prose brief** plus a trace and both expected verdicts with a rationale and evidence ids.
It is a starter eval, not permanent architecture — rows, tags and a future scoring lane may
be added beside it.

**Unbuilt:** the closed option sets; the extraction labels (prose → `{seam, deliverable,
bounds}`) and drift-only rows a lane could train on — distinct from the 16 synthetic
verdict rows that do exist; the rule baseline; a torch export path; the candle port and
rune readouts; a home for the row, and any drift-verdict input to `choose()`.

**Unresolved, user-owned, no recommendation made here:** JEV lane order (offline
rule-vs-model probe first, or the candle port first); the parked mid-turn ping-back
ruling; the acceptance budgets.
