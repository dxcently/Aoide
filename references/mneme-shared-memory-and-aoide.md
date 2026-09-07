# Proposal: shared memory and vault replication through Aoide

Draft for discussion. Mneme owns durable knowledge, revisions, retrieval, and
replication. Aoide owns session coordination, node relationships, and delivery
of notifications. Agents keep using Mneme through MCP.

The central rule is **one authoritative record, many clients and replicas**.
An edit made from any connected machine commits at the authority; replicas
follow that committed history. A local copy is not an independent source of
truth.

**Current operation versus proposed operation**

| Concern | Mneme / Aoide today, checked in source | Proposed system |
|---|---|---|
| Knowledge storage | Mneme reads and writes text files under configured vault roots. There is no central document database. | One authoritative Mneme store contains note bodies, memory records, revisions, and a durable change journal. |
| Multiple vaults | Already supported through `MNEME_VAULTS`; RPC selects a named vault. Each has its own root and sidecars. `dispatch`, `get_index`, and skills still target the default vault. | Preserve logical vaults as boundaries for ownership, access, conventions, and replication selection within the shared store; make context retrieval explicitly vault-aware. |
| MCP | Two tools, `dispatch` and `RPC`, expose note operations, discovery, recovery, conventions, skills, and semantic queries. | Keep these entry points. Extend their functions with memory operations, stable identities, revisions, and replication discovery. |
| Conventions and schemas | Mneme serves operating conventions and canonical skill bodies. Most editorial rules depend on the agent following them. | Ordinary notes retain lightweight conventions. Memory records get the fields and lifecycle checks necessary for reliable reuse. |
| Retrieval | Text search and optional section embeddings already exist; semantic queries are scoped to one vault. | Select authorized vaults, filter memory state and scope, then retrieve relevant context with source revisions. |
| Agent memory | Agents can write memories as notes, but there is no dedicated lifecycle for capture, validation, expiry, supersession, or session hydration. | Add those operations over the same durable store; the calling agent performs interpretation and summarization. |
| Sync | Passive mode delegates to external file sync. Git mode pulls on reads and commits/pushes after writes, favoring local conflicting changes during push recovery. | Replicas consume committed revisions and deletions from the authority. Conflicting edits return a conflict instead of silently choosing a winner. |
| Concurrent editing | Some edit tools validate hashes or matching text. Whole-note updates have no expected-revision argument or transaction spanning update and notification. | Every managed write checks its base revision and commits content, revision, and change entry together. |
| Recovery | External version snapshots are enabled by default outside Git mode; snapshot failures do not block overwrites. Trash is configurable. Raw filesystem edits bypass these handlers. | Managed changes have transactional revision history. Replica deletion and recovery follow explicit changes in the same journal. |
| Access | OAuth guards the server. Vault arguments select data; the handler layer has no per-agent vault grant model. | Mneme authorizes vault reads, writes, and replication independently of Aoide pairing and mailbox names. |
| Aoide integration | A Mneme service launcher exists. Aoide's content commands are stubs; session hooks and mesh pairing/reporting exist. | An adapter connects session context, Mneme references, and change notifications without owning a second wiki. |
| Messaging | Aoide has live conduct and a signed local mailbase. Remote mail, outbox, hub transit, and doorbells are designed but absent from the inspected mail implementation. | Use local integration first; attach notifications to remote mail when that transport exists. Mneme replication remains recoverable without notifications. |

