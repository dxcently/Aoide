# AGENTS.md — aoide-conductor

## Invariants

- Frontend only: every mutation uses the injected `DispatchFn` with a normal
  invocation. Reuse existing command outcomes and storage types; do not
  create another project registry, mail store, policy engine or dispatcher.
- Rendering stays pure. `ui`, `board` and graph views draw from state;
  input handling owns state changes and dispatch. Mouse hit testing and
  drawing share geometry and row ordering, including scrolling and narrow
  layouts. A popup is sized from its own measured content, and the draw
  path and the hit test call the same sizing function so the click target
  is always the drawn rectangle. An action must mean the same thing by
  mouse and keyboard.
- The graph is a retained scene. World positions, camera, view and the selected
  node's identity live in `App::graph`; a frame reads them and never rebuilds
  the world. Placement keeps a surviving node's rectangle, re-places a node
  whose depth changed, and drops a departed one. Selection is a node identity,
  never a row index, so a refresh cannot move it.
- Graph zoom is a camera transform of those fixed world rectangles at
  50/75/100/125/150%. Pointer-anchored zoom, pan limits, render and hit tests
  share one transform; never let render record rectangles for a later hit test.
  Terminal glyphs stay fixed-size and clip inside cards; never relayout
  entities into alternative card presets when zoom changes.
- The painter clips; the world outside the camera is never drawn. Edges paint
  before cards. Focus draws the selected node's connected component and All
  draws every node, and the synthetic root gathering unattached sessions is not
  an edge Focus may traverse.
- One identity mark per kind, from `theme::mark`; never a second glyph for
  the same kind, never a mark wider than one cell, never `@` (addresses).
  Colour comes from a `Role`, not from the glyph. Measure labels with
  `board::cells`, never `len()`.
- Only the keyboard-focused pane gets a bright selection; other selected
  regions use subdued livery shading. Controls use ASCII markers and plain
  field labels, without editing badges or double borders.
- Project removal snapshots its target and requires an exact-name confirmation;
  initial keys, empty input and Escape never unregister a project.
- Context menus snapshot their exact target. Historical actions may resurrect
  only through the existing registered-project/native-ID or restore-snapshot
  path; never route them to a stale live focus or project mutation. Project
  resurrection retains the backend's undying semantics. Do not advertise
  unsupported Rename or Kill actions.
- Semantic colors read exact RGB Base16 tokens from rice-stage livery;
  ANSI colors are fallbacks for absent tokens, not a competing theme.
- History is separate from live state. Ledger entries never enter
  `App::merged()` or the live graph. Opening one only selects historical
  detail; stale live selection must not receive Enter, kill or focus while
  history is displayed. Preserve explicit recorded project attribution;
  use the existing root matcher only when attribution is absent.
- Human mail browsing is non-consuming. Read the existing immutable base
  through storage's `Entry` shape without recipient read/mark, receipts,
  repair or migration writes. Keep original message IDs, canonical
  addresses and text. Structured conversation identity is the signed
  `threadId`, never the current participant set or subject. Aggregate
  canonical sender, envelope target, To and Cc endpoints without changing
  routing identity. Legacy pair correspondence stays labeled; a legacy
  reply thread includes only the exact original whose msgid anchors it.
  Preserve `replyTo`; new letters and forwards receive new thread IDs.
- The mail form owns input before global navigation. Its selected field and
  UTF-8 cursor stay consistent with mouse hit testing. Opening a tree
  recipient action only edits a draft; it never dispatches implicitly.
  The visible recipient tree obeys the selected To/Cc target and never
  treats a project group as an implicit broadcast destination. To/Cc targeting
  persists across additions. Ctrl-P changes tree/form focus; Escape leaves
  the tree before cancelling the form. Text entry owns printable keys.
  Reply and Forward retain the immutable original as context. Partial
  accepted submissions cannot repeat the whole fanout; surface recipient
  results and keep the draft locked against duplicate sends.
- A human-composed letter is attributed to `conductor-human` and sent only
  by explicit dispatch. Enter in the body inserts a newline. Do not
  silently rewrite or re-sign agent-authored/delivered mail as that agent.
- Mail interception needs a real daemon proposal gate. Neither transport
  hold nor Pending input approvals is an editable-mail policy. Preserve
  originals, actor attribution and exact revision decisions if that seam
  is implemented. Do not claim outgoing archive completeness or remote sync.
- Bounded mail/audit reads ignore incomplete final records without
  modifying files. Failed refreshes expose the error and retain prior
  data. Raw audit fields remain inspectable; absent facts stay unknown.
