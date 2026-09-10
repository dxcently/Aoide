# Decisions and open questions: Mneme and Aoide integration

This register gathers the approved interoperability direction, Claude's eight mailed architecture questions (mail sequences 18, 21–23), and implementation questions exposed by fetched receipts. The [response to Noah](https://github.com/dxcently/Aoide/issues/3#issuecomment-5611283732) records the approved scope; the [build register](aoide-interoperability-build.md) owns current work assignments and acceptance evidence. Decided behavior is distinct from implemented behavior. Historical defects require a source check before a coding brief.

## Decided requirements

- Under Aoide, sessions bound to the same persona share memories and history through Mneme across harnesses, including Claude and Codex.
- Melete is optional. Core access works through a shell without Lyra.
- Fetching a letter produces a distinct fetched receipt. It does not assert understanding, action, or completion. Receipt reads do not recursively generate receipts.
- Enduring identity uses an opaque key independent of session/display names, hosts, and harnesses; the upstream persona-reference convention remains open.
- Initial context contains the handoff package, persona, canonical memory, vault entry points, selected references, and destination capabilities. Revisions are checked at turn boundaries; older history is explicitly retrievable.
- Available harness transcripts are normalized with source pointers and log fallback. Capture includes prompts, responses, tool calls/results, presentation/artifacts, exposed reasoning records, and evidenced file changes; unavailable records are reported.
- Handoff transfers work and shared-memory write ownership after checkpoint and successor loading. The predecessor remains idle and available; collaboration is explicit.
- Mail addresses endure across executors. Read cursors remain executor-specific; previous reads do not remove successor access to retained correspondence.
- File-backed Mneme vaults remain authoritative, with one authority and explicit read replicas per logical vault. Replicas reject writes; no automatic write failover. SQLite replacement and custom replication are outside this implementation.
- Melete memory migration has one canonical destination and an explicit cutover, preserving old data for recovery without dual writes. The project BBS is separate.

## Shared personas, memory, and history

| ID | Question | Why the answer matters |
|---|---|---|
| M1 | Which upstream persona-reference convention maps to the approved opaque agent key? | Identity independence is decided. Mapping/migration must preserve attribution across persona moves and renames. |
| M2 | Where is Melete's memory-backend seam, and how is import/cutover verified? | One canonical Mneme destination is decided; recovery copies must not remain writable masters. |
| M3 | Does a persona use a folder, a named vault, or references across collections? | This affects discovery, grants, replication selection, and duplicate names. |
| M4 | What normalized record schema and source cursors implement shared history? | Requested record classes and original-log fallback are decided; adapter fidelity and recovery remain implementation work. |
| M5 | How are required context references and handoff contents validated and bounded? | Initial context categories and explicit older-history retrieval are decided; missing required references must be visible. |
| M6 | What revision/checkpoint API supports turn-boundary refresh? | Refresh timing is decided; per-content hashes cannot be presented as server revisions or an atomic multi-note snapshot. |
| M7 | How are portable persona/capability requirements represented and destination availability checked? | Agent-agnostic context is retained; live tools and authorization must be revalidated. |
| M8 | How are session bindings and collection grants authenticated? | A selected vault, mailbox name, or persona string alone does not confer read/write access or tool permissions. |
| M9 | How do indexed sources retain canonical references and writer ownership? | Source files and reviewed project docs remain authoritative; indexing does not move writable authority. |
| M10 | Which persona, history, handoff, and freshness fields appear in Lyra widgets? | The shell bridge must expose the chosen state; widgets display it without owning the memory service. |

## Storage and replication implementation

| ID | Question | Why the answer matters |
|---|---|---|
| S1 | Outside scope: managed database replacement | File-backed authority remains; no SQLite conversion or materialized-file editor adapter in this implementation. |
| S2 | Which machines replicate which collections? | Local replicas require authorized snapshot/change delivery, including deletion and reconnect behavior. A handoff notice alone does not synchronize data. |
| S3 | How do read replicas report freshness and recover after disconnection? | Replica writes are refused; Git is the proposed first transport, with its actual consistency limits. |
| S4 | Decided: no automatic authority promotion | An unreachable authority does not make a replica writable. Fresh reads use authority or wait for a sufficiently current replica. |
| S5 | What are retention and erasure rules for memory and shared history? | Excluding an expired record from retrieval differs from removing it from revisions, replicas, exports, and backups. |
| S6 | Are attachments included beyond text and Canvas? | Binary storage, references, transfer, and preservation are outside the current pilot contract. |

## Eight original Aoide architecture questions

