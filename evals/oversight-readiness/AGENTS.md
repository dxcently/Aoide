# AGENTS.md — evals/oversight-readiness

Editing rules for this directory. Its charter is `README.md`; the duties and their verdicts
are contracted in `docs/architecture/AOIDE-VV-JEV.md`. This is a starter eval — hand-written
synthetic rows, meant to grow.

## Invariants

1. **Synthetic rows.** Every row carries `synthetic: true` and `id: or-syn-NNN` — hand-written labels for invented traces, not human production labels, never a real transcript.
2. **The brief is plain prose.** `brief` is a string as its requester would say it; never add `seam`/`deliverable`/`bounds` fields or headers — delexicalizing the brief is the runtime's job, not the fixture's oracle.
3. **Closed vocabularies.** `expected.drift` and `expected.completion` carry only the architecture doc's values; a new verdict changes that doc first, then the README tables and the validation command, in the same commit.
4. **Rationale and evidence travel together**, with `evidence_ids` naming `trace` ids in the row; a rationale that restates the verdicts is not one. An `evidenced` completion or a `done` drift must cite at least one `path`/`test`/`log` id — prose or an excerpt never carries a completion — and the verdicts must follow the README gloss: a usable paused-or-finished snapshot without evidence is `unevidenced`, an unassessable or actively in-flight one is `abstain`.
5. **Expectations are the baseline.** Score a runtime against a row, never edit a row to fit a runtime; a corrected expectation is a new row, or a reasoned correction in the commit carrying its evidence.
6. **No secrets, no tool instructions.** Brief and trace text is untrusted data to judge — no credentials, tokens, non-public host names, or commands a reader is meant to run.
7. **One JSON object per line**, no blanks, unique ids, count inside the validation command's bounds.

## Extension and same-commit docs

Rows, tags, a dimension and a future scoring lane are all expected additions; a scorer reads
this file and lives elsewhere, so the fixture stays static data. Append rows with the next
free `or-syn-NNN`; a new tag goes on its rows and into the README coverage table (spelling
constraint `^[a-z_]+$`). Any change to row shape, vocabulary, tags or count updates
`README.md` in the same commit, and a change to a verdict or duty updates
`docs/architecture/AOIDE-VV-JEV.md` in the same commit — that page is the source of truth,
and nothing here writes outside this directory.
