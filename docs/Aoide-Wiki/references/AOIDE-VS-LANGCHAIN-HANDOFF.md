# Aoide vs. LangChain/LangGraph/LangSmith — positioning handoff

Status: a chat discussion (2026-08-03), not a design doc — nothing here is
decided or built. Captured so the reasoning isn't lost, not as a contract.
The graph-db idea in §3 and the DAG-to-KG shift in §4 are open proposals
khoa raised to *discuss*, explicitly not to build yet.

---

## 1. The comparison

Real overlap in *ambition* (coordinate multiple agents, observe what they
do), but the three LangChain-ecosystem products sit one layer down from
where Aoide operates.

|                           | LangChain                                          | LangGraph                                                     | LangSmith                                           | Aoide                                                                                                                                                    |
| ------------------------- | -------------------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **What it is**            | Base library for calling an LLM                    | In-process Python/JS graph library, built on LangChain        | Hosted SaaS tracing/eval dashboard                  | Rust CLI+daemon + a live desktop                                                                                                                         |
| **Unit of orchestration** | A chain (LLM call → tool → parse)                  | A graph node (a function/LLM call *you wrote*)                | A logged trace of a run                             | A real OS process (whatever's in that terminal)                                                                                                          |
| **Where the logic lives** | Your chain code                                    | An explicit graph authored ahead of time, baked into your app | N/A (observability only)                            | No predefined graph — a human or agent drives already-running sessions at runtime, ad hoc                                                                |
| **Agent-awareness**       | You write the LLM-calling code in their primitives | You write each node in LangGraph's framework                  | Agnostic to framework, reads traces                 | Fully agnostic — never looks inside the agent's reasoning, only its PTY session state (working/awaiting/idle/needsSudo, context-token fill)              |
| **Observability**         | —                                                  | Delegates to LangSmith                                        | Its whole job — dashboards, evals, dataset curation | Local audit log (`aoided`) + a desktop widget (dock/conductor), self-hosted, no cloud. `evals` crate is explicitly deferred — no mature eval harness yet |
| **Interop wire**          | Added native A2A support March 2026                | Same (built on LangChain)                                     | —                                                   | A2A since earlier in 2026 too — **not a differentiator**, both now speak the same open protocol                                                          |
| **Deployment shape**      | `pip install`, runs inside your app process        | Same                                                          | Point traces at their cloud                         | An entire NixOS distro or portable Nix flake — ships the terminal, the agents, the desktop, hooks, theming                                               |

**Stack picture** — these three are not competing layers with Aoide, they're
stacked *inside* it:

```
Aoide        — OS-level session/process control, agnostic to what's inside the terminal
LangSmith     ─┐
LangGraph      ├─ all three live INSIDE a process, calling LLMs
LangChain     ─┘
```

A LangChain-built agent (LangGraph-orchestrated, LangSmith-traced) running
in a terminal is exactly as conductable by Aoide as Claude Code, Kimi Code,
or `pi` — Aoide only ever sees the PTY session, never what library the
process used to talk to a model. That stack could run *inside* an
Aoide-tracked session today with zero conflict.

**Zero overlap with LangChain specifically.** Aoide's stated thesis: *"any
agent with a shell is fully capable — aoide wraps no LLM API."* No
chat-model wrapper, no prompt templates, no tool-calling abstraction, no
RAG/retriever glue. Not an oversight — a deliberate refusal.

---

## 2. Where this actually bites: the deferred `steward` crate

Phase 7 (`steward`, docs/architecture/PACKAGE-LAYOUT.md) is DEFER, blocked
first on khoa's unmade call: is `steward` LLM-driven itself (its own
harness/turn-loop), or a verb-surface an external agent drives via
`conduct`? Asked directly this session — answer was "not ready to decide
yet." The LangChain comparison forks on exactly that decision:

- **Verb-surface route** (tools/canon/audit driven externally) — zero
  LangChain overlap, same posture as the rest of Aoide today. No model
  client, period.
- **LLM-driven route** (`steward` gets its own `harness`) — walks straight
  into LangChain's territory. This isn't speculation: it's the literal
  blocker already on record in the Phase 7 status note — *"harness would
  require aoide to adopt a model client it has explicitly refused to own."*
  Building that harness means building or adopting exactly what LangChain
  provides (chat-model wrapper, tool-calling loop, memory plumbing).

Still open. No action queued on Phase 7 without khoa's call.

---

## 3. Open proposal: reframe around a session graph, backed by a graph DB

khoa's framing: change the *positioning* to "Aoide graphs agent and
terminal sessions, keeping memory and compacts context" backed by a real graph database for concurrent-session
management, instead of the current flat-file + re-walked-in-app-code
approach. **Discussed, not decided, not built.**

### What exists today

The DAG already exists *conceptually* — `conduct` tracks parent/child
sessions and projects — but isn't *stored* as a graph. It's flat
`sessions.json` / `projects.json`, re-walked into a tree **twice**: once in
Rust (`conduct`), once again in QML (the Conductor widget re-derives
grouping/rollups client-side). That duplication — not storage engine — is
the demonstrated pain point today.

### The case for a real graph store

An embedded graph store (a Rust-native graph engine, or the lighter middle
ground — SQLite with a recursive-CTE adjacency schema, still zero-daemon)
buys native traversal queries and makes "Aoide = a graph of terminal
sessions" a claim backed by real infra rather than app-code tree-walking.
It also sharpens the §1 comparison into something cleaner: *LangGraph
graphs LLM calls inside a process; Aoide graphs terminal/agent processes
themselves* — same shape, one layer down. If `steward` ever goes
LLM-driven (§2), a shared graph substrate could unify its reasoning-state
tracking with session tracking instead of running two.

