//! `graph` / `session` / `project` / `send` / `spawn` / `resurrect` /
//! `conduct` — thin registrations over this crate's `graph/` domain
//! (concepts/Terminal-Commander, concepts/Conductor-Channel). Handler bodies
//! live in `graph/`; this module only wires schema metadata to the
//! already-public `crate::graph::*` functions — nothing here duplicates
//! graph-domain logic.
//!
//! **The graph-prefix cutover (task #101, Lane R, Phase R1).** The `graph`
//! prefix used to own the WHOLE conducting surface; it now owns only the
//! read/analysis lens — the bare render (`aoide graph`, ex-`graph view`) and
//! `graph link`. Everything that ACTS (send/spawn/resurrect) or manages
//! session/project lifecycle promoted to its own top-level family:
//! `send`/`spawn`/`resurrect` (bare, ex-`graph send`/`graph spawn`/`graph
//! resurrect`), `session start|phase|end|hook|carry|permit|pending
//! list|approve|deny|reap|prune` (ex-`graph session *`/`graph permit`/`graph
//! pending *`/`graph reap`/`graph prune`), `project add|list|remove`
//! (ex-`graph project *`). A hard cutover — no aliases, the old `graph
//! <command>` spellings are plain unknown commands now, same as a typo. Handler
//! functions and their own file layout are UNCHANGED; only the registered
//! `path:` (and the `examples:`/summary prose that quotes an invocation)
//! moved. See `pkgs/aoide/crates/AGENTS.md` on why the handler names staying
//! put is deliberate — a command's home is its domain crate's own `commands`
//! module, independent of what its registered path spells.
//!
//! Moved from the root package's `src/commands/graph.rs` (Phase 9
//! restructure, docs/architecture/PACKAGE-LAYOUT.md): a domain's CLI commands
//! live with the domain; the root package's `commands::all()` calls
//! [`register`] at the exact historical position so `schema --json` order
//! never shifts (the graph-prefix cutover renames spellings in place, at the
//! same registration slots — it does not reorder `all()`).

