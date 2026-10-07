# AOIDE-VV-JEV — the oversight integration contract

> **Status: the VV lane runs; the JEV lane does not.** `aoide do` and its kit generator
> exist; no kit is trained or adopted yet, and nothing here is measured. JEV is ruled and
> unbuilt. No latency, memory, accuracy or budget number here is measured or agreed.

Two oversight duties ride on local classifiers, and both are advisory: a classifier
never executes and never holds a credential — it selects from a closed set, writes a
row, and `aoided` decides what happens next.

## The split

**VV** turns one free-text utterance into one command id from a closed set plus its
argument slots; core **shells out** to the `verba-volantia` binary and links no runtime.
**JEV** turns one plain-prose brief plus the child's trace into a drift verdict and a
completion-evidence verdict, upward only; its candidate runtime is a local
reimplementation that does not exist yet, torch being training/export only.

Ruled line: a model only where a rule cannot be written — free text → a closed command
set, prose → the brief row, the ordering among many claims. Everything else stays code:
ring/wake/skip/defer, the awaiting guard, escalation chains, mail batching, tool-path →
seam, the ping-back's own line selection. The kit's test is that rule's operational
form: ~99% on rule-labelled rows means ship the rule.

## VV — `aoide do`

`aoide do <sentence>` prints the one `aoide` command the sentence means, and never runs
it: a model suggests, the person decides. Words after `do` need no quotes. The line it
prints is full and copy-pasteable, built the way the registry's own refusals build theirs,
and checked by the registry's `Command::check` before it is shown, so it parses. In a
terminal, text mode prints the bare line on stdout (`$(aoide do "…")` substitutes it) and
every refusal on stderr with nothing on stdout; `--json` keeps the envelope, with the verdict
under `data.verdict`. `aoide do kit` emits the kit's training spec (see *The command kit*).
`aoide schema --json` lists both.

**Shell-out, never linkage.** Core shells out to the `verba-volantia` binary — no VV crates,
no candle, no vendored weights. One runtime per box serves many kits, each kit owning a
weights directory; Melete's client already has that shape, and Aoide names the two parts the
same way: the binary is `verba-volantia` found on `PATH`, overridden by config
`[verba] binary`, and the kit is a directory, `[verba] weightsDir`, default
`$AOIDE_ROOT/verba/aoide`. The kit holds `meta.json` and `model.safetensors` (and an optional
`lexicon.txt`); both keys are `aoide config set`-able and written only when an operator sets
them.

**The wire.** `verba-volantia dispatch --out <kit>` reads one utterance per line on stdin and
prints one JSON verdict per line; `aoide do` sends one line and reads the first line of
stdout that is a JSON object, bounded to 20 seconds and 1 MiB, and only when the child exited successfully. The fields core reads are `intent`,
`margin`, `threshold`, `accept`, `candidates[]` (`{intent, score}`, best first), `slots{}`,
`conflicts[]` and `trailing_editorial_text`; `intent_prob`, `delex`, `utterance`, `risk` and
`route` ride along unread. `accept`, `margin` and `threshold` belong to the checkpoint. The
wire contract is the doc comment on `dispatch()` in VV's `src/main.rs`.

**Binding is literal, outside the model.** The classifier is asked *which command*, never
*with what values*: in raw-text mode VV delexicalizes the utterance itself, so the model sees
placeholders and a value cannot be read as intent, then returns each slot as the verbatim
literal it lifted. Core binds each returned slot to the command's argument or flag of that
name, and those literals become the command's **actual arguments** — the guarantee covers the
intent, not the command line, which is exactly where the literal must go. A bare value that
looks like a dictionary word is not lifted unless the kit's `lexicon.txt` lists it; the
lexicon is the kit's per-host lever, not something core generates.

**What prevents dispatch.** Fail-closed; ignoring any row below deletes the guard. Every
row ends in a taught refusal (what, why, fix) with exit 1.

| condition | behavior |
| --- | --- |
| `accept` false or null | abstain: the nearest commands print as full lines with their scores, nothing dispatches |
| `conflicts` non-empty | **do not dispatch** — the verdict contradicts itself |
| `trailing_editorial_text` non-null | **do not dispatch** — part of the utterance is unaddressed, so the intent is not trustworthy |
| intent `none` | not an aoide command; `aoide guide` is the fix |
| an intent or slot the registry does not hold | a kit bug, never a command |
| a required slot unfilled, or a value the registry refuses | the command prints with the slot named (`aoide session trace <id>`), refused, so nothing half-bound reads as done |
| a bound command whose printed line does not parse back to the checked invocation (a value such as `--follow` or `--yes` that reads as a flag) | refused, nothing printed: the line is re-parsed by the door and must equal what the registry checked |
| a slot value holding a control character or a line break | refused, nothing printed |
| an intent in the denied set (see *The command kit*) | not a command; never taught, never resolved |
| no binary, no kit, timeout, spawn failure, a non-zero exit (even after a verdict line) | one refusal naming the missing piece and where it goes |
| unparsable line, or no verdict | one refusal |