### The case against, right now

- **Reverses a decision already on record.** `storage` is file-first *by
  choice* (Phase 3: "zero-dep, matches today's discipline… until the
  steward's memory needs query/search" — hasn't happened, steward's
  deferred). Every other Aoide state file is atomic-write-then-rename JSON,
  `cat`/`jq`-debuggable, no query tool required. A graph DB is a real step
  away from that house discipline, not just a new dependency.
- **Unproven bottleneck.** Session count today is tens of terminals, not
  millions — the O(n) rebuild-on-read isn't demonstrated to be a real
  problem yet. Solving it ahead of the interface-duplication problem (which
  demonstrably *is* real) is scope ahead of evidence.
- **A graph DB doesn't by itself fix the duplication.** It changes *where*
  the data lives, not *who* computes derived views — either the daemon
  starts serving graph queries to the widget, or the widget still needs
  pre-shaped data. That's solvable today without a new storage engine:
  precompute the DAG once in Rust, publish a ready-walked tree in the stage
  files, let QML only render.

### Open question, unresolved

What's the actual driver — wanting the graph-db infra itself (as
positioning/capability), or wanting to stop computing the tree twice in two
languages? The two have different right answers. Not settled this session.

---

## 4. Intention: shift away from a plain DAG, toward a knowledge-graph paradigm

khoa's stated direction: Aoide's session graph should stop being modeled as
a plain parent/child **DAG of processes** and instead adhere to the
paradigms in [codejunkie99/graph-engineering's curriculum.md](https://github.com/codejunkie99/graph-engineering/blob/master/graph-engineering/references/curriculum.md)
— a **knowledge-graph (KG) engineering** course. **Stated intention only —
not scoped, not designed, not built.**

### What that curriculum actually teaches

Verified against the source, not inferred from the name. It's a 9-lecture
KG foundations course (2025 edition):

1. **Theory** — what a knowledge graph is; KG's lineage (semantic networks →
   expert systems → Semantic Web → Google's 2012 Knowledge Graph); KG vs.
   deep learning vs. traditional databases; the KG technology stack:
   **extraction → fusion → representation learning → reasoning → storage**.
2. **Representation** — semantic networks, production/rule systems, frame
   systems, conceptual graphs, description logic, ontologies, ontology
   languages (RDF/RDFS/OWL), and KG embeddings.
3. **Ontology modeling** — schema-first design, semi-automatic schema
   induction, modeling tooling (e.g. Protégé).
4. **Knowledge extraction** — from structured (D2R-style mapping),
   semi-structured (wrappers/web tables), and unstructured text sources.
5. **Entity recognition** — rule/dictionary → classical ML → deep learning
   → pretrained/LLM-era methods.
6. **Relation extraction** — template/pattern, supervised, distant
   supervision, open IE, deep/RL methods.
7. **Event extraction** — triggers, arguments, event types, and
   **event-logic graphs** (事理图谱) — graphs whose *edges* carry
   causal/temporal/conditional semantics between events, not just structural
   ones.
8. **Knowledge fusion** — ontology matching, instance/entity matching and
   deduplication at scale, resolving heterogeneous sources into one graph.
9. **KG × LLMs, bidirectionally** — KG-for-LLM (grounding, retrieval,
   hallucination reduction, structured memory) and LLM-for-KG (LLM-assisted
   extraction, schema induction, fusion).

The curriculum takes **no dogmatic stance** on property graphs vs. RDF or
on a specific graph database — the commitments are **ontology-first
modeling**, **typed entities and typed/semantic relations** (not bare
structural edges), and a **pipeline discipline** (extraction → fusion →
representation → reasoning → storage), not a technology pick.

### The shape of the intended shift, as far as it's been said

Not designed — but the direction implies replacing "session A is a child
process of session B" (structural DAG edges only) with something closer to
a KG's typed-entity/typed-relation model: sessions, agents, projects, and
hooks as distinct entity types, related by *semantically meaningful* edges
(not just parent/child) — closer to the curriculum's **event-logic graph**
idea (lecture 7) than to a plain process tree, since Aoide's hook stream is
already literally a sequence of causally/temporally related events
(session spawned → became `awaiting` → got `needsSudo` → a human answered →
resumed `working`). The **KG × LLM** lecture (9) is the other evident point
of contact: it names exactly the kind of thing the deferred `steward`
`canon` (§2, Phase 7) would need if it's ever built — LLM-assisted
extraction of structured facts (design decisions, session outcomes) into a
graph, and the graph in turn grounding future agent turns.

### Tension with §3, named plainly

This is a considerably larger scope than "put the existing DAG in a graph
database." §3's proposal was a storage-engine swap under an unchanged data
model (still parent/child sessions, just queryable). A KG paradigm shift
is a **data-model** change first — ontology design, typed relations, an
extraction/fusion pipeline — and only secondarily a storage question. The
two proposals are compatible (a KG needs a store too) but shouldn't be
conflated: adopting curriculum's paradigms doesn't require picking §3's
graph-DB-vs-file-first fight first, and picking a graph DB doesn't by
itself get you a knowledge graph. Not reconciled this session — recorded
as two related but distinct open threads.

---

## Related

- `docs/architecture/PACKAGE-LAYOUT.md` — the crate restructure, Phase 7
  (`steward`) status note referenced in §2.
- [[Conductor-Channel]], [[Session-Graph]] — the current session-DAG
  implementation this doc discusses reframing.
- [[A2A-Door]] — the interop wire referenced in §1.
- [codejunkie99/graph-engineering curriculum.md](https://github.com/codejunkie99/graph-engineering/blob/master/graph-engineering/references/curriculum.md)
  — the knowledge-graph paradigm referenced in §4.
