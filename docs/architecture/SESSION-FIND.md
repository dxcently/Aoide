# Session find — every session on every node, and what each one carries

> **Status: the bridge (B1-B5) is built.** `aoide session find` runs with its
> query grammar, related reasons, index and scope. The conductor's `/` page and
> its Find action (T1, T2) are not built, and bare `session <filter>` keeps its
> own matcher until the later slice in Decision 3.

The User's ruling: find agent sessions across every agent, and for specific
ones, and see what each carries — its primitives, roles, mailboxes and cc's.
`aoide session find <query>` answers that from a bare terminal; the
conductor's `/` page renders the same answer and holds no logic of its own.
Find returns facts and the reason each one matched. It never routes, sends,
rings, pulls or writes.

"Across all agents and for specific ones" reads two ways, and both are in
scope: every harness (claude, kimi, codex threads, eidolon, pi, shells) or one
(`agent:kimi`); and every session, or one session's full carries plus the
sessions related to it (`--related <ref>`).

## The shape

```
 inputs, read-only, nothing fetched
 ──────────────────────────────────
 state/stage/sessions.json + hooks.json ─┐
 state/stage/projects.json ──────────────┤
 state/stage/remote-children.json ───────┤  load_index()      FindIndex
 state/node-cache/<node>.json (each) ────┼─────────────────▶  rows · scope · mail facts
 state/mail/cursors.json ────────────────┤  (all the I/O)     · repo keys
 state/mail/base.jsonl ──────────────────┘                        │
                                                                  │ pure, no I/O
                ┌─────────────────────────────┬───────────────────┴──────────┐
                ▼                             ▼                              ▼
          find(query)                   carries(row)                  related(row, query)
          ranked rows + `matched`       the field map below, filled   rows + reasons
                │                             │                              │
                └──────────────┬──────────────┴──────────────┬───────────────┘
                               ▼                             ▼
             aoide session find … [--json]          conductor `/` page
             CLI and MCP doors, audited             in-process library call,
                                                    nothing dispatched per keystroke
```

House rule 7's test: delete every conductor file and `aoide session find`
still answers everything on this page. The code lives in
`aoide-conduct::graph::find`, beside `who.rs`, whose `SessionView` and
`NodeView` it reuses rather than redefines, and registers as `session find` in
`conduct/src/commands/graph.rs`.

## What a session carries

