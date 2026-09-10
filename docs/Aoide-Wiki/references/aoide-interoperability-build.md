# Aoide interoperability build register

This register coordinates implementation of the [approved response to Noah](https://github.com/dxcently/Aoide/issues/3#issuecomment-5611283732). Reserved work is not implemented behavior. User explicitly authorizes development code and documentation edits outside `song/` for this implementation; runtime activation remains a separate gate.

## Ownership

One active implementation worker per task. Both coordinators may dispatch subagents for distinct tasks and independent review. Dispatch transfers implementation ownership; it does not create a second implementation. The author does not independently review their own diff. Cargo runs are serialized across both sessions.

| Task | Coordinator | Worker | State / scope |
|---|---|---|---|
| Work register and integration decisions | Codex | Codex | This document |
| P-M5, fetched receipts, control readiness | Fable | One executor for P-M5a-1 | Latch/storage and stale-socket slice dispatched; Fable owns Cargo; actual injection awaits readiness ruling |
| Runtime directory preservation | Codex | Executor and independent reviewer complete | Commit `ff01b77`; selected Nix evaluation and formatting passed; no activation |
| Enduring identity/context architecture | Codex | `triad_architecture` | Read-only brief complete; worker idle |
| Identity/context implementation | Codex | `context_integration` | K1a binding and K1b retrieval preparation; command wiring/docs wait for shared-path release; no Cargo |
| Independent review | Owning coordinator | None | Assigned after a diff exists |
| Melete backend, replicas, audit | Joint | None | Later slices and upstream contracts |

Fable reserves storage `mail.rs`; conduct `graph/doorbell.rs`, `graph/send.rs`, `graph/doc.rs`, `graph.rs`, `commands/graph.rs`; server `a2a.rs` deposit arm; client `commands.rs` mail-send handler; CLI registration/golden; the mail contract and owning crate docs. Exact scope is in the P-M5a brief. Codex holds conflicting K1 writes until those paths are released.

The K1 executor owns only storage `config.rs`, `records.rs`, `session.rs`, `ledger.rs`; conduct `graph/session_store.rs`; client `context.rs`, `mcp_client.rs`, `lib.rs`, and the client manifest's existing-workspace hash dependency. Changes requiring reserved callsites wait. This is preparation within one coherent implementation; command wiring and matching documentation must land before it is considered complete.

Coordination addresses are `self/codex-integration` and `self/claude-mail`. Fable uses executor `e7fce841-5c0e-4d6b-b9e5-f86d4f8c6f0d`. This is a routing fact, not an enduring identity. Neither coordinator reads the other's mailbox to infer acknowledgment. The idle user shell attributed beneath Fable is not a worker.

Existing flake, OpenAI dendrite, and ChatGPT packaging changes remain outside this work's ownership. Nix composition and Lyra migration retain their [separate plan](aoide-composable-system-and-muse-triad.md); neither blocks shell-only interoperability. That architecture is implemented in AoideOS within this Aoide repository first. `dxflake` is the reference and later migration target, not a current write target.

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
- Reader identity: existing wrap-keyed cursors do not distinguish every successive harness conversation. Executor read identity and ancestor socket resolution remain separate integration contracts.
- Fetched receipts: observable retrieval boundary, idempotence, cursor/receipt crash ordering, return wakeup. Server-side retrieval cannot claim model ingestion.
- Identity/context: smallest useful binding/retrieval slice, compatibility, explicit persona reference pending Noah's ID convention.
- Shared paths: storage records/config/session/ledger, command registry/goldens, Cargo manifests have one writer at a time.

The proposed first context slice binds existing sessions to an operator-declared opaque key and retrieves explicit persona/memory references through Mneme. It does not yet automate prompt insertion or ownership transfer. Existing executor cursors and `--reread` are reused. Mneme's public MCP `RPC` wrapper and initialize/session exchange must be supported; Aoide's stateless Melete request assumption is insufficient. Content hashes describe fetched bytes, not a server revision or atomic multi-note snapshot.

The native Codex app session is not a conducted terminal. Running a child under `aoide conduct` does not retroactively wrap its conversation. PTY doorbell acceptance therefore uses actual conducted harness sessions; native-app prompting and tracking require an appropriate adapter. A roster entry alone proves neither capability.

The context pilot resolves credentials in the local daemon environment, without forwarding bearer values through command arguments or audit records. Mneme's current `read_note` can fall back to a title/alias; an exact-path listing preflight catches an already-missing configured note, but concurrent path changes remain a non-atomic read limitation. Returned provenance names the requested reference and fetched bytes, not an unverified resolved path.

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
- 2026-09-10: Fable sequence 43 dispatches P-M5a-1 to one executor and claims Cargo. Sequence 44 accepts daemon-owned serialized ringing, distinct enrolled/armed readers, nonrecursive fetched ring-back, and explicit replay triggers. Automatic submission into an unknown interactive composer remains unaccepted; dedicated headless sessions or an input-readiness contract are required. The shared field remains `enduringAgentId` rather than a second `agentKey` name. A temporary wiring transfer between P-M5 slices is requested for K1a.
- 2026-09-10: User clarifies that the earlier Nix/Lyra architecture targets AoideOS first; dxflake follows later. The composition plan and ownership scope reflect this order. K1's sole executor pauses after preparing unshared binding/config and retrieval code; no Cargo or duplicate worker is started while waiting for shared paths.
- 2026-09-10: Sequences 46 and 48 ask Fable to separate identity binding from optional knowledge-service configuration and distinguish executor identity from a reusable terminal wrapper. The CLI JSON mail response repeats letter text in its human message and structured entries; coordinator reads now select structured entries to avoid duplicate context and truncation.
