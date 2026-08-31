---
name: memory-handoff
description: Carry Claude's memory across to the kimi agent on this box. Lints the Claude store, folds in what this session learned, then rewrites kimi's own instruction file from it. Use when kimi is about to run a lane, after a session that changed standing preferences, or when asked to sync/transfer/hand off memory between agents.
---

# Memory handoff — Claude → kimi

Two stores, different shapes. The handoff is a **projection**, never a copy.

| | Claude | kimi |
|---|---|---|
| store | `~/.claude/projects/<slug>/memory/` — one file per fact + `MEMORY.md` index | `~/.kimi-code/AGENTS.md` — one instruction file |
| slug | the project path with `/` → `-` (`/home/khoa/Aoide` → `-home-khoa-Aoide`) | — |
| holds | facts, deltas, per-agent quirks, live lane state | voice, standing constraints, where to read |
| read | at session start, by recall | at every session start, whole |

kimi has no per-fact store and no recall. Everything that crosses is read by it
in full, every session, so what crosses is chosen, not dumped.

## 1. Lint before anything

```
.claude/skills/memory-handoff/lint.sh            # this project's store
.claude/skills/memory-handoff/lint.sh <dir>      # another one
```

Report-only, never repairs — the same discipline `aoide soundcheck` holds for
the working tree. It exits non-zero on `error`, zero on `info` alone.

`error` means the store is structurally broken and recall will silently miss:
a memory with no index pointer is never recalled, a `name:` that disagrees with
its filename breaks `[[wikilinks]]`, a type outside the four is unroutable.
Fix every one before exporting.

`info` is judgement. A relative date is a memory that will lie in three months.
A feedback memory with no **Why:** is a rule nobody can apply to a case it does
not literally name. A dangling wikilink is not a defect — it marks a memory
worth writing — but a long list of them usually means facts moved somewhere
else and the links were never re-pointed.

## 2. Update Claude's own memory first

Exporting a stale store propagates staleness into a second agent, where it is
harder to notice. Before the transfer:

- Fold this session's deltas in — what was decided, what was corrected, what
  turned out to be wrong. Convert relative dates to absolute as you go.
- Delete what the session disproved. A memory that is now false is worse than
  a missing one.
- Check the repo did not absorb it. Anything now stated in `AGENTS.md`,
  `CONTRACTS.md`, or `docs/Aoide-Wiki/protocol/dev/` belongs there, not in
  memory — memory holds only what the repo cannot record.
- Re-run the lint. It should be clean of errors.

## 3. Rewrite kimi's file

Back it up first — every effect gets an inverse:

```
cp ~/.kimi-code/AGENTS.md ~/.kimi-code/AGENTS.md.bak-$(date +%F)
```

Then rewrite the file whole. Keep its existing section shape (Voice · Thinking ·
Answering · Building · Asking · About User · Aoide); edit integrally so it reads
as though it was always that way, never as an appended changelog.

**What crosses, rendered as instructions:**

- `~/.claude/CLAUDE.md` — the voice and working style. Verbatim in substance;
  it is the same operator behind both agents.
- `type: user` and `type: feedback` memories that are harness-agnostic —
  distilled to the rule and its why, not pasted.
- `type: reference` — as pointers, never as inlined content.

**What does NOT cross, and points instead:**

- `type: project` — lane state, task stacks, handoffs. It changes hourly; a
  copy in a second file is a staleness schedule, not a handoff. Name the path
  and let kimi read it.
- Anything the repo already states. kimi's file already routes to root
  `AGENTS.md` and the dev protocol set; restating a rule there is a second
  authority for one fact.
- Claude-harness quirks — subagent parking, dispatch-tool behaviour, model
  routing. `docs/Aoide-Wiki/protocol/dev/HARNESS-KIMI.md` is kimi's own page
  and the only harness page it needs.

**The test before you write a line into kimi's file:** would kimi act
differently for having read it? If not, it is costing context every session
for nothing.

## 4. Verify

The voice half is mechanical — it must come back identical:

```
diff -B <(sed -n '/^# Rook/,$p' ~/.claude/CLAUDE.md) \
     <(sed -n '/^# Rook/,/^## Aoide/p' ~/.kimi-code/AGENTS.md | head -n -1)
```

The rest is judgement: read the file back against the store you exported from,
not against your memory of what you wrote. Then say plainly what crossed, what you
deliberately left behind, and what you could not verify.