| ID | Question | Existing decision context |
|---|---|---|
| A1 | Which widget tree is authoritative for reload and deployment? | Claude reported deployment copying repository widgets into run/qml while reload copied an independently seeded runtime songbook, allowing reload to restore older widgets. |
| A2 | What result should a failed shell reload return? | Staging/draft could report status ok alongside an IPC reload failure. No running shell is a distinct headless case. |
| A3 | Where should send validate that its control socket exists? | A stored nonempty socket path can outlive the actual socket. The fork concerns checks at I/O boundaries versus shared validation code. |
| A4 | Where does the review rule for stale derived state belong? | Cursor and socket defects recur when stored observations are treated as current facts. A brief template and the shared CRAFT protocol have different reach. |
| A5 | Who owns the core runtime directory and its lifetime? | A Lyra shellbridge restart was observed deleting the directory containing core sockets. Preservation and separating ownership address different parts of the problem; module changes and rebuilding remain user-gated. |
| A6 | Where is remote text made safe for terminal rendering? | Letter fields, peer names, and remote outbox outcomes reach different render paths. The location of normalization determines coverage. |
| A7 | Must reader hardening precede remotely reachable mail? | Remote deposits widened who could supply terminal-rendered text while the specification scheduled rendering hardening later. The question concerns release dependency and any interim behavior. |
| A8 | How does the audit receive a handler's logical outcome? | A refused deposit encoded in a successful RPC result could produce generic ok and bespoke invalid records for the same request. Transport success and logical acceptance need distinguishable reporting. |

## Doorbell integration questions

These gaps appear in Claude's handoff letter at mail sequence 29. Claude reports no prepared brief or implementation. Implementation status and dependencies require verification at brief time.

| ID | Question | Why the answer matters |
|---|---|---|
| D1 | Where does the per-reader notification latch live, and how does it recover after restart? | In-memory and durable state have different replay and missed-wakeup behavior. The specification names a latch without fixing its storage. |
| D2 | Which layer reacts to a filed letter and schedules the nudge? | Storage cannot depend on conduct, which already depends on storage. Filing, policy, and prompt delivery need a defined boundary. |
| D3 | How does a mailbox reader bind to a live, prompt-ready session? | A fallback reader key such as claude-mail is not a session ID. Busy, disconnected, resumed, and approval-blocked sessions need distinct handling. |
| D4 | What is the delivery sequence and acceptance gate for the local doorbell? | The phase list places P-M5 after remote transit work; a local idle-agent loop and a two-node integration gate prove different capabilities. |

## Fetched-receipt implementation questions

These follow from the new requirement; they are not part of Claude's original eight.

| ID | Question | Why the answer matters |
|---|---|---|
| F1 | At what observable boundary is a fetch receipt created? | Storage selection, cursor persistence, returning a tool result, and model-context delivery are different events. The receipt must name the boundary the implementation can prove. |
| F2 | How are receipts identified across readers, retries, and rereads? | Aoide has a separate cursor per reader. Multiple legitimate readers and repeated retrieval must not be mistaken for the same event or cause notification floods. |
| F3 | How are cursor advancement and durable receipt creation recovered after failure? | A crash between them can otherwise lose a receipt or duplicate one; a network failure must not erase its pending return delivery. |
| F4 | How does a fetched receipt reach and wake its original sender? | Return addressing and transport acknowledgments must remain distinct. Storing the fetched receipt and submitting an idle-agent nudge are separate steps; automatic wakeup still depends on P-M5. |

## Operational review and release questions

Claude's handoff (mail sequences 28–30) adds these items. They are distinct from the design choices above; reported verification is not an independent rerun.

| ID | Question or outstanding evidence | Why it matters |
|---|---|---|
| O1 | How are incompatible mail-store readers rejected, and which MCP clients still require refresh? | Claude reports old running MCP binaries can reinterpret nested cursors as the old format and rewrite them. Refreshing clients mitigates the live mismatch; a durable compatibility boundary prevents recurrence. The coordinator uses the installed CLI rather than the stale MCP reader. |
| O2 | Where is the required P-M2 two-node live-test evidence? | Claude reports per-crate tests and the schema check completed, but no yomi-to-osaka live run. The documented verification gate remains incomplete until evidence is recorded or the requirement is explicitly changed. |
| O3 | How is the existing mail branch reviewed and integrated, and what happens to local main? | Local refs checked at handoff show feat/mail-wire matches origin/feat/mail-wire, while main is 39 ahead and zero behind origin/main. These are cached remote refs, not a fresh fetch. Claude reports no PR. No reset, merge, or branch rewrite is authorized by this register. |

Separate existing worktree changes under pkgs/chatgpt-linux, the OpenAI dendrite, and flake files require their own packaging review. They are not P-M5 and are not accepted merely because the orchestration role changed.

## Sources and review boundary

- [Current proposal](mneme-shared-memory-and-aoide.md), whose historical source review must be distinguished from deployed capability.
- [Approved response to Noah](https://github.com/dxcently/Aoide/issues/3#issuecomment-5611283732): retains file-backed storage and existing upstream interfaces while specifying interoperability behavior.
- [Mail design](../../architecture/MAIL.md): its specified phases are not all implemented.
- Original mailed questions are retained in the local mailbase, sequences 18 and 21–23; this register does not advance mail cursors.

Noah's reviewed PR contributes explicit vault retrieval, optional Git-HEAD write preconditions, and divergent-history handling. It does not implement the Aoide adapter, shared persona history, atomic record revisions, or automatic idle-agent delivery.
