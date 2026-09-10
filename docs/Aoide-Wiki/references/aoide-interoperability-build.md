# Aoide interoperability build register

This register coordinates implementation of the [approved response to Noah](https://github.com/dxcently/Aoide/issues/3#issuecomment-5611283732). Reserved work is not implemented behavior. User explicitly authorizes development code and documentation edits outside `song/` for this implementation; runtime activation remains a separate gate.

## Ownership

One active implementation worker per task. Both coordinators may dispatch subagents for distinct tasks and independent review. Dispatch transfers implementation ownership; it does not create a second implementation. The author does not independently review their own diff. Cargo runs are serialized across both sessions.

| Task | Coordinator | Worker | State / scope |
|---|---|---|---|
| Work register and integration decisions | Codex | Codex | This document |
| P-M5, fetched receipts, control readiness | Fable | Fable for architecture | Split accepted; P-M5a brief under peer discussion |
| Runtime directory preservation | Codex | Executor and independent reviewer complete | Commit `ff01b77`; selected Nix evaluation and formatting passed; no activation |
| Enduring identity/context architecture | Codex | `triad_architecture` | Read-only brief complete; worker idle |
| Identity/context implementation | Codex | `context_integration` | Unshared storage binding/config and client context implementation; command wiring/docs wait for shared-path release; Cargo not granted |
| Independent review | Owning coordinator | None | Assigned after a diff exists |
| Melete backend, replicas, audit | Joint | None | Later slices and upstream contracts |

Fable reserves storage `mail.rs`; conduct `graph/doorbell.rs`, `graph/send.rs`, `graph/doc.rs`, `graph.rs`, `commands/graph.rs`; server `a2a.rs` deposit arm; client `commands.rs` mail-send handler; CLI registration/golden; the mail contract and owning crate docs. Exact scope is in the P-M5a brief. Codex holds conflicting K1 writes until those paths are released.

The K1 executor owns only storage `config.rs`, `records.rs`, `session.rs`, `ledger.rs`; conduct `graph/session_store.rs`; client `context.rs`, `mcp_client.rs`, `lib.rs`, and the client manifest's existing-workspace hash dependency. Changes requiring reserved callsites wait. This is preparation within one coherent implementation; command wiring and matching documentation must land before it is considered complete.

Coordination addresses are `self/codex-integration` and `self/claude-mail`. Fable uses executor `e7fce841-5c0e-4d6b-b9e5-f86d4f8c6f0d`. This is a routing fact, not an enduring identity. Neither coordinator reads the other's mailbox to infer acknowledgment. The idle user shell attributed beneath Fable is not a worker.

Existing flake, OpenAI dendrite, and ChatGPT packaging changes remain outside this work's ownership. Nix composition and Lyra migration retain their [separate plan](aoide-composable-system-and-muse-triad.md); neither blocks shell-only interoperability.

## Accepted behavior

| Concern | Requirement |
|---|---|
| Identity | Opaque enduring agent key; independent executor IDs, names, hostnames, and harnesses |
| Memory | Existing Mneme persona/vault facilities, one canonical memory destination, shared across authorized harnesses |
| Writers | Active executor maintains shared agent memory; collaborators retain provenance and submit findings; project documents have designated writers |
| Handoff | Finish current operation, checkpoint, load successor context, transfer ownership, retain predecessor idle and available |
| Context | Handoff package, persona, shared memory, vault entry points, selected references, destination capabilities; revision checks at turn boundaries |
| History | Normalize available harness records into Aoide index/log interfaces; preserve source pointers and explicit fallback retrieval |
| Capture | Prompts, responses, tool calls/results, presentation/artifacts, exposed reasoning records, evidenced file changes; report gaps |
| Capabilities | Retain portable requirements; recheck live availability and authorization |
| Mail | Enduring address, independent executor cursors, selected handoff correspondence, access to all retained letters |
| Fetched receipts | Retrieval only; separate from delivery and completion; receipt reads never recurse; sender receives an observable ring-back |
| Servers | Authority per logical vault, explicit read replicas, actual write rejection, freshness, no automatic promotion |
| Melete | Optional; verified memory import/cutover without dual writable masters |
| Mneme interface | Existing authenticated MCP can serve the first shell-facing client; orchestration stays outside Mneme |
| BBS | Separate extension |

File-backed vault authority remains. SQLite replacement, custom replication, and universal note IDs are outside this implementation.

## Dependencies and acceptance

```text
Control reachability + reader binding + idle readiness
  -> submitted, latched P-M5 nudge
  -> durable fetched receipt with explicit retrieval boundary

Agent binding + authorized persona/vault references
  -> shared context retrieval through Mneme
  -> checkpoint + ownership transfer + retained correspondence
  -> Claude/Codex handoff with Melete absent

Available harness records -> normalized index + source fallback
                                      -> older-history retrieval

Shared context -> Melete backend migration
Authority/freshness -> read-replica proof
Revision/checkpoint -> audit recovery
```

| Slice | Required proof |
|---|---|
| Coordination | Filed letter, intended executor receives submitted nudge, reply reaches originating mailbox |
| P-M5 | Burst coalescing including concurrent filers, cursor rearming, busy/idle handling, late readers, restart recovery, unavailable endpoints |
| Fetched receipts | Letter/reader correlation, retry/crash recovery, explicit reread policy, no recursion, durable remote return and sender notification |
| Shared context | Two harnesses retrieve canonical persona/memory without Melete; missing and unauthorized references fail explicitly |
| Handoff | Context/mail/provenance retained; predecessor reads do not erase successor access; predecessor remains available |
| History | Known source fixtures normalize; fallback pointers and capture gaps preserved |
| Melete | Verified migration, recovery copy retained, canonical destination is sole writer |
| Replica | Reject writes, report freshness, catch up after disconnect, no silent promotion |
| Audit | Resume checkpoint after interruption; detect gaps and reconcile |

## Architecture discussion

Fable and Codex settle implementation boundaries in writing before dispatching affected code:

- P-M5: reader-to-executor routing, safe prompt readiness, busy-session retry, latch durability, control socket lifetime.
- Target selection: no armed reader is different from no enrolled reader. Petname fallback cannot bypass existing latches.
- Concurrency: selecting then injecting then stamping in separate critical sections must not allow concurrent filers to flood a reader. The global stage lock cannot cover blocking terminal I/O.
- Names: an invalid name cannot be transformed into another mailbox identity by a lossy clamp.
- Fetched receipts: observable retrieval boundary, idempotence, cursor/receipt crash ordering, return wakeup. Server-side retrieval cannot claim model ingestion.
- Identity/context: smallest useful binding/retrieval slice, compatibility, explicit persona reference pending Noah's ID convention.
- Shared paths: storage records/config/session/ledger, command registry/goldens, Cargo manifests have one writer at a time.

The proposed first context slice binds existing sessions to an operator-declared opaque key and retrieves explicit persona/memory references through Mneme. It does not yet automate prompt insertion or ownership transfer. Existing executor cursors and `--reread` are reused. Mneme's public MCP `RPC` wrapper and initialize/session exchange must be supported; Aoide's stateless Melete request assumption is insufficient. Content hashes describe fetched bytes, not a server revision or atomic multi-note snapshot.

Upstream decisions remain the persona reference convention, Melete memory-backend seam, authority/read-only/freshness facilities, authenticated attribution/audit query, and Mneme service interface. Existing reads and revisions can support the first pilot without a complete upstream audit service.

## Coordination log

Append-only evidence:

- 2026-09-10: User requests joint implementation and architectural discussion with Fable, one worker per task. Previous Codex research workers are complete; `triad_architecture` resumes for a bounded read-only brief and finishes without edits or Cargo.
- 2026-09-10: Coordination letter filed to `claude-mail`, sequence 33. Direct send fails `not-conductable`. Parent conductor is alive; advertised socket path is absent although the kernel still lists the bound socket. Process ancestry and terminal identity checked.
- 2026-09-10: Lyra captures verify empty prompt, pasted request, and processing after Enter through keyboard fallback. Clipboard provider had to remain alive through paste. Offscreen capture initially clamped to 36 pixels; focus and recapture were required.
- 2026-09-10: Fable replies in sequences 36–37, accepts the split, reports no live workers or Cargo, and supplies `/tmp/aoide-p-m5a-brief.md`. Control limitations affect both existing sessions; registering an app session does not create a PTY socket.
- 2026-09-10: User explicitly approves development edits outside `song/` after automatic review rejected a work-register write. P-M5a paths acknowledged in sequence 38; concurrency, readiness, identity-preserving name handling, and receipt ring-back objections returned before execution. A separate worker receives only the runtime-directory fix.
- 2026-09-10: User clarifies that Fable should orchestrate subagents. Sequence 39 distinguishes one implementation worker per task from a restriction on total workers. Runtime-directory diff is complete and assigned to an independent reviewer; no live service changed.
- 2026-09-10: Sequence 40 announces non-overlapping K1 preparation. One executor receives only unreserved files and no Cargo token; shared command wiring and docs wait for P-M5a path release.
- 2026-09-10: Runtime preservation lands as `ff01b77` after independent review. Loaded shellbridge still reports `RuntimeDirectoryPreserve=no`; first activation must account for stopping under the old unit definition. Existing unlinked sockets remain unavailable. Sequence 41 requests a temporary transfer of the HTTP helper file for Mneme session headers; no concurrent edit is authorized.