Ground truth: [Mneme architecture](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/wiki/architecture.md),
[RPC registry](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/src/rpc.rs), [write handler](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/src/main.rs),
[sync implementation](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/src/sync.rs), [convention store](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/src/conventions.rs),
and [authentication configuration](https://github.com/noah427/mneme/blob/551229919fac07724de029dfbb7325b29dddbc16/src/auth.rs).
Aoide evidence: launcher (`modules/dendrites/mneme.nix`),
content stubs (`pkgs/aoide/crates/cli/src/commands/stubs.rs`),
session hooks (`pkgs/aoide/crates/conduct/src/commands/hooks.rs`),
mesh implementation (`pkgs/aoide/crates/client/src/mesh.rs`), and
local mail implementation (`pkgs/aoide/crates/storage/src/mail.rs`).

The current architecture is:

```text
Agents -- OAuth / MCP --> Mneme -- read/write --> Vault folders
                           |                       |
                           +-- conventions         +-- external file sync
                           +-- skills              |   OR Mneme's Git hooks
                           +-- embedding sidecars  v
                                               Other machines' folders

Aoide -- launcher --> Mneme service
Aoide -- hooks / conduct / pairing --> Agent sessions and other nodes
Aoide -- mail --> Local signed letters
```

**Three responsibilities, one knowledge system**

| Concept | What it is | Example | Management rule |
|---|---|---|---|
| Agent memory management | A lifecycle and retrieval service over records. | A session handoff, a verified constraint, a learned preference. | Capture with provenance; distinguish observations from maintained facts; supersede outdated records; exclude expired records from normal retrieval. |
| Agent vault | A logical collection or view for an enduring agent role or team. | Rook's handoffs and working observations across different harnesses and machines. | Scope by role, project, and access. A new session or agent provider does not create a new knowledge database. |
| Regular notes vault | Human-facing documents and project knowledge. | Personal notes, project wikis, architecture decisions, research. | Edit, organize, link, search, version, and sync. No automatic memory expiry applies to ordinary notes. |

These are different axes: a vault is a container; memory management is behavior.
An agent can edit a project wiki without making the wiki an agent-memory vault.
A shared project decision belongs to that project's notes; an agent's memory
links to it rather than copying its body into a private knowledge base.

```text
One Mneme knowledge store
├── personal                  ordinary notes
├── projects
│   ├── mneme                 maintained project wiki
│   └── aoide                 maintained project wiki
└── agents
    ├── rook                  role memory and handoffs
    └── shared                team observations and reusable context

Machine and harness views --> references/subsets of these same records
```

The tree illustrates organization, not a requirement for one physical folder or
database per branch. Separate logical vaults where access or replication differs;
use folders or views for organization inside those boundaries.

A memory record starts with a stable ID, body, kind, project/owner scope,
source reference and revision, recorded time, and lifecycle state. Supersession
links retain why an earlier record stopped being current. Expiry applies to
temporary handoffs where useful. Agent-authored assertions remain attributed;
repetition or a high embedding score does not validate a claim.

```text
Agent observation --> attributed memory --> checked against source
                                               |
                                               v
                                    maintained project note
                                               |
                                               v
                               memory points to canonical note

New evidence --> supersede old memory --> future retrieval uses current state
```

Mneme enforces storage and lifecycle rules. The agent performs semantic work;
this proposal does not add an LLM provider inside the server. Existing conventions
and skills remain the shared instructions that agents explicitly load.

**Authority, database, and replicas**

Centralization can start today by pointing clients at one file-backed Mneme
instance. A database conversion is a separate change needed for the proposed
transactional record model, not a prerequisite for sharing the existing API.

For the managed store, the proposed first implementation is SQLite on the
authority's local disk, accessed only through Mneme. It keeps the existing
single-service deployment small. SQLite documents this application-server use
and its serialized writes; high concurrent write demand is a reason to revisit
the engine. [SQLite deployment guidance](https://www.sqlite.org/whentouse.html),
[transaction isolation](https://www.sqlite.org/isolation.html).

```text
Agent on A -- MCP -----------------------------+
Agent on B -- MCP -----------------------------+
Human editor -- Mneme editor adapter ----------+
                                              v
                                   Authoritative Mneme
                                   +---------------------+
                                   | Canonical database  |
                                   | notes + memories    |
                                   | revisions + changes |
                                   +---------------------+
                                              |
                               authorized snapshot + changes
                                  /                       \
                                 v                         v
                        Replica on A                Replica on B
                        local read API              local read API
                        Markdown view               Markdown view
                        derived search index        derived search index

Replica write requests ------------------> Authoritative Mneme
```

- **One writer authority for the initial shared store.** Multiple clients submit writes; the authority orders commits. There is no automatic replica promotion during a partition.
- **Stable identity.** A note's ID survives a rename or move. Replicas retain the same vault and note IDs; paths are presentation and lookup aliases.
- **Revision checks.** A write supplies the revision it read and an operation ID. The authority rejects a stale base; a repeated operation returns its recorded result without applying twice.
- **Atomic changes.** A transaction saves the new content, its revision, and a sequenced change entry together. Rename/backlink updates belong to one logical change.
- **Catch-up.** A replica starts from an authorized snapshot at a known cursor, then applies later changes in order. It advances its cursor only with the corresponding local commit. An expired cursor requires a fresh snapshot.
- **Deletions.** Tombstones propagate deletion. Retaining them through the supported replay window prevents an old replica from resurrecting a deleted note.
- **Selective replication.** Each node receives explicitly granted vaults. Discovery of a node or membership in a mesh does not grant access to personal notes.
- **Derived copies.** Embeddings, search indexes, and exported Markdown can be rebuilt. Replica application updates or removes affected index entries; old semantic handles are not portable record IDs.
- **Freshness.** Replica reads expose their applied cursor and last successful synchronization. A read requiring a just-committed revision waits for it or uses the authority.
- **Offline default.** Replicas serve their last committed state while disconnected. Writes require the authority; autonomous offline editing and conflict merging remain an open requirement.

SQLite's transaction guarantees cover the local database, not replication between
machines. Mneme implements the snapshot/change protocol; a live `.db` file is not
distributed through ordinary folder sync.

**Keeping ordinary vault editing usable**

The managed database stores canonical note bodies as Markdown/text and preserves
Canvas content. Files on machines are materialized views for existing editors.
An editor adapter submits edits with the note ID and base revision, then refreshes
the view after a successful commit. Conflicts preserve the unsent edit for review.
Creates, renames, deletes, and restores need the same adapter contract; absence
from a partially exported folder is not itself an instruction to delete a note.

```text
Editor buffer --> adapter + base revision --> Mneme commit --> refreshed file view
```

A watcher alone cannot promise conflict-safe editing: it must know which revision
produced the edited file and distinguish its own exports from user changes.
The editor adapter is part of the database migration's acceptance criteria.
Until it exists, the existing file-backed mode remains the usable notes workflow.

Importing a vault into managed storage establishes the database as its authority.
The original folder becomes an archive or a managed view; it does not continue as
an independently writable master. Existing folder-backed deployments remain
supported. Registration-in-place and physical consolidation are different things:
serving several existing roots gives one API, but does not yet give one database.

**Chaining memory to Aoide**

```text
Session starts
  --> Aoide exposes session / role / project context
  --> adapter requests permitted, relevant context from Mneme
  --> harness presents that context to its agent

Agent records a finding through MCP
  --> Mneme validates scope and base revision
  --> commit content + revision + change entry
  --> adapter publishes a small Aoide change notice
  --> replica reads Mneme changes after its cursor
  --> subscribed agent pulls the updated record when needed
```

The session-to-context adapter is new work. Existing hook installation tracks
session events; it does not already hydrate Mneme memory. Explicit checkpoints
save important handoffs during work, since a session-end hook can be missed.

Aoide letters carry references and change cursors, for example
`{kind: "mneme.changed", vault_id: "projects", through_seq: 1842}`.
Mneme serves the authoritative content after checking access. Mailing complete
wikis would create another retained copy in the mailbase and, eventually, transit
hubs. The inspected mail design explicitly treats mail as readable data at rest.

The durable change journal also acts as the notification outbox: the adapter
retries committed entries after a crash. Recipients tolerate duplicate notices;
periodic cursor reconciliation catches missed notices. This is the
[transactional outbox pattern](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html).
Aoide's mail outbox handles delivery once implemented; Mneme's journal records
which knowledge changes exist. These stores have different responsibilities.

A mail delivery acknowledgement means the notice arrived. It does not mean a
replica applied the revision or an agent incorporated it into context. Mail
remains data that agents pull; intentional tasking continues through Aoide's
conduct path and its existing gate.

Aoide pairing supplies node identity and connectivity. Mneme keeps its own
vault authorization, including reads, search results, history, and replication.
Mapping a verified Aoide node to a Mneme grant is an explicit integration step;
an arbitrary session ID or mailbox name is attribution, not authentication.
Existing source admission policy in Aoide applies when connecting new content.

**Build sequence and acceptance**

| Step | Concrete deliverable | Evidence that it works |
|---|---|---|
| 1. Shared context | Existing Mneme endpoint shared by two different agent clients; project/role organization; a small Aoide adapter retrieves context and saves explicit handoffs through MCP. | Agent B can retrieve Agent A's sourced handoff without maintaining a second wiki; regular note operations still work. |
| 2. Managed authority | Stable IDs, revision-checked transactions, managed database, durable change journal, import/export and editor adapter for one pilot vault. | Two edits from the same base cannot silently overwrite each other; a crash after commit cannot lose the change notice; notes/links/Canvas survive import and export. |
| 3. Replica vaults | Authorized snapshot and cursor catch-up on a second machine; local read API; write forwarding; deletion and rename propagation. | Disconnect/reconnect converges to the same committed state; repeated changes do not duplicate content; unauthorized vaults are absent. |
| 4. Aoide remote notices | Adapter publishes references using Aoide's implemented remote mail transport and routing rules. | Dropped or repeated notices do not affect convergence; receiving a letter does not execute its contents. |

Step 1 uses existing storage and exposes whether the memory workflow is useful.
Steps 2 and 3 establish the guarantees that distinguish this proposal from adding
another folder-sync wrapper. Step 4 depends on Aoide's remaining remote mail work;
replica catch-up can use a reachable Mneme API before that work lands.

**Open decisions and review limits**

- Offline editing: this draft assumes offline reads and connected commits; accepting disconnected writes adds durable pending edits and a conflict-resolution workflow.
- Availability: replicas improve read availability; automatic write failover needs a separate ownership/consensus design.
- Memory removal: expiry stops routine retrieval; erasure across revisions, replicas, exports, and backups needs an explicit retention policy.
- Scope: the first pilot covers Mneme's current text and Canvas surface; general attachment storage is not specified here.
- Verification: source review of Mneme `5512299` and Aoide `90e87ea`, performed 2026-09-07. No running service, cross-machine sync, or executable tests were exercised.
- Documentation drift: older README counts and parts of the roadmaps lag the source; Aoide's mail design describes more than its current implementation.

**Proposal log**

- 2026-09-07: Drafted the current/proposed comparison, responsibility split, managed-store target, Aoide adapter boundaries, and staged acceptance criteria. No runtime implementation changed.
