# aoide-conductor

`aoide conductor` is Aoide's terminal workspace for projects, agents,
terminals and correspondence. It is a ratatui/crossterm frontend over the
existing dispatcher and stores, usable without a desktop session.

## Working in the conductor

Home opens as a full page with Aoide's logo outside a continuous light
livery surface containing the central actions and recent projects, including
the gaps between them. The surface has three columns of side padding and
vertical padding around its controls, with darker outer margins. The logo
is centered as one preformatted block, preserving its internal alignment. Arrows or `h` / `j` / `k` / `l` select an action or
recent project; Enter opens it. `H` opens connected hosts and `L` opens Activity. Shared surfaces use even shading and single separator
lines; bright selection highlights identify only the keyboard-focused region,
while other selections retain subdued shading. Workspace views expose a project sidebar whose Agents,
Terminals and Past groups fold independently. Past includes durable session
ledger entries, not just ended records still retained in the current stage.
Sessions belonging to no project gather in an `Active sessions` group holding
only live ones; their ended sessions form the one Past node at the root of the
tree, below every project.
Opening a historical entry displays its recorded facts; it does not focus,
kill or resurrect a process. A scrollbar appears at the tree's right edge
whenever it holds more rows than fit, and stays gone otherwise.

| Key | View or action |
|---|---|
| `1` / `2` / `3` / `4` / `5` | Home / Mail / Agents / Terminals / Mesh |
| `6` / `7` / `8` / `9` / `0` | Review / Projects / Graph / Log / Status |
| Tab / Shift-Tab | Next / previous view |
| Ctrl-P | Switch focus to or from the project tree; Home opens Projects |
| Arrows or `j` / `k` | Select rows in the focused region |
| `h` / `l`, Left / Right | Fold or unfold tree groups |
| `e` / right-click | Actions for a tree, graph, project or session target |
| `a` in Graph | Whole forest, or only the selected card's own graph |
| `j` / `k`, Down / Up in Graph | Move the selection down/up a rank, toward a child or the parent |
| `h` / `l`, Left / Right in Graph | Move the selection to the previous/next sibling across the rank |
| Enter in Graph | Open or focus the selected session |
| `s` in Graph | Write a letter to the selected agent |
| `p` in Graph | Prune ended sessions |
| `?` | Context help |
| `q` / Ctrl-C | Quit outside text entry / quit globally |
| Review: `j` / `k` | Walk session approvals, pairing requests and TOTP asks |
| Review: `a` / `d` / `r` | Approve or resume / deny, reject or dismiss / re-list every queue |
| Mesh: `e` / right-click | Pair, toggle a node's read/spawn/message grant, or unregister it |
| Status: Enter or `e` | Edit the selected config key, or a secret's consumer grant |

The single-line `𝄞 CONDUCTOR` header keeps the clef on the left and anchors the
project, agent and terminal count buttons on the right. Every kind of thing has
one identity mark, defined once in `theme::mark` and used wherever that kind
appears (header buttons, tab labels, tree rows, roster rows, graph cards,
legends): `⌂` home, `✉` mail, `♜` agent, `▣` terminal, `🖧` host, `⚑` review,
`◆` project, `◇` folder, `∴` graph, `≡` log, `⚙` status, `◌` past session,
`↻` resurrect, `⊚` model, `◉` cursor. Marks are one cell wide; `@` is reserved
for addresses; a mark carries no colour of its own, its Role does, so the
conductor stays themeable through livery alone. Layout measures labels in
display cells, never bytes. Semantic colors use the livery Base16 tokens as exact RGB: projects
use base0D, agents base0E, terminals base0C and mail base09. Focused tabs use
their role color as a fill; inactive selections retain subdued livery shading.
Missing tokens fall back to ANSI colors. A single border encloses the tabs;
the hint bar and status bar use distinct livery shades without ornaments.
Controls use ASCII marks such as `[+]`, `[-]`, `[+ To]` and `[+ Cc]`;
field labels remain plain, with focus shown by shading and the cursor.