- Conducting stage and rice stage are distinct. Project/session/hook reads
  use `App::stage`; palette notes use `App::rice_stage`. Historical ledger,
  mail and audit paths use their existing owning APIs.
- Roster probes remain bounded and asynchronous; refresh throttling must
  not freeze input. Offline cached sessions retain their last-seen status.
- Pending IDs are array positions. Relist after every approve/deny before
  using another selection; never recycle those indices as durable mail
  proposal identifiers.
- A pairing code lives only in the dedicated ceremony popup
  (`app::PairCeremony`, no `Debug`). Take it from a pair outcome's own
  `sas`/`replySas` fields, never by scanning text; sanitise the outcome before
  it reaches `last_outcome`, and never write a code into a row, a status or
  detail line, a log view or a test fixture (fixtures may carry a synthetic
  code; only a REAL one is forbidden anywhere). A code the operator types is
  masked as it is drawn, is never pre-filled, and is dropped on submit: an empty
  field and Escape dispatch nothing, and a rejected code or TOTP is never
  replayed.
- Never dispatch bare `pair` (the CLI raises its interactive menu on this
  process's own tty), never a `pair` wait other than `--wait 0`, and never a
  code on a new request. A pairing leg that dials or sweeps runs on the
  background thread and is drained without blocking; a new request follows an
  explicit UI activation.
- Anything that can cross the broker socket runs on a worker: `secrets pending`
  and `secrets status` reads, and every secrets mutation
  (`approve`/`dismiss`/`grant`/`revoke`), whose client reads are unbounded.
  Exactly ONE mutation is in flight at a time — a second is refused with a
  visible reason, never queued or raced — and nothing retries automatically.
  While a read is in flight the last rows stay on screen and the pane says it
  is reading.
- While a pairing leg is in flight, no node write is advertised or dispatched:
  the leg's commit writes `state/nodes.json` from its own thread, so a
  `node allow`/`node remove` from the UI thread at the same time is a lost
  update. The refusal names the reason, and the pane that started the leg
  renders its own "working…" line. The INBOUND approval is the same kind of
  write even though its leg is local, so it is refused while another leg is on
  the wire too, and the refused prompt is dropped with the code. `pair reject`
  needs no such guard: it writes only the flock-guarded pairing store, never the
  node registry, so it cannot lose a commit — its outcome is the command's own
  (an entry the leg has already taken reads as unknown).
- Every popup is modal for the pointer as well as the keyboard: no click
  switches a panel or opens a menu under an open popup, and the established
  dismissal closes it.
- A failed read renders that read's own refusal. A `secrets status` answer that
  never arrived — absent socket, unknown subcommand — is an error line and no
  rows, never an empty inventory. Render only the metadata fields the command
  names.
- Review and Status selections are row identities, clamped by identity after a
  re-list; a shrink moves the cursor with its own row and never onto a section
  header. Draw and hit test split those panes through the same function.
- Grants and configuration go through their existing commands (`node allow`,
  `config set`, `secrets grant|revoke`, `pair`), whose validation and refusals
  are the gate. This frontend adds no custody, no privilege and no second
  schema.
- All exit paths preserve `TermGuard` restoration and panic-hook cleanup.
  Keep terminal/mouse state correct after normal exit, errors and panics.
- No Wayland, image, Qt, Nix or song runtime dependency enters this core
  frontend. The bundled logo is plain text branding, sourced from the
  Fastfetch art, not a runtime call to the desktop module.

## Extension points

A panel adds an App selection/state seam and pure renderer, then joins the
shared navigation, keyboard and mouse geometry. Number keys 1–9 then 0 and Tab cycling follow the visible tab order. Panel-specific keys
are scoped to their handlers; text input and overlays take priority.

Project rows represent projects, not individual roots. Use `Project::roots`
and existing attribution/grouping helpers. Tree row models are the common
source for rendering, focus, collapse and activation.

A scrollable list reserves its rightmost column for a scrollbar only when its
rows overflow the viewport, through `board::scrollbar_split` and
`draw_scrollbar`; draw and hit test both call `scrollbar_split` so a reserved
column, when there is one, is exactly what was painted. A new scrollable list
reuses this pair rather than hand-rolling another offset/thumb calculation.

Tests use isolated paths or injected fixtures. Never read or mutate the
operator's ambient mail cursors, ledger, registry or terminal during unit
checks. Add meaningful state-transition and narrow-layout tests for changed
input/view seams; run this crate's tests, not an unrelated workspace sweep.

## Documentation updates

Update README.md for user-visible behavior and named seams, and DESIGN.md
for state flow or integrity boundaries in the same commit as the change.
Keep documents integral and current; implementation history belongs in the
commit/log. Cross-crate invariants belong in the parent AGENTS.md.