use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["graph"],
        summary: "Render the project/session DAG (Unicode tree; --json emits the graph document).",
        args: [],
        flags: [flag!("focus", "string", "Node id to highlight with ▶ (session:<id>, project:<name>, or bare id).")],
        gated: false,
        implemented: true,
        handler: crate::graph::view,
        examples: [
            "graph",
            "graph --focus session:<id>",
        ],
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        kind: "graph", verb: "show",
    ));
    r.insert(cmd!(
        path: ["project", "add"],
        summary: "Register a project in state/stage/projects.json — with folders to anchor by cwd, or name-only — or ADD roots to an existing project (atomic, idempotent per root).",
        args: [
            arg!("name", "string", true, "Project name (its node id becomes project:<name>)."),
            arg!("path", "string", false, "Project root path; give one or more. Each is appended as a root — a name that already exists gains roots rather than losing the ones it has. Sessions anchor by cwd prefix, longest root wins. Omit every path to register a NAME-ONLY project: it anchors no cwd and is reached by a workspace binding or an explicit `session project`."),
        ],
        flags: [
            flag!("auto-resume", "bool", "Opt this project into the daemon's boot-time auto-resume sweep (`resurrect --project <name>` on `run_loop` entry, once per boot). Only ever sets it true — hand-edit projects.json to clear it."),
            flag!("new", "bool", "Refuse if the project name already exists instead of adding roots to it."),
            flag!("host", "string", "Re-scope the path list to that REGISTERED node's own roots instead of local ones — adds it as a member and appends any given roots (idempotent per root). With no path, membership-only; no path is ever a cwd default. Refused (unknown-host, zero writes) if the node isn't registered."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::project_add,
        examples: [
            "project add aoide ~/Aoide",
            "project add aoide",
            "project add aoide ~/Aoide --auto-resume",
            "project add aoide ~/Aoide ~/Aoide-docs",
            "project add aoide ~/Aoide --new",
        ],
        launches: true,
        kind: "project", verb: "act",
    ));
    r.insert(cmd!(
        path: ["project", "remove"],
        summary: "Unregister a project anchor root, or one root of a multi-root project (ok + no-op if absent).",
        args: [
            arg!("name", "string", true, "Project name to remove."),
            arg!("path", "string", false, "One root to remove instead of the whole project; removing a project's LAST root leaves it standing with no folder (name-only), its workspace bindings intact — only omitting this path deletes the project."),
        ],
        flags: [
            flag!("host", "string", "Re-scope PATH to that host's own roots: bare `--host <node>` drops the whole membership (roots included); `--host <node> <path>` drops just that one host root and leaves the membership, even at zero roots."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::project_remove,
        kind: "project", verb: "act",
    ));
    r.insert(cmd!(
        path: ["project", "list"],
        summary: "List the registered project anchor roots, with every root.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::project_list,
        kind: "project", verb: "list",
    ));
    r.insert(cmd!(
        path: ["graph", "link"],
        summary: "Record a spawned-by edge: set parentSessionId on the child session (cycle-checked). Moving the parent of a session that exists needs a door that can approve (the CLI); the MCP and A2A doors are refused.",
        args: [
            arg!("child", "string", true, "Session id of the spawned (child) session."),
            arg!("parent", "string", true, "Session id of the spawning (parent) session."),
        ],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::link,
        approves: true,
        kind: "graph", verb: "act",
    ));
    r.insert({
        let mut c = cmd!(
            path: ["session", "start"],
            summary: "Register or update a running session in state/stage/sessions.json (UPSERT; atomic; startedAt preserved on re-start).",
            args: [],
            flags: [
                flag!("id", "string", "Session id (required); its node id becomes session:<id>."),
                flag!("agent", "string", "Agent name driving the session (default claude)."),
                flag!("cwd", "string", "Working directory; the session anchors under the longest-prefix project."),
                flag!("window", "string", "Window address to record for the session. The desktop normally binds it (`session window attach`); a session with no window is still focusable by workspace."),
                flag!("parent", "string", "Spawning session id — records the spawned-by edge (cycle-checked). On a session that already exists it re-parents it, which a door that cannot approve (MCP, A2A) is refused; any door may register a new session under a parent."),
            ],
            gated: false,
            implemented: true,
            handler: crate::graph::session_start,
            approves: true,
            kind: "session", verb: "act",
        );
        // Hook-plumbing, not an operator command — a harness's own lifecycle
        // drives it, never a human typing it directly (task #101 R1). Hidden
        // from `guide.rs`'s human listing; still enumerated by schema/MCP/A2A.
        c.internal = true;
        c
    });
    r.insert({
        let mut c = cmd!(
            path: ["session", "phase"],
            summary: "Upsert the live hook phase for a session in state/stage/hooks.json (latest updatedAt wins). A `done` phase (and `exit`, `finished`, `complete`, which fold to it) is `session end`.",
            args: [],
            flags: [
                flag!("id", "string", "Session id (required)."),
                flag!("phase", "string", "Live phase to record, folded to one canonical state: working | awaiting | stopped | idle | done. `died` is not recordable here: a wrap records its child's death."),
            ],
            gated: false,
            implemented: true,
            handler: crate::graph::session_phase,
            kind: "session", verb: "act",
        );
        c.internal = true;
        c
    });
    r.insert({
        let mut c = cmd!(
            path: ["session", "end"],
            summary: "Mark a session done (state=done in sessions.json, phase=done in hooks.json); ok no-op if unknown. From outside a live harness it is refused and writes nothing (reason `harness-anchored`).",
            args: [],
            flags: [flag!("id", "string", "Session id to end (required).")],
            gated: false,
            implemented: true,
            handler: crate::graph::session_end,
            kind: "session", verb: "act",
        );
        c.internal = true;
        c
    });
    r.insert({
        let mut c = cmd!(
            path: ["session", "hook"],
            summary: "Hook door for agent harnesses: read one hook JSON from stdin and map it to a session command through the agent's profile (never exits non-zero for a payload problem). A door that cannot approve (MCP, the network door) is refused before stdin is read.",
            args: [],
            flags: [
                flag!("agent", "string", "Agent harness the payload comes from: claude (default), kimi, or pi."),
            ],
            gated: false,
            implemented: true,
            handler: crate::graph::session_hook,
            approves: true,
            kind: "session", verb: "act",
        );
        c.internal = true;
        c
    });
    r.insert(cmd!(
        path: ["spawn"],
        summary: "Spawn ANY agent command as a DETACHED conducted session that outlives this call — headless by default (re-execs `conduct --headless`), or in a real terminal with --windowed (execs $AOIDE_TERMINAL running the same conducted command) — waits briefly for it to register its control socket, and returns. Exports AOIDE_SESSION_ID to the child same as `conduct`/`wrap`. For a FOREGROUND wait-and-stream wrapper, `aoide conduct --task …` is the same wrapper without detaching. An optional --prompt is injected through the one gated injection door (`send --yes --submit`) once registration succeeds; skipped (honestly reported) if it never does. Over MCP the caller must be attested: a server launched under a sealed session spawns the child as that session's child and its --prompt is a parent-to-child send; any other MCP spawn is held whole in pending.json for `session pending approve` and nothing launches.",
        args: [arg!("command", "string", true, "The wrapped command and its args — put them after `--` so the child's own flags pass through verbatim.")],
        flags: [
            flag!("agent", "string", "Agent name for the roster (default: the command's basename)."),
            flag!("parent", "string", "Spawning session id — records the spawned-by edge (passed through to `conduct`)."),
            flag!("id", "string", "Session id override (default spawn-<pid>-<unixts>). Over MCP it must be plain (A-Z a-z 0-9 . _ : -, at most 128 characters) and must not name a live session or an existing control socket."),
            flag!("prompt", "string", "A first turn to inject once the session registers (skipped, honestly reported, if it never does)."),
            flag!("windowed", "bool", "Open a real terminal (from $AOIDE_TERMINAL, a whitespace-split argv with a `{cmd}` placeholder) instead of a detached headless child. A bare `{cmd}` splices the conducted argv as separate arguments (`kitty -e {cmd}`); a quote-wrapped `'{cmd}'` joins it shell-quoted into one word for `sh -c` templates (`foot sh -c '{cmd}'`). Taught errors when unset, or when no display is present."),
            flag!("cwd", "string", "Working directory for the spawned child (default: this process's own cwd) — for --windowed, the terminal emulator's own cwd, which its own shell inherits."),
            flag!("undying", "bool", "Mark the spawned session durable in state/undying.json once it registers (no-op if it never does) — the same mark `session grant undying on` sets, so this project's whole undying set can later be resurrected together.", doors: aoide_protocol::registry::CLI_DOORS),
            flag!("task", "string", "MANAGED TASK WRAPPER MODE: the task's slug, which is also its mailbox name (^[a-z0-9][a-z0-9-]*$, the same predicate `mail send` applies). Refused while another live session already holds the slug. The run's instructions, output, mail and exit report are readable with `session watch <id>`; the report is filed to self/<slug> when the run ends. Requires no new daemon or store."),
            flag!("report-to", "string", "The mailbox this run's exit report is addressed to (^[a-z0-9][a-z0-9-]*$), when the report must outlive the parent's session. Omitted: the run's parent session id when that is itself a legal mailbox name, else the documented role mailbox `conductor` — an id that is NOT a legal mailbox name is never rewritten into one, because two ids could then collide. The child's own inbox (self/<task>) never carries the report."),
            flag!("timeout", "int", "Wall-clock deadline in seconds for the run (omitted = no deadline). It is the SUPERVISOR's clock, never inactivity: a silent child is alive until the deadline and a child printing continuously is still killed at it. On expiry the wrapper kills the direct child IT spawned and records outcome=timeout, with the post-kill status kept as secondary evidence — its own kill is never reported as the child's exit code."),
            flag!("instructions", "string", "The task's original instruction text, stored once beside the run's log (state/sessions/<id>.instructions.md, mode 0600) and handed to the child as AOIDE_TASK_INSTRUCTIONS. Three forms: literal text, @<path> to read a file, or - to read this command's stdin. Stored, never injected — a first turn is still --prompt's job, and mail text is never injected either. Carries no credentials and no environment."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_spawn,
        examples: [
            "spawn --agent codex -- codex --model x",
            "spawn --task fix-flaky --instructions @brief.md -- claude",
        ],
        brief: "Spawn an agent command as a detached, conducted session.",
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        approves: true, launches: true,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["resurrect"],
        summary: "Revive sessions. BARE (no --project/--all/--id): walk up from cwd for the nearest .aoide/project.json and revive THAT manifest's specs directly — no projects.json registration needed. Each local-host spec (a spec whose host doesn't match this host's own name is skipped, remote summoning lands in a later phase) enriches off the newest matching state/session-ledger.jsonl entry (same cwd/agent) through the exact --id path below when one exists, else clean-spawns windowed from the spec's own command or the agent's default launch. WITH --project (resolved against projects.json by exact name; --all/--id ignore the manifest entirely): anchors ledger entries to it (longest-prefix, same rule the bare `graph` render uses), then selects — bare (no --all/--id) resumes the project's WHOLE undying set (state/undying.json, `session grant undying on|off`) minus any id already alive; --all widens to every anchored entry; --id narrows to one. Each candidate resolves through two arms: a harness with a verified resume argv spawns windowed running `<harness> --resume <id>`; a conducted TERMINAL (no harness profile, but a captured `restore` snapshot) spawns windowed running its login shell, then — once registered — types its last foreground command at the prompt (`--yes`, never `--submit`: a human presses Enter; only when it was demonstrably running one, and not when the restore refuses it, as for a recorded `sudo …`); an idle one restores its cwd alone and delivers nothing. Neither arm resolving is skipped with a taught message. The revived session always mints a NEW sessionId (ids are never recycled) and is stamped resumedFrom, rendered as a `resumed` graph edge; an undying old id transfers its mark onto the new one. A windowed-spawn failure (no $AOIDE_TERMINAL / no display) is folded into `failed` rather than erroring the command, so a headless host degrades gracefully. Over MCP the caller must be attested to revive: with no sealed session (and always from the A2A door) `--project …` is held whole in pending.json for `session pending approve` and nothing launches, and BARE mode is refused (it would run the manifest's commands from a directory the approver never sees). --dry-run lists the same rows and launches, holds and types nothing. From a door that cannot approve, an unattested caller's dry run gets what the call would give it: bare mode's refusal, a usage error without --project, and with --project a refusal (dry-run-not-held), since the call would be held and show that caller nothing.",
        args: [],
        flags: [
            flag!("project", "string", "Project name to resurrect a session for; resolved against projects.json by exact name. Omit entirely, along with --all/--id, to use bare-manifest mode instead (walk up from cwd for .aoide/project.json)."),
            flag!("all", "bool", "Resurrect every anchored, resumable ledger entry instead of just the single most recent. Requires --project; ignores any .aoide/project.json."),
            flag!("id", "string", "Resurrect one specific ledger sessionId instead of the most recent (mutually exclusive with --all; --id wins if both given). Requires --project; ignores any .aoide/project.json."),
            flag!("dry-run", "bool", "List the rows this resurrect would act on, launching, holding and typing nothing. Each row names its disposition, and pickedByBare marks the rows the bare --project form would revive. The handler audits it with status dry-run, beside the line the door writes for every call. Where the call would be held for an approval, the dry run is refused instead."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_resurrect,
        examples: [
            "resurrect",
            "resurrect --project aoide",
            "resurrect --project aoide --all",
        ],
        brief: "Revive sessions from the project's record.",
        approves: true, launches: true,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["send"],
        summary: "Inject text into a conducted session's control socket (the one gated injection door). Held pending approval by default; --yes (or an autogate policy) delivers and auto-renames the node to a one-line form of the text — except for a bare keystroke answer (text with no letters, e.g. a permission verdict digit), which is not a task and leaves the node's name alone. Siblings (sharing a live parent) autogate each other by default too — opt out with AOIDE_CONDUCT_SIBLING_AUTOGATE={0,false,no}. Over MCP the same parent and sibling rules apply to the sealed session the MCP server runs under, found the way the CLI finds its own; --yes and the autogate environment never apply, and a send with no sealed session behind it is held. The reciprocal also holds: a parent automatically hears the children it spawned — the daemon delivers ONE line off a child's own trace (settled, cancelled, died mid-turn, asking, wrapping up, failing, silent) straight to the parent's transport, never prompted, never pending, and never through this door. Every outcome is audited. --to resolves a name (local id/tail4/petname, or node/<query> for a remote session over A2A) instead of a raw --id; mutually exclusive with --id — a remote send is always attempted (the receiving node gates its own delivery) and never queues locally.",
        args: [arg!("text", "string", true, "The text to inject — put it after `--` so its own words/flags pass through verbatim.")],
        flags: [
            flag!("id", "string", "Target session id (required unless --to is given); its socket is resolved from sessions.json.", value: "session-id", conflicts: &["to"]),
            flag!("submit", "bool", "Append the target harness's own submit keystroke (Enter for most agents, \\r for kimi — resolved from the target session's agent profile at delivery time). No-op for a --to remote send (the receiving node always submits its own way)."),
            flag!("yes", "bool", "Authorise delivery now (else the send is held pending approval). Never authorises over MCP, which cannot approve. No-op for a --to remote send — the receiving node gates its own delivery.", doors: aoide_protocol::registry::CLI_DOORS),
            flag!("harness", "string", "Refuse unless the target is a wrap of this harness (claude, kimi, pi, eidolon). A shell wrap, or a wrap of another harness, is refused as `not-that-harness` and nothing is typed. Refused before the gate, so --yes does not get past it.", doors: aoide_protocol::registry::CLI_DOORS),
            flag!("from", "string", "Sender attribution override for the delivered provenance prefix (default: AOIDE_SESSION_ID). ATTRIBUTION ONLY, not authentication — unauthenticated and as spoofable as the env var it defaults from. Over MCP a send is recorded and delivered under the attested session (else the door's own source) and this rides beside it as a cleaned claim; a value spelling a source a door derives (`node:`, `addr:`, `mcp`) is refused."),
            flag!("to", "string", "Target by name instead of --id: a local session id/tail4/petname/host-role-petname line, or node/<query> to resolve against a registered node's CACHED graph and deliver over A2A message/send. Mutually exclusive with --id.", value: "target", conflicts: &["id"]),
            flag!("mesh", "string", "For a `node/<query>` target: the mesh this request acts in (P-CHARTER) — the far door reads the caller's grant in it. Absent = the node's sole granted mesh, else `[pairing] homeMesh`; required when that node is trusted in more than one mesh. Unused by a local send."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_send,
        examples: [
            "send --id <session-id> --submit -- yes, ship it",
            "send --id <session-id> --yes -- 1",
            "send --to brave-otter --yes --submit -- status?",
            "send --to yomi-strix/brave-otter -- ping",
        ],
        one_of: &[&["id", "to"]],
        brief: "Inject text into a conducted session (held for approval unless --yes).",
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        approves: true,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "pending", "list"],
        summary: "Enumerate held `send` / A2A entries in state/stage/pending.json (id is the entry's position — re-list after any approve/deny, positions shift). A malformed entry (a stale hand-edited line) is listed with state `malformed` rather than failing the whole read. The human line spells every control, bidi and invisible character as a visible escape (`\\n`, `\\x1b`, `\\u{202e}`) and shows a held spawn as its whole command; `data.pending[].text` keeps the bytes as they replay.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::pending_list,
        examples: ["session pending list"],
        kind: "hold", verb: "list",
    ));
    r.insert(cmd!(
        path: ["session", "trace"],
        summary: "Render a session's TRACE, one line per record — the run, step by step (docs/architecture/EIDOLON-TRACE.md: eidolon publishes its journal as one JSON record per line — as a `<log>.jsonl` mirror the installed generation writes, or through its own read-only export door `eidolon log --json <journal> [--after <id>]` — and Aoide resolves it through the record's presence metadata; it is the one harness that does today). <id> resolves like `send --to` (id / tail4 / petname). Human form is `#<id>  <hh:mm:ss local>  <kind>  <summary>` — an assistant message shows its thinking (dimmed, cut) then its text then each `→ tool(name)`, a tool result shows its first line prefixed `!` when it errored, a settled turn shows its stop reason and tokens. --tail N shows the last N records (default 50); --follow re-reads for new lines until Ctrl-C (CLI-only); --json carries `data.lines`, the journal's own JSON lines: raw unsafe characters removed, JSON escapes kept, so a decoded line holds untrusted strings and a consumer shows the steps; PLUS `data.steps`: the same window PROJECTED, one object per emitted content block in the record's own order (`{id, ts, kind, text, error, clipped}`, kind in thinking/say/tool/result/settled/user/other), bounded by the window, by blocks-per-record and by a whole-projection cap (`stepsOmitted` names the last one's count); --clip line|detail sets how much of each block `steps` keeps. A session whose harness keeps no trace (or whose presence names none) is a taught error naming which of the two it is, never an empty listing. Read-only: no stage write, no daemon, no lock.",
        args: [arg!("id", "string", true, "Session to render: a local session id, its tail4, or its petname (resolved by the same resolver `send --to` uses).")],
        flags: [
            flag!("tail", "int", "Show only the last N records (default 50). A non-numeric or zero value is a usage error, never a silently empty listing."),
            flag!("clip", "string", "How much of each emitted block `--json`'s `data.steps` carries: `line` (default — each block on one clipped line, the same spelling the human body shows) or `detail` (the block's own line breaks, kept longer, plus a tool call's arguments, for a details view). An unknown word is a usage error, never a silent widening. `data.lines` is unaffected: it stays the whole records."),
            flag!("follow", "bool", "Re-read the trace every 500ms and print new records as they land; blocks until Ctrl-C. CLI-only — a follow that parks a connection makes no sense over MCP/A2A/the daemon socket.", doors: &[Door::Cli]),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_trace,
        examples: [
            "session trace brave-otter",
            "session trace 7d23 --tail 20",
            "session trace Aoide-7d23 --follow",
            "session trace brave-otter --json",
        ],
        kind: "session", verb: "watch",
    ));
    r.insert(cmd!(
        path: ["session", "chat"],
        summary: "Read a session's conversation as chat lines, one per message, the same shape for every harness (CONTRACTS.md \"The chat line — v1\"): the agent's text and thinking (thinking flagged), its tool calls and their results, peer and tool messages as system lines, the state (waiting, settled) and a Claude permission prompt as an ask line (its fingerprint is the permit card's received_at). The source is read through the harness's own chat adapter; a harness with none (kimi, pi) is refused with the reason. <id> resolves like `session trace` (id / tail4 / petname). --follow prints new lines until Ctrl-C, then a closing envelope; each tick re-reads the session's record, so a /clear or /resume that moves it to another transcript keeps the chat. Terminal and daemon doors only (the default doors): A2A and MCP do not reach it, and a remote `--to` is not resolved. Read-only: no stage write, no lock.",
        args: [arg!("id", "string", true, "Session to read: a local session id, its tail4, or its petname (resolved by the same resolver `session trace` uses).")],
        flags: [
            flag!("follow", "bool", "Print new chat lines as they land, until Ctrl-C. CLI-only — a follow that parks a connection makes no sense over the daemon socket.", doors: &[Door::Cli]),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_chat,
        examples: [
            "session chat brave-otter",
            "session chat Aoide-7d23 --follow",
            "session chat brave-otter --json",
        ],
        kind: "session", verb: "watch",
    ));
    r.insert(cmd!(
        path: ["session", "watch"],
        summary: "Watch a managed task run (`snapshot`/live): the instructions it was started with, its live output, the CHILD's own unread queue (the letters sent TO the subagent), and where it stands — LIVE by default, until the run ends or Ctrl-C. --snapshot returns ONE frame instead (the bounded mode a door that must return asks for); --tail N widens the output window (default 50); --json returns the frame in the registry's structured envelope; live, --json prints one JSON object per line, each tagged by `type` (`frame`, `output`, `mail`, `read`, `end`), then the outcome envelope on one line, so every stdout line is one JSON object. Read-only by construction: no stage write, no lock, no cursor — mail is read with `mail::unread_for` (a reader-selected PEEK), so watching advances neither the child's nor the observer's unread state, and the run's own report never lands in the child's inbox (it goes to its report mailbox). The footer's report line is read from the report lane's cursor, never inferred from a letter; a process exit is never rendered as task success, and a killed run's absent exitCode is reported as absent, never 0. Refusals: an unknown id, a `sub:` card, and a record that keeps no conduct-owned PTY.",
        args: [arg!("id", "string", true, "Session to watch: a local session id, its tail4, or its petname (resolved by the same resolver `send --to` uses).")],
        flags: [
            flag!("tail", "int", "Show the last N lines of the run's output (default 50). A non-numeric or zero value is a usage error, never a silently empty view."),
            flag!("snapshot", "bool", "Print ONE frame and return, instead of following. The only bounded mode: live following is the default, and a door that must return (MCP/stdin) is told to ask for this rather than being silently narrowed."),
            flag!("json", "bool", "Structured output (sanitized), instead of the rendered block: with --snapshot, the frame in the registry's envelope; live, one JSON object per line, each tagged by `type`."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_watch,
        examples: [
            "session watch task-fix-flaky",
            "session watch fix-flaky --tail 200",
            "session watch fix-flaky --snapshot",
            "session watch fix-flaky --snapshot --json",
        ],
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        kind: "session", verb: "watch",
    ));
    r.insert(cmd!(
        path: ["session", "pending", "approve"],
        summary: "Approve one held pending entry: re-drive it through the one gated injection door (`send`, in-process, --yes) and remove it from the queue. A held spawn whose `--id` has since gone live is refused and stays queued. A malformed or out-of-range id fails cleanly, leaving the entry untouched.",
        args: [arg!("id", "string", true, "Pending entry id — its position from `session pending list`.")],
        flags: [flag!("expect", "string", "Fingerprint of the entry to approve, as `session pending list --json` prints it (its `fingerprint`). Approve refuses without it: a bare index is refused before anything is taken, and the refusal prints the entry and this command with its fingerprint. The entry is taken only while it still matches; a stale one takes nothing and says to list the queue again.")],
        gated: false,
        implemented: true,
        handler: crate::graph::pending_approve,
        examples: ["session pending approve 0 --expect <fingerprint>"],
        approves: true, launches: true,
        kind: "hold", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "pending", "deny"],
        summary: "Reject one held pending entry: remove it from the queue and inject nothing. A door that cannot approve (MCP, the network door) is refused, as for approve. A malformed or out-of-range id fails cleanly, leaving the entry untouched.",
        args: [arg!("id", "string", true, "Pending entry id — its position from `session pending list`.")],
        flags: [flag!("expect", "string", "Fingerprint of the entry to deny, as `session pending list --json` prints it (its `fingerprint`). Deny refuses without it: a bare index is refused before anything is removed, and the refusal prints the entry and this command with its fingerprint. The entry is removed only while it still matches; a stale one removes nothing and says to list the queue again.")],
        gated: false,
        implemented: true,
        handler: crate::graph::pending_deny,
        examples: ["session pending deny 0 --expect <fingerprint>"],
        approves: true,
        kind: "hold", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "permit"],
        summary: "Publish the notifications' permission SUMMONS for a session blocked on a permission prompt. The card is filed into the notifications ledger (stage/notifications.json) and this command RETURNS — the Quickshell notifications draws it with real approve/deny buttons, and the click routes back through the shellbridge to type the verdict in. The hook door raises it automatically when a session goes `awaiting`. Only ever raised for a conductable session whose harness has verified prompt keys, and only while that session is awaiting. A door that cannot approve (MCP, the network door) is refused and files no card. The verdict is only typed while the session is still awaiting.",
        args: [],
        flags: [
            flag!("id", "string", "Target session id (required); its socket is resolved from sessions.json."),
            flag!("tool", "string", "Tool the permission is being asked for — the card's title tier."),
            flag!("what", "string", "One line describing the ask — the card's context tier (rendered as plain text)."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_permit,
        approves: true,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "prune"],
        summary: "Drop `done` sessions and their hook records; clear orphaned parentSessionId links.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::prune,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "reap"],
        summary: "Reap dead sessions: mark every KILLED session (its pid gone from /proc) done and drop it, decay every `stopped` session at rest over an hour to `idle`, then re-stage. Also collects the three ghosts no liveness signal catches — a record whose every timestamp predates this boot (a recycled pid reads as alive forever), a sub-agent whose parent has left the roster, and the control socket a killed `conduct` left in $XDG_RUNTIME_DIR (only ever one nothing is listening on). Plus the worker shells `aoide spawn` left running that no one has typed in for two days — see `--now`, which takes those on the spot. Judged on pid-only liveness, on every host; a window the desktop reports closed is judged by `session window closed`, which ends the sessions that ran in it. Never errors on nothing-to-reap.",
        args: [],
        flags: [
            flag!("announce", "bool", "Always raise the desktop toast, even on a quiet pass — for the dock's reap control, where a human pressed something and is owed an answer. Unflagged, the sweep only toasts when it actually changed the roster."),
            flag!("now", "bool", "Take every idle worker shell `aoide spawn` left running, however recently it was touched — the unattended sweep otherwise waits out a two-day silence on the shell's pty log. Implied by a human gesture: the dock's reap control passes it, and a bare `aoide session reap` typed at a terminal picks it up. No other band moves."),
        ],
        gated: false,
        implemented: true,
        handler: crate::reap::reap_and_announce,
        examples: [
            "session reap",
            "session reap --announce",
            "session reap --now",
        ],
        kind: "session", verb: "act",
    ));
    // ── conduct: the PTY-backed conductable wrap (concepts/Conductor-Channel) ─
    r.insert(cmd!(
        path: ["conduct"],
        summary: "Run an agent command on its own PTY as a CONDUCTABLE session (a door that cannot approve, MCP and A2A, is refused: it launches through `spawn`): spawn, register running, wait, end (exit mirrored, AOIDE_SESSION_ID exported), with a controlling tty + a per-session control socket, so `send` can type into the running agent while its TUI runs undisturbed. WITH --task it is also the FOREGROUND managed-task wrapper: it blocks, streams the child's own output, prints the task's instruction/unread context, and returns one deterministic outcome (exit | signal | timeout | stopped) — the same wrapper `spawn --task` runs detached.",
        args: [arg!("command", "string", true, "The wrapped command and its args — put them after `--` so the child's own flags pass through verbatim.")],
        flags: [
            flag!("agent", "string", "Agent name for the roster (default: the command's basename)."),
            flag!("parent", "string", "Spawning session id — records the spawned-by edge."),
            flag!("id", "string", "Session id override (default conduct-<pid>-<unixts>)."),
            flag!("headless", "bool", "No controlling tty: never touch the real terminal (no raw-mode, no stdin shuttle), and mirror the pty's output to state/sessions/<id>.log (logPath on the record) instead of stdout."),
            flag!("spawned", "bool", "Mark the record as created by `aoide spawn` (spawned on the record), permanently. Set by spawn's own re-exec in both launch modes; reap's abandoned-shell sweep judges only records carrying it."),
            flag!("task", "string", "MANAGED TASK WRAPPER MODE: the task's slug (also its mailbox name), stamped on this session's own record at registration — set by `spawn --task`'s re-exec, and exportable to the child as AOIDE_TASK. The child is enrolled as that mailbox's reader, so it shows what was sent TO the subagent."),
            flag!("instructions-path", "string", "Absolute path of this run's write-once instruction sidecar (state/sessions/<id>.instructions.md), stamped on the record at registration and exported to the child as AOIDE_TASK_INSTRUCTIONS. Set by `spawn --instructions`'s re-exec, or given directly for a foreground run."),
            flag!("report-to", "string", "The mailbox this run's exit report is addressed to (a legal mailbox name), for when the report must outlive the parent's session. Omitted: the parent session id when that is a legal mailbox name, else the role mailbox `conductor`; an id that is not a legal name is never rewritten into one."),
            flag!("timeout", "int", "Wall-clock deadline in seconds for THIS foreground wrapper (omitted = no deadline). Never inactivity: quiet output does not shorten it, and continuous output does not extend it. On expiry the wrapper kills the direct child it spawned and reports outcome=timeout — never an exit code for its own kill. A `spawn --timeout` re-execs with the same flag, so both shapes are one wrapper."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_conduct,
        brief: "Run an agent command on its own PTY as a conductable session.",
        approves: true, launches: true,
        kind: "session", verb: "act",
    ));
    // ── bare `session`: the ROSTER (session-surface redesign, command-defrag
    // lane X, 2026-08-28 — supersedes both the U3 undying picker that used
    // to live here AND the standalone `aoide who` command, folded away
    // entirely). Registered here, at the END — same parent-command pattern
    // R1 established for bare `graph` (path ["graph"] alongside ["graph",
    // "link"]) — registration order matters (`crates/AGENTS.md`), and this
    // landed after every entry above it.
    r.insert(cmd!(
        path: ["session"],
        summary: "Roster listing: this box's own sessions plus every registered node, probed in parallel on every call (the retired `aoide who` command's exact probe/cache-fallback pipeline). Bare groups sessions by PROJECT (a local session by a registered projects.json name, else a .aoide/project.json manifest directory's own basename; a session from another node only by the project that node published; else a trailing (no project) bucket); --hosts groups by HOST instead — this host, then each node, byte-identical to `who`'s old rendering. --json mirrors either grouping structurally.",
        args: [arg!("filter", "string", false, "Narrow what's displayed (never what's probed): a local session id/tail4/petname, a host/role/petname line, node/<rest>, or a plain substring against a node/session name.")],
        flags: [
            flag!("hosts", "bool", "Group by host instead of by project — the retired `who` command's own grouping."),
            flag!("all", "bool", "Also list `done` sessions (omitted by default)."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_roster,
        examples: [
            "session",
            "session --hosts",
            "session --hosts --all",
        ],
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        kind: "session", verb: "list",
    ));
    // ── session grant: the GRANT family (session-surface redesign,
    // command-defrag lane X) — a POSITIONAL <kind> grammar (`secrets
    // automate <name> on|off` style) replacing the old standalone `session
    // undying` command, which this absorbs and retires (hard cutover, no
    // alias). Two kinds today: `undying` (U1/U3's mark, relocated verbatim)
    // and `exempt` (task #20, the reaper's staleness safety valve — no
    // picker). Registered last — landed after every entry above it.
    r.insert(cmd!(
        path: ["session", "grant"],
        summary: "Grant (or open a picker to grant) a session capability. Bare (no <kind>) is refused naming the grantable kinds; an unknown kind is a taught refusal. `undying`: with no state, opens the interactive PICKER on a real CLI terminal — a multi-select over this box's own sessions plus every registered node's CACHED sessions (no live pulls), each row pre-checked by its current undying state, confirmed in one Enter (non-tty/non-CLI/--json steers to the scripted form below instead); with on|off, marks or unmarks a session as durable in state/undying.json directly, so a project's whole undying set can later be resurrected together — --id targets any session id directly, including one already gone from the roster. `exempt`: no picker (bare `exempt` is a taught refusal naming the scripted form); on|off vetoes the reaper's staleness judgments for a LIVE session only (never its window-gone/pid-gone/ghost/orphan signals, and never `--now`, which waives only the abandoned-shell band) — --id must name a session currently on the roster, since an exemption has nothing to mean once the record is gone. Bare and --self both resolve the target from $AOIDE_SESSION_ID for either kind.",
        args: [
            arg!("kind", "string", true, "The grant kind, one of undying, exempt."),
            arg!("state", "string", false, "The state to set, `on` or `off`. For `undying` only, omit to open the interactive picker instead — `exempt` has none."),
        ],
        flags: [
            flag!("self", "bool", "Target this session, resolved from $AOIDE_SESSION_ID (the default when neither --self nor --id is given)."),
            flag!("id", "string", "Target session id directly (mutually exclusive with --self). For `undying`, no roster lookup gates it, so a dead id is a valid target; for `exempt`, the id must currently be on the roster — an off-roster id is a refusal."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_grant,
        examples: [
            "session grant undying",
            "session grant undying on --self",
            "session grant undying off --id <session-id>",
            "session grant exempt on --self",
            "session grant exempt off --id <session-id>",
        ],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "window", "attach"],
        summary: "Record the terminal window a session runs in. A session with no window takes this one; the same window again only moves its workspace; a different window is refused while the first one is held. The desktop's window sweep is the caller; requires aoided.",
        args: [
            arg!("id", "string", true, "Session id to attach the window to."),
            arg!("address", "string", true, "The window address the compositor reports, e.g. 0x55d0c2a1."),
        ],
        flags: [
            flag!("workspace", "string", "Workspace id the window is on now (an integer). Moves the session's workspace; the birth default project is never re-stamped."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_window_attach,
        examples: ["session window attach s-1 0x55d0c2a1 --workspace 3"],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "window", "closed"],
        summary: "Record that a terminal window closed. The sessions that ran in it end now, except one a live harness or a reaper protection still anchors, and every session that claimed the window lets go of it. A session whose pid is still alive ends too: a terminal outlives the agent in it. The desktop's window listener calls this once per closed window; requires aoided.",
        args: [
            arg!("address", "string", true, "The window address the compositor reports as closed, e.g. 0x55d0c2a1."),
        ],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::session_window_closed,
        examples: ["session window closed 0x55d0c2a1"],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "window", "roster"],
        summary: "Publish the terminal windows the desktop sees. The window list is read from stdin as a JSON array of window objects (address, class, title, workspace, pid, mapped, cwd); every terminal window no tracked session claims gets a `win:*` record, and a `win:*` record whose window is gone is removed. Idempotent: an unchanged roster changes nothing. Requires aoided.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::session_window_roster,
        examples: ["session window roster"],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "codex", "sync"],
        summary: "Reconcile the desktop's codex threads now: enroll the live ones, drop the ones that have stopped, and fill in their titles. The reaper runs the same reconcile on every pass; the desktop calls this when a window appears and on each reconnect, so a thread enrolled just now is stamped with its window in the same pass. Requires aoided.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::session_codex_sync,
        examples: ["session codex sync"],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "show"],
        summary: "Show one local session's window and workspace, read-only: the facts the desktop's focus verb needs. `<id>` is an exact session id, as `session find` prints it. No stage write, no lock, no daemon. Refused as `session-not-found` when no local record carries the id.",
        args: [arg!("id", "string", true, "Exact local session id.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::session_show,
        examples: ["session show 7e3f5976-98b2-44a4-827c-c687a0d9526e"],
        kind: "session", verb: "show",
    ));
    r.insert(cmd!(
        path: ["session", "bind"],
        summary: "Bind a local executor to an explicit enduring agent key; idempotent, refuses a conflicting binding, requires aoided. Does not grant access or require Mneme.",
        args: [],
        flags: [
            flag!("id", "string", "Existing session id (required)."),
            flag!("agent-id", "string", "Opaque enduring key (required): lowercase letters, digits, and hyphens; starts with a letter or digit. Independent of display names and harness sessions."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_bind,
        examples: ["session bind --id executor-1 --agent-id 7e3f5976-98b2-44a4-827c-c687a0d9526e"],
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["project", "edit"],
        summary: "Replace a project's anchor roots outright (the name is immutable; autoResume is untouched).",
        args: [
            arg!("name", "string", true, "Project name to edit; must already be registered."),
            arg!("path", "string", true, "The project's new root path; give one or more. The first becomes the project's primary root, the rest follow in order."),
        ],
        flags: [
            flag!("host", "string", "Re-scope the path list to REPLACE that host's own roots exactly, instead of the local ones — local roots and every other host stay untouched. Upserts the membership if it wasn't already one."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::project_edit,
        examples: [
            "project edit aoide ~/Aoide ~/Aoide-docs",
            "project edit aoide ~/Aoide",
        ],
        kind: "project", verb: "act",
    ));
    r.insert(cmd!(
        path: ["project", "lead"],
        summary: "Place one session directly under the project; every other parentless session of the project hangs off it.",
        args: [
            arg!("name", "string", true, "Project name; must already be registered."),
            arg!("session", "string", false, "Session id to lead the project; must be in the roster. Omit with --none to clear."),
        ],
        flags: [
            flag!("none", "bool", "Clear the project's lead."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::project_lead,
        examples: [
            "project lead aoide 7e3f5976-98b2-44a4-827c-c687a0d9526e",
            "project lead aoide --none",
        ],
        kind: "project", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "find"],
        summary: "Find sessions on this box and on every registered node, from the stage and each node's cached graph. Reads files only: it probes no node, takes no lock and writes nothing; `node pull` refreshes a cache. Each word is a term (`key:value`, keys agent, kind, state, node, project, role, mailbox, cc, parent) or a bare word matched against petname, task, title, id and cwd. Every word must match. Rows sort by the weakest bare word's grade, then the sum of grades, local before cached, state urgency, then the label. `--related <ref>` shows <ref>'s carries and the sessions that share a fact with it (lineage, sibling, role, mailbox, thread, project, repo), each with its reasons; the query words then narrow those. A cached row reads `was <state>` and its mailboxes and cc are unknown: its node does not publish them. The scope line names where each node's rows came from and how old its cache is.",
        args: [arg!("query", "string", false, "Query words, one per shell argument or daemon array element; MCP and A2A pass them as one string. Quote a value with spaces: `cc:\"pr 41\"`. With --related these narrow the related rows.")],
        flags: [
            flag!("related", "string", "Show <ref>'s carries and the sessions related to it. <ref> resolves like `send --to`, and `node/<rest>` names a cached row."),
            flag!("all", "bool", "Also list `done` sessions (omitted by default)."),
            flag!("json", "bool", "Carry the same facts in `data`: query, scope, sessions (with carries, unknown and matched terms) and related. Every door but the CLI carries them without this flag."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::session_find,
        examples: [
            "session find brave",
            "session find agent:kimi state:working",
            "session find role:review node:yomi",
            "session find mailbox:reviewer",
            "session find cc:\"pr 41\"",
            "session find parent:warm-birch",
            "session find --related brave-otter",
        ],
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
        kind: "session", verb: "list",
    ));
}

/// `mail ring` (P-M5a-2, MAIL.md "Delivery and the doorbell") — registered
/// separately from [`register`] because its home command family (`mail`)
/// lives in the CLIENT crate's `commands.rs`, which cannot depend on this
/// crate; `aoide-cli`'s own `commands::all()` calls this alongside
/// `aoide_client::commands::register`/`aoide_client::context::register`
/// instead, at the end of that function, so the doorbell's one command
/// slots in last rather than reordering the client's own `mail` family.
pub fn register_mail_ring(r: &mut Registry) {
    r.insert(cmd!(
        path: ["mail", "ring"],
        summary: "Ring every armed reader of a mailbox that is headless and at the prompt; latched until the reader reads. Local, runs in-process.",
        args: [],
        flags: [
            flag!("for", "string", "Mailbox name to ring (required)."),
            flag!("from", "string", "The filer's own session — excluded from the ring, never rung for its own letter."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::mail_ring,
        examples: ["mail ring --for claude-mail"],
        approves: true,
        kind: "mailbox", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "project"],
        summary: "Assign a local session to a registered project without changing its cwd; --clear drops the explicit choice, so the ladder applies again (the session's workspace default, else its cwd anchor).",
        args: [],
        flags: [flag!("id", "string", "Exact local session id."), flag!("project", "string", "Registered project name."), flag!("clear", "bool", "Drop the explicit project: the session's saved workspace default (if it was born on a bound workspace) then its cwd anchoring applies again.")],
        gated: false,
        implemented: true,
        handler: crate::graph::session_project,
        kind: "session", verb: "act",
    ));
    r.insert(cmd!(
        path: ["session", "kill"],
        summary: "Send SIGTERM to the verified local session-owning process. Refuses shared, desktop, unsealed, stale, or unsupported targets. Does not mark completion; the reaper observes exit.",
        args: [],
        flags: [flag!("id", "string", "Exact local session id.")],
        gated: true,
        implemented: true,
        handler: crate::graph::session_kill,
        kind: "session", verb: "act",
    ));
    // The workspace ↔ project binding (core-seams §B). Appended last: the
    // registry's order is byte-stable, so a family that is not moving a
    // binary lands at the tail and the sort slot is the golden test's own
    // sorted list.
    r.insert(cmd!(
        path: ["workspace", "set"],
        summary: "Bind a workspace to a project: sessions BORN on that workspace join it. The workspace id is required; a desktop that means its focused workspace resolves it first.",
        args: [
            arg!("workspace", "integer", true, "Workspace id (an integer, the same value sessions.json holds)."),
            arg!("project", "string", true, "Registered project name to bind. A project may have no folder; with --new an unregistered name is created name-only and bound in one call."),
        ],
        flags: [
            flag!("new", "bool", "Create the project (name-only, no folder) when the name is not registered, then bind it; on a name that IS registered it just binds — it never edits that project. Never implicit — a typo must refuse, not register."),
        ],
        gated: false,
        implemented: true,
        handler: crate::graph::workspace_set,
        examples: [
            "workspace set 3 aoide",
            "workspace set 4 cadenza --new",
        ],
        kind: "workspace", verb: "act",
    ));
    r.insert(cmd!(
        path: ["workspace", "clear"],
        summary: "Unbind a compositor workspace. Sessions already born on it keep the default project they were stamped with — the binding is a birth default, not a live link.",
        args: [arg!("workspace", "integer", true, "Workspace id to unbind.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::workspace_clear,
        examples: ["workspace clear 3"],
        kind: "workspace", verb: "act",
    ));
    r.insert(cmd!(
        path: ["workspace", "list"],
        summary: "Every workspace binding plus every workspace a session on this host reports, sorted by id; --json is the shape a widget pad reads.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::workspace_list,
        examples: ["workspace list", "workspace list --json"],
        kind: "workspace", verb: "list",
    ));
    r.insert(cmd!(
        path: ["workspace", "root"],
        summary: "Print the first folder of the project bound to a workspace, for a launcher to open a terminal in — one bare path on stdout, nothing at all when it refuses.",
        args: [arg!("workspace", "integer", true, "Workspace id.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::graph::workspace_root,
        examples: ["workspace root 3"],
        kind: "workspace", verb: "show",
    ));
}

/// `mail` (bare) — the first of the MCP-open mail reads. The declaration is
/// the client's; the handler is conduct's, because the MCP clip is. Called by
/// `aoide-cli`'s `commands::all()` before the client's `mail send`, so the
/// CLI's order is the one it always had.
pub fn register_mail_names(r: &mut Registry) {
    aoide_client::commands::register_mail_names(r, crate::graph::mail_names);
}

/// `mail read`, `mail show`, `mail mark` — the MCP-open reads, registered after
/// `mail send` for the same order reason as [`register_mail_names`].
pub fn register_mail_reads(r: &mut Registry) {
    aoide_client::commands::register_mail_reads(r, crate::graph::mail_read, crate::graph::mail_show, crate::graph::mail_mark);
}