Mouse navigation selects the same tabs, tree rows, content rows and actions
as keyboard navigation. Visible action labels explain panel-specific keys.
Text entry takes precedence over navigation shortcuts. Bracketed paste inserts
text into the active field without interpreting it as commands.

Graph is a retained scene, drawn top-down: depth runs downward through ranks
and siblings spread across a rank, with a parent centred over the horizontal
span of its own children. Cards are fixed-size rectangles standing at world
coordinates the scene keeps: a refresh that adds, ends or re-parents sessions
leaves every surviving card where it was, and an arriving card takes the first
free slot beside its proposed one. A session that changed parent is the one
exception — it moves to its new rank, because a retained position would draw
a child above its parent. Selection names a card by identity rather than by
row, so the same card stays selected across a refresh.

`j` / `k` or Down / Up step the selection a rank at a time, toward a child or
the parent; `h` / `l` or Left / Right step it to the previous or next sibling
across the rank, and neither wraps at a rank's end. Enter opens or focuses
the selected session, `s` writes a letter to the selected agent directly
without opening the actions menu, and `p` prunes ended sessions — the one
mutation the panel dispatches on its own.

`a` switches the two views. Focus, the default, draws only the connected graph
the selected card belongs to; All draws the whole forest. Sessions belonging to
no project gather under one synthetic root, and that root is not a connection:
a session attached to nothing shows itself alone under Focus rather than
borrowing a forest of strangers.

Space + left drag or middle drag pans the canvas; the wheel pans vertically and
Shift + wheel pans horizontally. Ctrl + wheel zooms the camera through 50%,
75%, 100%, 125% and 150%, anchored at the pointer. Until a drag or a pan moves
it, the camera follows the selection by centring the selected card. The forest
sits on a padded canvas, so the camera reaches past the outermost cards. Only
what the camera can see is painted.

Wires run below the cards, so a card covers the wire that crosses it. Each wire
wears the state colour of the session it leads to, so a working agent lights
its own connections; project trunks keep the accent. Cards carry their identity
mark in the heading and no port glyphs. Zoom transforms that same world rather
than choosing a different card preset. Terminal glyphs stay cell-sized, so
labels clip within their visible boxes. Rendering, selection and mouse hits
share one camera transform, so a click and a key resolve the same card.

Actions open a target-specific menu. Arrows or `j` / `k` select an entry,
Enter applies it and Escape closes the menu.