Each of the User's words maps onto facts the stores already hold. *Local* is
this box's stage; *cached remote* is what a far node published in its own
`graph.json` and `aoide node pull` last wrote to `state/node-cache/<name>.json`.
Cost: **free** — already loaded by the roster (`collect_roster`'s inputs);
**file** — one more small file; **scan** — one pass over `base.jsonl`.

### Primitives

| Carries | Local fact | Cached remote | Cost | Notes |
|---|---|---|---|---|
| agent (harness) | `SessionRecord.agent` | `agent` | free | a label the caller chose (`--agent pi -- bash`); the wrapped program itself is recorded only as the `shell` bool |
| model | `model` | `model` | free | absent for shells and before the first turn |
| kind | `kind`, absent → `agent != "shell"` (the backfill rule) | the `role` key (the document's rename) | free | agent · shell · subagent · a2a · app |
| state | `state` merged with the `hooks.json` phase (`merged_sessions`) | `state`, presence `last-seen`/`unknown` (`cached_presence`) | free | a cached row reads "was \<state\>" |
| node | which file the row came from — the stage (`display::local_host_name`) or `node-cache/<name>.json` | — | free | not a record field, and needs none |
| cwd | `cwd` | `cwd` | free | |
| project | the bucket bare `aoide session` files the row under: effective project (`effective_project_for`), else stored `project`, else a `.aoide/project.json` basename (`project_bucket`) | stored `project` (the roster's remote rule; cross-host inheritance is S-D's) | free | a box with no registered projects (osaka in the review's captures) buckets by manifest only |
| title, petname | `title`, `petname` | `title`, `petname` | free | `title` is text another session sent — untrusted |

### Roles

| Carries | Local fact | Cached remote | Cost | Notes |
|---|---|---|---|---|
| task | `task`, the managed run's slug | `task`, read back only as a legal slug of at most 64 characters (Decision 9) | free | a value that is no slug reads as absent |
| native role | `nativeRole` | `nativeRole` | free | the harness's own label (codex `subagent`, `guardian_review`) |
| lead | `projects.json` `lead` (`leads_project`) | the project node's `lead` | free | "lead of aoide" |
| assigned role | **not recorded anywhere** | — | — | TASK-REGISTER §15/§21: `mail role` (ML3) and the published `assignedRole`/`roleMailbox` (ML4) are designed and unbuilt. Find adds no role store; once ML4 publishes the binding, `role:` and `mailbox:` read it with no grammar change |

### Mailboxes

| Carries | Local fact | Cached remote | Cost | Notes |
|---|---|---|---|---|
| reads | every name in `cursors.json` whose reader map holds the session's mail identity: its own id when it is a conducted wrap, else its nearest conducted ancestor (`doorbell::conducted_ancestor`, the doorbell's own join) | **not published** | file | inside a wrap `AOIDE_SESSION_ID` is the wrap's id, so a hooked claude reads through its wrap; the pseudo-reader (key == name) is nobody; a key left by a dead wrap joins to no row |
| unread | `Mark.seq` against the entries filed to that name | — | scan | the count `aoide mail` prints |
| inbox | the task slug (`self/<task>`) and the petname the doorbell enrols on (MAIL.md, the petname fallback) | task (after B1), petname | free | a letter to a name is not proof any session read it (§24) |
| reports to | `reportTo`, else the parent id when it is a legal mailbox name, else `conductor` (the report lane's rule) | not published | free | task runs only |
| deliveries | `receipt` entries filed to the session's id (`file_receipt`) | — | scan | recorded; not shown in v1 |

### cc's and threads

| Carries | Local fact | Cost | Notes |
|---|---|---|---|
| cc'd on | every `letter` filed to a mailbox the session reads whose `AOIDE-LETTER/1` content (`letter::decode`) lists that address under Cc | scan | Cc is signed context, not delivery (MAIL.md); the copy filed under the mailbox is the fact |
| threads | the `threadId` of every structured letter filed to a mailbox it reads, To or Cc | scan | a legacy letter carries no thread |
| sent | **not attributed** | — | `from.name` is a petname claim, attribution only; §24 rules that an envelope naming only a name never infers a session, so find never maps a sender name to a session |

Mail facts are local. No node publishes its cursors, so a remote row's
mailboxes, cc's and threads read *not published by \<node\>*, never *none*.

### Edges

| Carries | Local fact | Cached remote | Cost |
|---|---|---|---|
| parent | `parentSessionId` (`resolved_parent`; a dangling id is no parent) | the `spawned` edge | free |
| children, siblings | its inverse; the same parent | the same edges | free |
| wrap | `conducted_ancestor` | — | free |
| remote parent | `remoteParent` | `remoteParent` | free |
| remote children | `remote-children.json`, joined to the cache by `(node, sessionId)` (`attach_subtrees`) | `remoteChildren` | free |
| repo | the git common dir of `cwd`: a `.git` directory, or a `.git` file's `gitdir:` followed to its `commondir` — so worktrees of one repo share it | — (a far path is not on this disk) | one stat walk per distinct cwd |

### What this design adds, and why

1. **`task` on `graph.json` session nodes** — additive v0, beside `petname`.
   Without it `role:` cannot see a far run. The slug is the run's mailbox
   name, so a node that publishes it sends a value its older document did not
   carry. Decision 9 says how a reader trusts it.
2. **A lock-free read of the mailbase** in `aoide-storage::mail`:
   `peek_cursors()` (the reader maps, pseudo-readers dropped) and `peek_base()`
   (entries, an incomplete final line ignored) — no lock, no migration, no
   write. The conductor's mail view already reads the base this way
   (`mailview::read_snapshot`); every other public read goes through the stage
   lock.
3. **`git::common_dir(path)`** in `aoide-storage` — a pure walk, no `git`
   process, for the repo key.

Three widenings, no copies: `doorbell::conducted_ancestor` becomes
`pub(in crate::graph)`; `who.rs` splits `cached_node(node, cache)` out of
`build_mesh_node`'s failed-probe arm (that arm becomes `cached_node` plus
`unreachable`); and the bucket rule leaves `group_by_project` for `project_of`,
so find and bare `session` can never file a row differently.

## Query syntax

```
aoide session find [<query>…] [--related <ref>] [--all] [--json]
```

A query is a list of words. On the command line each word is one shell
argument, quoted the way the shell quotes (`cc:"pr 41"` is one word, value
`pr 41`); the conductor's query line splits on spaces and honours `"…"`; both
reach the parser as the same list. A word whose text before the first `:` is a
key below is a term; every other word is a bare word. Every term and every bare
word must match (AND); inside one term, commas are alternatives (OR):
`state:working,awaiting`.

| Term | Matches when | How |
|---|---|---|
| *bare word* | petname, task, title, session id or a cwd path component holds it | graded below |
| `agent:` | the harness label | exact or prefix |
| `kind:` | agent · shell · subagent · a2a · app | exact or prefix |
| `state:` | the merged state (a cached row: its last-seen state) | exact or prefix |
| `node:` | the row's node; `self` is this box | exact or prefix |
| `project:` | the roster's project bucket | exact or prefix |
| `role:` | task slug, native role, or `lead` for a project lead | exact or prefix |
| `mailbox:` | a mailbox it reads, its task inbox, or its petname | exact or prefix |
| `cc:` | it was cc'd on a letter whose sender, To, subject or thread id holds the value | substring (thread id: prefix) |
| `parent:` | its parent, local or remote, is the session `<ref>` names | `<ref>` resolves like `send --to` |

Case folds on both sides (`str::to_lowercase`), except `parent:`, whose ref
goes verbatim to `storage::addr::resolve` (case-sensitive by its own contract;
an ambiguous ref is a taught error listing the candidates, never a first
match). An unknown key, an empty value or an unclosed quote is a taught error
naming the keys. `mailbox:` and `cc:` never match a remote row, and the scope
line says why.

A bare word grades against each field — **exact** (the whole field, or a whole
cwd component) > **prefix** > **substring** — and takes its best field's grade
(ties name the field petname > task > title > id > cwd in `matched`). Rows sort
by:

1. the weakest bare word's grade, then the sum of grades — only bare words
   rank; terms only filter
2. local rows before cached rows
3. state: awaiting · working · idle · stopped · done (the fold mark's urgency)
4. petname (else id), node, session id

No fuzzy match, no recency, no weights: the same files and the same query give
the same order, and the four keys explain every position — and an order with
no clock in it does not reshuffle while the User types. An empty query lists
every row in that order. `done` rows are left out unless `--all`.

```
aoide session find brave                       petname, task, title, id or cwd holding "brave"
aoide session find agent:kimi state:working    every working kimi on every node
aoide session find role:review node:yomi       review runs on yomi, from its cache
aoide session find mailbox:reviewer            who reads reviewer
aoide session find cc:"pr 41"                  who was cc'd about PR 41
aoide session find parent:warm-birch           everything warm-birch spawned, here and remote
aoide session find --related brave-otter       brave-otter's carries and its related sessions
```

## Scope

Find reads this box's stage and mail plus every registered node's cache, and
fetches nothing. Every answer ends with a scope line naming each registered
node and where its rows came from:

```
scope: osaka stage · yomi-strix cache 2h14m (stale) · sakaki never pulled
```

`fresh`/`stale` is `node_store::is_cache_fresh` (the five-minute TTL) and the
age is the cache's `fetchedAt`. A node with no matching row is still listed, so
an absence is never silent. The refresh is the command that already exists,
`aoide node pull [<name>]`; find has no `--live`. This is the undying picker's
posture (`grant.rs`: cached graphs, no probe), not bare `session`'s, which
probes every node live on every call.

## Related sessions

`--related <ref>` resolves one session (like `send --to`, `node/<rest>` for a
cached row) and returns every other row that shares a fact with it, each with
every reason it has. Reasons are derived per call and never stored: they are
not `graph.json` edges and not §22's typed edges.

| Order | Reason | Holds when | Shown as |
|---|---|---|---|
| 1 | lineage | parent or child; the wrap that hosts it, or the agent it hosts | `parent` · `child` · `parent (its wrap)` |
| 2 | remote lineage | `remoteParent` / `remoteChildren` | `remote parent` · `remote child` |
| 3 | sibling | the same local parent | `sibling` |
| 4 | same role | the same task slug or native role; both lead a project | `same role: review-pr` |
| 5 | mailbox | both read M; or one's `reportTo` is M and the other reads M | `same mailbox: reviewer` · `reads its reports: conductor` |
| 6 | thread | both read mailboxes holding letters of one thread, To or Cc | `shared thread: "PR 41 review" (3f2a…)` |
| 7 | same project | the same roster bucket | `same project: aoide` |
| 8 | same repo | the same git common dir (local rows) | `same repo: aoide` |

A related row sorts by its strongest reason, then by how many distinct reasons
it has, then by find's keys 2–4. A query beside `--related` narrows the related
rows (`--related brave-otter agent:claude`). The target never relates to
itself, `done` rows need `--all`, and the mail reasons (5, 6) hold only between
local rows.

## Output

The human form is one line per row, then the scope line:

```
$ aoide session find role:review
♪ brave-otter (…7d23)  osaka       claude  working   review-pr  ~/worktrees/aoide-pr-41
· quiet-fern (…a1c0)   yomi-strix  kimi    was idle  review-pr  ~/aoide
2 of 37 · scope: osaka stage · yomi-strix cache 2h14m (stale) · sakaki never pulled
```

```
$ aoide session find --related brave-otter
brave-otter (…7d23) · osaka · stage
  agent      claude · claude-opus-4-8 · agent · working ▸ Bash: cargo test
  where      ~/worktrees/aoide-pr-41 · project — · repo aoide
  role       task review-pr
  mailboxes  reads reviewer (2 unread), conductor — via wrap warm-birch
             inbox review-pr · rings as brave-otter · reports to conductor
  cc         3 letters on 2 threads · newest "PR 41 review" from osaka/builder
  edges      parent warm-birch · 2 children · 1 sibling · ↓ 1 remote
related (4)
  warm-birch (…19be)   osaka       parent (its wrap)
  quiet-fern (…a1c0)   yomi-strix  remote child · same role: review-pr      cache 2h14m
  fond-aspen (…0c11)   osaka       same mailbox: reviewer · same repo: aoide
  lone-heron (…77aa)   osaka       shared thread: "PR 41 review" (3f2a…)
scope: osaka stage · yomi-strix cache 2h14m (stale) · sakaki never pulled
```

The glyphs and words are the roster's (`who::glyph`, `↑ node/id`, `↓ n
remote`, the "was" of a cached row), not new ones. `--json` carries the same
facts in `data`:

```
{ "query":    { "words": [...], "terms": [{ "key": "role", "values": ["review"] }] },
  "scope":    [{ "node", "source": "stage" | "cache" | "never-pulled", "fetchedAt"?, "fresh"? }],
  "sessions": [{ "node", "sessionId", "label", "source", "state", "presence",
                 "carries": {
                   "agent", "model", "kind", "cwd", "project", "title", "petname",
                   "role":      { "task", "nativeRole", "leads": [] },
                   "mailboxes": { "reads": [{ "name", "unread", "via" }], "inbox": [], "reportTo" },
                   "cc":        [{ "msgid", "threadId", "subject", "from", "to": [] }],
                   "threads":   [],
                   "edges":     { "parent", "children", "siblings", "wrap",
                                  "remoteParent", "remoteChildren", "repo" } },
                 "unknown": ["mailboxes", "cc", "threads"],
                 "matched": [{ "term", "field", "value", "how": "exact" | "prefix" | "substring" }] }],
  "related":  [{ "node", "sessionId", "reasons": [{ "kind", "detail" }] }] }
```

`unknown` rides on remote rows; `related` rides with `--related`. An absent
key, a key named in `unknown`, and an empty list are three different answers:
not recorded, not published to this box, recorded as none. Titles, subjects,
`say` and sender names are text other sessions and nodes wrote: the human form
passes them through `clean_line`, `--json` carries the exact bytes, and a
consumer treats them as data (house rule 4). The human cc line names the newest
letter and counts the rest; `--json` carries them all.

Find writes nothing, takes no lock, never reaches the daemon and never dials a
node — `session trace`'s posture. A row is a fact, never a recommendation:
find picks no recipient, scores no fitness, and offers no action.

## Conductor: the `/` page

`/` opens Find from any panel outside text entry (no other handler binds it).
Find is a page, not an eleventh tab: like Home it takes the whole body and the
project tree folds away; number keys and the nav leave it. The page draws from
the library's index and never dispatches while the User types.

Two columns, when the column is at least 100 cells wide:

```
┌ FIND ────────────────────────────────────────────────────────────────────────────────────┐
│ / role:review▏                                                               2 of 37     │
│ osaka stage · yomi-strix cache 2h14m (stale) · sakaki never pulled                       │
│ ── results ─────────────────────────── │ ── carries ──────────────────────────────────── │
│ ▶ ♪ ♜ brave-otter  osaka  working      │ brave-otter (…7d23) · osaka · stage             │
│   · ♜ quiet-fern   yomi   was idle     │ agent  claude · opus-4-8 · working              │
│                                        │ where  ~/worktrees/aoide-pr-41 · repo aoide     │
│                                        │ role   task review-pr                           │
│                                        │ mail   reads reviewer (2), conductor · via wrap │
│                                        │ cc     3 letters · 2 threads                    │
│                                        │ edges  parent warm-birch · 2 children · ↓ 1     │
│                                        │ match  role:review → task review-pr (prefix)    │
│                                        │ ── related ──────────────────────────────────── │
│                                        │ warm-birch   parent (its wrap)                  │
│                                        │ quiet-fern   remote child · same role           │
│                                        │ fond-aspen   same mailbox: reviewer             │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ [/ query] [Tab pane] [Enter open] [e actions] [s letter] [r pull] [Esc]                  │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

Stacked, when narrower but at least 14 rows tall; one pane at a time below
that, with a pane strip naming which:

```
/ role:review▏                          2 of 37      / role:review▏          2 of 37
osaka stage · yomi-strix cache 2h (stale)            osaka · yomi 2h · sakaki —
── results ────────────────────────────────          [results]  carries  related
▶ ♪ ♜ brave-otter  osaka  working                    ▶ ♪ brave-otter  working
  · ♜ quiet-fern   yomi   was idle                     · quiet-fern   was idle
── carries ────────────────────────────────
brave-otter (…7d23) · osaka · claude · working
role task review-pr · mail reviewer (2), conductor
edges parent warm-birch · 2 children · ↓ 1 remote
── related ────────────────────────────────
warm-birch   parent (its wrap)
quiet-fern   remote child · same role
```

**Layout** — `findview::layout(inner) -> FindLayout`, the one function the
draw and the hit test both call:

- The block is centred across the body, at most 132 cells wide, and centred
  down it, at most 40 rows tall — the start page's rule (`board::home_layout`):
  a large terminal shows a centred block, a small one fills.
- The block's height comes from the terminal, never from the result count, so
  typing never moves it.
- Mode by the block's inner size: width ≥ 100 → two columns (results 40%, at
  least 36 cells); else height ≥ 14 → stacked (results ≥ 3 rows, carries 4–8,
  related ≥ 2); else one pane at a time.
- When space runs out, the decoration goes before a box clips, as on the start
  page: related first, then carries folds into the one-pane cycle, then the
  scope line shortens to node names and ages. The query line never drops.
- A row's columns drop right to left as width shrinks (cwd, task, agent,
  node); the state glyph, kind mark and petname stay. Widths are measured with
  `board::cells` and cut with `…`.

**Keys** — text entry first, as everywhere in the conductor:

| Key | In the query line | In results or related |
|---|---|---|
| `/` | text (`parent:yomi-strix/quiet-fern`) | back to the query line |
| typing | narrows live | — |
| ↑ ↓ | move the result selection | move |
| `j` `k` | text | move |
| Tab, Shift-Tab | next, previous pane — the page owns Tab while open, as the mail form owns its keys | next, previous pane |
| Enter | focus the results | a result: open or focus it, as Enter in Graph (a remote row opens its Mesh row); a related row: it becomes the query (its session id), so carries and related re-root on it |
| `e`, right-click | — | the existing actions menu for that target (`ContextTarget::Session` / `::Remote`) |
| `s` | — | write a letter through the existing path (a remote row needs a legal mailbox petname) |
| `r` | — | `node pull` through the audited dispatcher on the background path, then rebuild |
| Esc | text → clear it; empty → close Find | → the query line |
| `?` | text | help |

**State flow:**

```
`/`        App::find = FindState { query, focus, pane, selected (node, sessionId) }
           index: FindIndex, built on a worker by graph::find::load_index(),
           held behind an Rc on the UI thread,
           keyed by (len, mtime) of the stage files, cursors.json, base.jsonl
           and each node-cache file
keystroke  parse(split_line(query)) → find(&index, &q)   pure; memoised on
           (index generation, query)
poll tick  a key changed → rebuild on the worker; the last index stays drawn
           and the scope line says "reading"; selection kept by identity
`r`        dispatch `node pull` (audited) → rebuild
```

The selection is a session identity, never a row index — the Graph's own
invariant — so a rebuild cannot move it. The page never dispatches on a
keystroke, never writes, never probes, and offers no action the existing menu
does not. If TASK-REGISTER §23's capability shell lands first, Find is
`capabilities/find/` under that contract; otherwise `findview.rs` beside
`mailview.rs` and one `App::find` seam.

## Why the conductor goes unused, and what find changes

The review counted 6 `conductor` launches against 820 `session trace` and 450
`send` records in the audit log (CONDUCTOR-REVIEW.md). Its twelve fixes removed
the conductor's own defects — a blank first screen, a focus trap, a local-only
graph, cache drawn as live. They did not give it a question the shell cannot
answer, and the User conducts from the shell while agents conduct from the CLI.

| Find fixes | Find does not fix |
|---|---|
| A question neither door answers: who reads mailbox X, who was cc'd on this thread, which runs carry task Y, what else touches this session. Without find it is `cursors.json` read by hand, `aoide session`, and grep. | The conductor is still a place one decides to open. Find lands in the shell first, so its use shows up as `session find` records, not conductor launches. |
| `aoide session <filter>` matches id, petname and label only, case-sensitively, and probes every node live (in parallel, up to 2 s each). Find also matches title, task and cwd, folds case, and answers from disk. | Remote rows are as old as the last `aoide node pull`; nothing pulls on a timer. Find shows the age and does not make it fresh. |
| In the conductor, one session among many is `/`, a few letters, Enter — not a rank-by-rank walk. The related walk (Enter re-roots on a related row) is browsing the shell does badly. | On a box with no registered projects, `project:` and "same project" are empty; "same repo" covers part of that. |
| Agents gain the same answer over MCP (`session find --json`), before they address a letter. | Mail facts are sparse: a session carries a mailbox only once it has run `mail read` (or the doorbell enrolled it by petname), and no role is ever assigned until ML3/ML4 exist. For most sessions "carries" is primitives, task and edges. |
| | Find does not act. After finding, `aoide send` is still the shell's one line. |

It may not change conductor usage. The check: two weeks after T1 lands, count
`session find` and `conductor` records in the audit log. Find's success is the
first number.

## A natural-language front — later, never in this build

The User's ruling is the code filter first and VV, the small intent/slot
classifier, later as a front over it — a translator onto this grammar and
nothing more. It maps an
utterance onto the closed key set the registry already publishes — intent
`find` or `related`; slots `agent`, `kind`, `state`, `node`, `project`, `role`,
`mailbox`, `cc`, `parent`, plus leftover bare words — and prints the query it
produced (*who's reviewing PR 41 on yomi* → `role:review node:yomi 41`) for the
User to run or edit. The matcher never sees prose, the classifier never sees a
row, and a wrong slot is visible as a wrong term before anything runs. It
never runs the query itself: `aoide do`, the same classifier mapping a
sentence onto the whole command set and acting on it, scored 56% on a blind
probe and was retired (TASK-REGISTER §36), so the front proposes and the User
or agent runs. It is built only once `session find` is in use, and only if the
audit log shows the grammar is the obstacle. Nothing here reserves a hook,
trait, flag or config key for it.

## Slices

One commit per slice; `cargo test -p <crate>` only. B1–B5 are Haiku executors,
T1–T2 a Sonnet executor (frontend routing). The bridge is whole before the
conductor starts.

| Slice | Scope | Tests | Docs, same commit | After |
|---|---|---|---|---|
| **B1** graph task key | `doc.rs` publishes `task` on a session node when present; `sessions_from_graph` reads it into a new `SessionView.task` (a local row from the record) | a tasked record publishes `task`; an untasked node is byte-identical to before; a cached document's `task` reaches `SessionView` | CONTRACTS §4 `graph.json`; conduct README (`who.rs` seam) | — |
| **B2** storage reads | `mail::peek_cursors`, `mail::peek_base` (no lock, no migration, no write, a torn final line ignored); `git::common_dir` | the pseudo-reader is dropped; file bytes and mtime unchanged after a peek; a torn last line is skipped; a legacy flat `cursors.json` reads as unknown; a worktree's `.git` file and its main repo share one common dir; works with no `git` on `PATH` | storage README seams, storage AGENTS invariants | — |
| **B3** query core | `graph/find.rs`: the `FindIndex` row type, `split_line` (the conductor's line → words), `parse`, `find` — pure, over hand-built rows | the grammar (shell words and split lines agree, quotes, commas, unknown key, empty value, unclosed quote); each key; case folding; exact/prefix/substring grades; cwd components; the four sort keys; a cached row as "was"; `--all`; an ambiguous `parent:` is an error | conduct AGENTS (find writes nothing, never probes, never infers a session from a name) | — |
| **B4** carries and related | `carries`, `related`, and `mail_facts(rows, cursors, entries)` — pure | a hooked child reads through its wrap; a cc from a decoded letter; unread equals `aoide mail`'s count; reason order; one row, several reasons, deduplicated; a remote row's `unknown`; the target excluded | conduct README | B3 |
| **B5** the command | `load_index` (the `who.rs` extractions `cached_node` and `project_of`, the `conducted_ancestor` widening, the peeks, `common_dir`); `session find` appended to the registry plus the CLI golden; human and JSON renderers; `--related`, `--all`; the scope line | the handler over an isolated stage and state dir; a `PullFn` that panics proves nothing is probed; stage and mail mtimes unchanged; never-pulled and stale nodes listed; existing `who.rs` tests green (bare `session` groups exactly as before); renderers take `host` as an argument, so no test reads the box's hostname | Session-Graph.md management layer; wiki CLI-Reference and Graph-and-Conduct; this page's status for the bridge | B1, B2, B4 |
| **T1** the `/` page | `findview.rs` (or `capabilities/find/`), `App::find`, the layout, the keys, the worker-built index, `r` through `node pull` | mode and no clipping at 160×48 (two columns), 100×30 (stacked), 80×24 (one pane), 40×12, and 20×6 (the query line kept); draw and hit test agree; typing never calls `DispatchFn`; the Esc ladder; selection survives a rebuild by identity; Enter on a related row re-roots; Enter on a remote row opens its Mesh row | conductor README (keys, a Find section), AGENTS invariants, DESIGN.md flow; wiki Conductor-TUI.md; CONDUCTOR-REVIEW item 14 | B5 |
| **T2** "Find related" | one `ContextAction` on `Session` and `Remote` targets in Graph, Agents and Mesh, opening Find on that session | the menu lists it; it opens Find with that session's id as the query and its related list shown | conductor README | T1 |

## Decisions

The calls the design rests on. Each names the alternative it beat, what that
alternative would have given, and what the chosen call costs.

1. **`cc:` searches inside the letters.** It matches a session that was cc'd
   on a letter whose sender, To, subject or thread id holds the value. The
   alternative, "reads a mailbox that was ever cc'd", is simpler and cannot
   tell a PR 41 letter from any other letter in the same box. Cost: one decode
   per letter a session reads (`letter::decode`), the scan in the cc table.

2. **Mailbox readers stay on the node that holds them.** `cursors.json` is not
   published to other nodes. Publishing would put cursor keys on the wire to
   every read-granted node. ML4's published role binding is the designed
   carrier for that fact, so until it lands a remote row reads *not published
   by \<node\>* and its `mailbox:` and `cc:` terms never match it.

3. **One matcher, in a later slice.** Bare `session <filter>` folds into
   find's parser, and `apply_filter`'s substring fallback goes with it. The
   slice comes after B5 and is not one of the slices in the table below. B5
   extracts the bucket rule but leaves bare `session`'s filter alone, so until
   the later slice the two matchers differ: bare `session` is case-sensitive
   over id, petname and label, and find folds case over more fields. Gain: one
   grammar. Cost: the same word can match different rows in the two until then.

4. **No `--live`.** Find never probes a node. Fresh rows come from
   `aoide node pull [<name>]`, then find; the page's `r` runs the same pull.
   The alternative, `--live`, gives fresh rows but makes every find dial each
   node (up to 2 s each), bare `session`'s posture, and puts a network wait
   under a question typed one key at a time. Cost: an answer is only as fresh
   as the last pull, and the scope line names that age.

5. **"Same repo" is the git common dir.** Worktrees of one repo share it, so
   the User's worktree sessions relate although their cwds differ. The
   alternative, same cwd only, relates nothing across worktrees and nothing on
   a box with no registered projects. Cost: one stat walk per distinct cwd, and
   a far path has no repo, because the far disk is not read.

6. **No `--past` in v1.** An ended session belongs to History (§19's
   searchable History), not to find. The ledger carries no mail joins, so an
   ended row would show `mailboxes`, `cc` and `threads` all unknown. Cost: an
   ended session cannot be found from here.

7. **A cached row matches its last-seen state.** `state:` matches the state the
   cache last recorded, and the row reads "was \<state\>". The alternative,
   `state:` never matching a cached row, is stricter and leaves `state:working`
   blind to every far node. Cost: "was working" can be stale; the scope line's
   age is the only signal of that.

8. **`kind:` stays.** It is the commonest cut (agent, shell, subagent) and the
   one key beyond the ruling's list. The alternative, keeping the list exact,
   saves one key. Its cost: a shell or subagent cannot be separated from an
   agent by any term but `agent:` or `state:`.

9. **A far run's slug is published, and read back only when it is a slug.**
   A far node's `task` is kept when it is a legal mailbox name of at most 64
   characters, and dropped otherwise, not clipped: a clipped slug names a
   different mailbox. The alternative, keep the slug local, leaves `role:`
   blind to every far run. The alternative, publish and trust the value, lets
   a far node put control bytes into a human row. Cost: one more field on the
   wire, and a far node can still claim any legal slug, which find shows as
   that node's claim.

## How it sits with other work

- **TASK-REGISTER §15/§21 (roles).** Assigned roles and role mailboxes are
  ML3/ML4's. Find reads them once published and adds no role store.
- **§22 (orchestration graph, frozen).** Related reasons are per-call
  derivations, never edges. Typed edges, once they exist, become one more
  reason kind.
- **§23 (capability shell).** T1 fits either conductor structure.
- **§24 (provenance trails).** Find shares its rule — never infer a session
  from a name — and builds no trail.
- **CONDUCTOR-REVIEW.md** lists find as item 14.
