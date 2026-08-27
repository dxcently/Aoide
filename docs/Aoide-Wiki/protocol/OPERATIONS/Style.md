# Style

How a page reads. [[Assertion]] governs what a sentence may claim; this page
governs the prose that carries the claim. Every write to a content page —
ingest, refactor, review sweep, a one-line fix — binds to it, whichever agent
holds the pen.

The rule exists because generated prose drifts toward a recognizable filler
register: symmetric lists, contrast rhetoric, hedged intensifiers, summaries
of what was just said. Each instance costs little; accumulated across sweeps
they bury the facts agents come here for. The wiki is read by agents under
token budgets — every sentence that carries no fact is paid for on every
future read.

## The register

Dense, technical, present indicative. Prose is connective tissue between
facts; the facts are file paths, command spellings, type names, wire fields,
exit codes, numbers. A section earns depth by being architectural, not by
being long.

Tests, in order:

1. **Every sentence carries a fact an agent needs.** A sentence that only
   frames, transitions, or reassures is deleted, not improved.
2. **Specificity over vocabulary.** Name the file, the command, the field,
   the value. `stage_dir()` resolves to `state/stage/` beats "state now
   lives in a dedicated location".
3. **One claim, one reason.** A claim gets at most one justification. A
   second reason is padding or belongs on the page that owns it.
4. **Between two phrasings that both hold the facts, the shorter ships.**

## Banned constructions

Each of these is a mechanical tell; the fix is stated with it.

- **Throat-clearing and meta-commentary** — openers that announce the page
  or section instead of starting it (`It's worth noting`, `In this section`,
  `Let's explore`, `To understand X, we must first`). Delete; start at the
  first fact.
- **Contrast rhetoric** — `not just X but Y`, `more than a X`, `X isn't
  simply Y`. State what the thing is ([[Assertion]] clause 2 bans the
  contrast with a road not taken; this bans the rhetorical shape even when
  both halves are true).
- **Symmetric lists** — three-plus bullets sharing one grammatical skeleton
  (adjective-noun, no verb: `robust tracking · seamless handoff · flexible
  routing`). Rewrite each item as a claim with a verb and a specific, or
  fold the list into a sentence. A table of enumerable facts is fine; a
  rhythm of slogans is not.
- **Rule-of-three runs** — triplets deployed for cadence (`fast, simple,
  and reliable`). Keep the member that is true and load-bearing.
- **Empty intensifiers and hedges** — `very`, `deeply`, `truly`, `robust`,
  `seamless`, `comprehensive`, `powerful`, `elegant`, `simply`, `arguably`,
  `essentially`. Delete the modifier; if the sentence loses meaning, it had
  none.
- **Marketing vocabulary** — `leverage`, `delve`, `unleash`, `empower`,
  `streamline`, `supercharge`, `elevate`, `journey`, `landscape`,
  `ecosystem` (except as a technical term of art with a referent).
- **Generic profundity** — a closing sentence that widens to significance
  (`This is the foundation for everything that follows`). End on the last
  fact.
- **Restating summaries** — a paragraph or "In summary" block repeating the
  section above it. The section is its own summary.
- **False tension** — dramatizing routine mechanics (`But there's a catch`,
  `Here's where it gets interesting`). The mechanics are the interest.

## What stays

Depth is not slop. An architectural section keeps every load-bearing
paragraph; a subtle invariant keeps its full statement; a warning keeps its
teeth. The measure is fact density, never length. Voice is permitted where
it is doing work — a memorable one-line rule (`delete every .qml — is the
capability still reachable?`) survives precisely because it compresses a
test, not decorates one.

## Scope

Binds every content page and every protocol page. `ingest/log.md` entries
are working notes and bind only to the banned-constructions list, not the
register tests — a log entry may narrate.

Changing this page is a [[Self-Update]].

## Related

- [[Assertion]] — what a sentence may claim
- [[Lint]] — the style-violation check
- [[Ingest]]
- [[Self-Update]]