**`aoided` remains the authority.** `aoide do` runs nothing, so it holds no new power: the
call itself is audited like any dispatch, and running the printed line is an ordinary
invocation through the one policy surface, one gate and one audit log (`$AOIDE_ROOT/log`).

**The command kit, in plain words.** A kit is four things, all data: **templates** — the
phrasings an utterance may take per command; **probe** — hand-written cases frozen before
training, so iterations compare against one yardstick; **weights** — one directory per kit
(checkpoint, metadata, lexicon); **tests** — template validation, the frozen probe, and the
token-savings check that gates any "VV saves tokens" claim. `aoide do kit` writes the first
from the registry, so it is derived and never invented: intents are command ids (the path
joined with `_`), slots are declared arguments and value-taking flags (bool flags are not
slots, so a printed command never carries `--yes` or `--submit`; add them by hand), and the
phrasings are read off each command's path, brief, summary and examples, plus a handful of
requests that are not commands as the `none` class. A phrasing two commands share is dropped
from both. The closed set is every implemented command a person can type — not hook plumbing,
not `do` itself, and not the denied set: every `secrets` command but `secrets status` (a
secret would land in the utterance, the shell history and the printed line), every `mesh
charter`, `mesh join`, `melete call`, and the irreversible removals (`session kill/prune/
reap`, `mail rm`, `mail outbox rm`, `node remove`, `project remove`, `workspace clear`). The
set is one const in `vv/kit.rs`; a denied intent is neither taught nor resolved, so a
classifier that answers one is refused as a kit bug.

```
aoide do kit --out templates.json
verba-volantia gen --spec templates.json --data data/aoide
verba-volantia train --data data/aoide --out weights/aoide --seed 1 --epochs 60 --batch 256 --bucket-window 16 --smooth 0.1
mkdir -p $AOIDE_ROOT/verba/aoide && cp -r weights/aoide/. $AOIDE_ROOT/verba/aoide
```

Training is VV's own procedure (its `AGENTS.md`): on a GPU build run by its own path, with
`device: cuda:0` as the first line of output; the same recipe on the CPU build ran 34 s an
epoch against 1.7 s on VV's own mneme corpus. The generated spec is the floor, not a tuned corpus: VV's template
validator is where hand-written phrasings enter a spec it owns, and the frozen probe is what
says whether a kit may be adopted.

**Which commands are in the surface** follows the registry, so adding a command changes the
spec on the next `aoide do kit` and a kit trained before it answers `none` or abstains on the
new command until retrained; a kit that names a command the registry no longer holds is
refused by name.

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

**Runtime.** The ruled candidate is candle + rune, mirroring VV (backbone via candle, head
as a candle module, readouts as rune scripts), and it **does not exist**: no candle
option-attention module, no rune dependency in core, and no export path out of the torch
experiment, so today's weights cannot load in it. Torch is training and export only, never
a node runtime; upstream JEV is hosted, so "JEV" here means Aoide's own reimplementation.

## Measurement

**No performance claim is available**, and the honest statement is narrower than
"unbenchmarked": a small scoring head does not make a large backbone cheap, so no cheap
CPU numbers may be implied for either lane. CPU-only operation is a required acceptance
target, not a fallback. The consumer eval hardware/OS matrix is **Linux and native
Windows**, **macOS later**. Aoide core must not require WSL; native Windows runtime
compatibility and CPU measurements remain unverified.

Before any number is quoted, record together, on that hardware: model and backbone identity
plus **parameter count and on-disk weight size** (and whether the backbone is frozen);
quantization and thread/context settings; **the bounded input** — the bytes and tokens a
brief plus its trace may occupy, with the maximum actually scored; cold and warm p50/p95
latency; peak resident memory; and false-completion and abstention rates on representative
oversight inputs.

**The numerical acceptance budgets are undecided.** No default SLO, latency or memory
ceiling is set here — a fabricated target makes an unmeasured number look agreed. The VV
lane inherits the rule, and `aoide do` (free text → a closed command set) has never been
measured against a keyword/registry rule; that baseline is a prerequisite.

## The fixture and what remains

`evals/oversight-readiness/` holds 16 hand-written synthetic rows: each one a **plain
prose brief** plus a trace and both expected verdicts with a rationale and evidence ids.
It is a starter eval, not permanent architecture — rows, tags and a future scoring lane may
be added beside it.

**Unbuilt — VV:** the trained and adopted kit; the frozen probe; the token-savings row; the
rule baseline. **Unbuilt — JEV:** the closed
option sets; the extraction labels (prose → `{seam, deliverable, bounds}`) and drift-only rows
a lane could train on — distinct from the 16 synthetic verdict rows that do exist; the rule
baseline; a torch export path; the candle port and rune readouts; a home for the row, and any
drift-verdict input to `choose()`.

**Unresolved, user-owned, no recommendation made here:** whether the closed surface for
`aoide do` is narrowed below every typeable command; JEV lane order (offline rule-vs-model probe first, or the candle port first); the parked
mid-turn ping-back ruling; the acceptance budgets.