| Target | Available actions |
|---|---|
| Every target | Details |
| Live local session | Open / focus; Set project (blank restores automatic attribution); Lead project (its effective project, so that project's other roots nest beneath it) |
| Live local agent with a mailbox petname | Write letter |
| Project | Write letter recipient chooser; Add folder; Resurrect its existing undying set |
| Historical session with a registered project and native session ID or restore snapshot | Resurrect that exact session |
| Registered node (Mesh) | Pair / re-pair; toggle read, spawn, message; Unregister node (exact name) |
| Node named by mesh drift, with no local record | Pair / re-pair only |
| Config key (Status) | Edit value |
| Secret reference (Status) | Grant consumer; Revoke consumer |

Historical actions never focus a stale process. The menu retains the exact
selected target through dispatch. Rename and Kill are not context actions. A
node menu carries the `allows` set it opened with, so a toggle states the
change it showed, and a node this box holds no record of is never offered a
grant or a removal the registry would refuse.

Projects use the existing project registry and its multiple-root model.
Removing a project from Projects or a session group opens a confirmation.
Type the exact project name and press Enter to unregister it; Escape cancels.
This removes the registration, not project files.
Mesh reuses bounded asynchronous `session --hosts` probes with cached
fallback; a last-seen remote session is not asserted to be currently live.
Session steering dispatches `send`; Mail dispatches signed correspondence.
Review's first queue remains the conductor-input/A2A approval queue, not mail
editing.

## Pairing, mesh trust and settings

Review lists three queues under one cursor: session send/A2A approvals from
`session pending list`, pending pairing requests from `pair --json`, and parked
secrets TOTP asks from `secrets pending --json`. Each section header carries its
own read's refusal, so a failed read never renders as an empty, all-clear
queue. The cursor follows a row's identity, so a resolve that shortens the list
moves the cursor with the rows that remain.

`a` acts on whatever the cursor names. Approving or resuming a pairing request,
like approving a TOTP ask, opens a masked prompt that names its exact target: a
pairing code is read off the other screen and typed once. An empty field and
Escape dispatch nothing, and submitting drops the prompt, so a rejected code or
TOTP is never replayed. An inbound approval is a local comparison; an outbound
resume polls the approver's door exactly once (`--wait 0`) and a new request
sweeps, dials and parks — both run on a worker thread so the interface keeps
responding, and a still-pending answer is the command's own wording, committing
nothing.

A pairing code is held in one place: the dedicated ceremony popup, opened by a
pair outcome and dismissed by Enter, Escape or `q`. It is taken only from the
outcome's own `sas`/`replySas` fields, never harvested out of text, and the
outcome stored for the status line has any code-shaped token replaced with a
pointer at that popup. No row, status line, log view or document string carries
a code; the ceremony popup exists because the ceremony itself requires a human
to read their own code to the other operator. Pairing never dispatches bare
`pair` (which would raise its interactive menu over this screen), never a
blocking `--wait`, and never a code on a new request.

Mesh adds the registry's own trust rows (`node status --json`) and every
declared mesh's divergence (`mesh --json`) beneath the roster probe: the node's
verified flag, its `allows` set, its cache state, and drift reported in the
compare's own words. Drift is reported, never repaired here — `e` or a
right-click offers the pairing ceremony for a divergent name, the three
capability toggles for a node with a record, and removal behind an exact-name
confirmation. Both trust reads are local, so opening Mesh dials nothing. While a
pairing leg is running this pane says so in its own status line, offers pairing
only, and refuses a node write it is asked for anyway: the leg's commit writes
the node registry from its own thread, and two read-modify-writes of that file
would lose one of them.

Status shows the config keys this instance may edit, walked from `config --json`
one level deep (`pairing.defaultGrant`, `upkeep.verifyCommand`), with the file's
own provenance — path, managed or unmanaged, present or absent — and the
backend's validation as the only gate for a value. Below them, `secrets
status --json` supplies the broker's own answer and one row per secret carrying
only the metadata fields that command names (backend, TOTP requirement,
consumers, automation, sharedWith, remote, allowRemoteOrigin). A broker that did
not answer — an absent socket, an unknown subcommand — renders that command's
error and no rows at all, never an empty inventory. Grant and revoke dispatch
the existing `secrets grant|revoke <name> <consumer>` and show the admin
command's own refusal verbatim; this pane escalates nothing and sets up no
custody.

Both secrets reads and every secrets mutation run on a worker thread: the broker
reads are unbounded, so a broker that accepts and never answers must leave the
interface painting. The last rows read stay on screen while a read is in flight,
with "reading…" beside them, and exactly one mutation runs at a time — a second
is refused with the reason on the status line rather than queued or raced, a
rejected TOTP is never replayed, and nothing is retried automatically.

## Correspondence and activity

Mail groups structured letters by their signed `threadId`, independent of
who joins later. Letters appear in local received-sequence order; the
participant roster includes observed senders, envelope targets, To and Cc.
Separate thread IDs stay separate even between the same people. Thread
labels use the earliest available nonempty subject.

The conversation list and the letter list each show a scrollbar when their
rows overflow the visible height, and none when everything already fits.

Letters without a thread ID appear as explicitly labeled legacy pair
correspondence. Replying to one uses that original message ID as the thread
ID and attaches only that original letter, not unrelated pair history.
Replies retain the thread ID and record the original message ID in
`replyTo`; a new letter or forward begins a new thread. These are local
archive views, not room membership or complete cross-host outgoing history.
Browsing performs no remote synchronization.

`n` creates a letter; `s`, `a` and `f` open Reply, Reply all and Forward.
The form keeps From, To, Cc, Subject and Message visible together with the
selected original letter. To and Cc accept comma-separated `node/mailbox`
addresses. The visible recipient tree lets you add individual agents while
composing: choose `[+ To]` or `[+ Cc]`, then a recipient. Project groups
organize those choices; selecting a project never broadcasts to all its agents.
Adding a recipient changes the draft only, without sending. The selected
To/Cc target remains active across recipient additions. Ctrl-P switches focus
between the recipient tree and the form. Escape from the tree returns to the
form before a further Escape can cancel the draft.

Click a field to edit it, or use Tab / Shift-Tab to move between fields.
The active field and text cursor are visible. Enter inserts a newline in
Message; elsewhere it advances to the next field. Arrow keys, Home/End,
Backspace and Delete edit text. Ctrl-S or the Send control dispatches the
letter; Escape in the form or Cancel discards the draft. Field labels stay
plain, with the cursor and shading indicating focus. Text input takes
priority over navigation and quit shortcuts, so typing `q` enters a letter.

Sending uses `mail send` with the explicit sender `conductor-human`.
Subject, To, Cc, thread ID and reply reference are structured content inside
the signed body; the backend files or queues an envelope for each distinct
recipient. A listed Cc address
alone is not proof of delivery. Local canonical addresses route through the
local delivery path. A partial failure keeps recipient results visible and
locks the submitted draft against resending successful recipients. An
entirely rejected submission remains editable. Browsing never advances an
agent's cursor, emits a receipt or rewrites a signed message.

Mail retains at most 200 letters from the last 2 MiB of the existing base.
Activity likewise reads at most 200 complete events from the last 2 MiB of
the audit JSONL. Selecting an event shows recorded time, door, class,
command, status, message and full raw JSON, including additional fields.
Missing facts are labeled rather than invented. Page Up/Down scrolls event
detail. Incomplete trailing writes wait for another refresh; errors remain
visible alongside the previous successful snapshot.

## Named seams

| Module | Owns |
|---|---|
| `app::App` | Loaded state, selections, folding, history, asynchronous roster refresh and dispatched actions |
| `app::PairCeremony` | The one place a pairing code is held and drawn; dropped on dismissal |
| `board` | Home/workspace composition, navigation, sidebar and shared drawing/hit-test geometry |
| `ui` | Pure panel/detail/overlay rendering |
| `scene` | Camera, view choice, retained world positions and the clipping painter |
| `graphview` | Graph model, card layout and wires over the retained scene |
| `mailview` | Non-consuming local letters, signed thread grouping and legacy pair correspondence |
| `eventview` | Bounded audit reader and full-record event rendering |
| `logtail` | Read-only headless-session log overlay |
| `theme` | Styles, glyphs and shared formatters |
| `commands` | Registration of `conductor` |

The CLI supplies an injected `DispatchFn`; this crate does not assemble a
second command registry or depend on the CLI/daemon implementation. Mutations
use that dispatch seam. Pure views read state without performing actions.

Projects, sessions and hooks come from the conducting stage (`state/stage`).
Past sessions come from `state/session-ledger.jsonl`. Mail uses the existing
mail base; Activity uses the existing audit path. The rice stage
(`song/stage`) supplies palette notes only and is not the conducting store.

`assets/logo.txt` bundles the plain text art from
`modules/dendrites/fastfetch/ascii-fetch`. Its inclusion adds no runtime Nix,
Fastfetch or Qt dependency. Conductor belongs to the core binary, not Lyra.

## Mail review boundary

Editing/intercepting agent mail before dispatch is not implemented. It needs
a daemon-enforced proposal gate with stable IDs, exact-revision approval,
preserved original text and operator attribution. Transport outbox hold and
the existing Pending panel do not provide that guarantee. Delivered signed
letters remain immutable; corrections are new messages.

See [DESIGN.md](DESIGN.md) for state flow and the remaining review seam.
Run `cargo test -p aoide-conductor` from the package workspace for this
crate's model, input and rendering checks.
