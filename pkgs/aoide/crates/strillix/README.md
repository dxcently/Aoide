# strillix (bin `strillix`)

`strx` is the shorthand for `strillix`: the same binary under its short name, one surface, one output.

The paint app crate — the second composition root over the same
domain-crate handler code `aoide-cli` assembles (P-A4). In Cordis terms
(CONTRACTS.md §0): a BUNDLE, same as `cli` — its own ordered
`commands::all()` profile over an independent `Registry`. Owns the
self-ricing loop, `screen`, `notifications`, `shellbridge`, and `quickshell`;
deliberately never conducting, the graph, A2A, nodes, or the daemon (those
are core `aoide` identity, root `AGENTS.md`).

## Named seams (what it exposes)

- `bin/strillix` — the binary entry point.
- `dispatch`/`registry` — strillix's own argv parsing, dispatch, and golden
  command-path snapshot, independent of core's.
- `commands::LAYOUT` — how `strillix` lists itself (Start here · Rices &
  themes · Widgets & icons · Desktop apps · Clipboard · Screen control (screen, lock) ·
  Dialogs & notices · Agent interfaces · System), adopted by `all()` through
  `Registry::arrange`; bare `strillix`, `--help`, group pages and `guide` read it.
- `guide` — `strx guide`.
- `bridge::VERBS` — the nine desktop verbs `strillix shellbridge --run` serves
  through conduct's socket loop; see *The desktop bridge* below.
- `commands` — strillix's `commands::all()`, pulling in `rice`, `screen`,
  `shellbridge` (its own command, `src/commands/shellbridge.rs`), and
  `conduct`'s `notifications` registration line (the file stays in `conduct`;
  only the registry line is strillix's), plus strillix's own root-
  coupled `onboard` (below).
- `commands::onboard` — `strx onboard` (P-I3, docs/architecture/ONBOARD.md):
  the nix half of the onboarding flow, reached only via `aoide onboard`'s
  delegate spawn once `rice_bin()` resolves. Reads the SHIPPED
  `share/strillix/aoide-options.json` (`fs::strillix_share_dir()`, which
  `pkgs/strillix-rices` builds from `lib/options.nix` — every `aoide.*` option
  declared across `modules/{nucleus,dendrites}`, derived, not hand-listed) and
  renders `aoide.nix`: a nix module the user imports, every option commented
  out at its default. No `nix` and no checkout are needed: the file is the
  eval, so it works from any cwd. Reruns over a
  previously-generated file warn, back up to `<out>.bak`, and regenerate; a
  hand-written file at the target path is refused, never overwritten. Never
  touches the user's flake.
- `run_strillix` — drives `aoide_protocol::door::run` with strillix's own registry/
  dispatcher and its own smaller `special` hook (`mcp serve --stdio`,
  `guide`/`schema`/`theme`/`secrets ask`/`pair ask`/`pair show` raw
  output). Deliberately absent: `a2a serve`, `conductor`.
- `commands::dialog_qml` (P-PV3, task #132; the confirm surface repurposed
  into the show surface at R2) — the shared quickshell code-entry/show
  SURFACES `secrets ask`/`pair ask` (entry) and `pair show` (show) render:
  the entry variant's six boxes, the dash, the underlying `TextInput`; the
  show variant's plain code display, a Copy control (a hidden read-only
  `TextInput` backing `selectAll()`/`copy()`), and a Done control — no
  reject control, since this dialog fires only after the ceremony's own
  commit already succeeded; both share the spawn/wait-for-marker/cleanup
  orchestration, `qml_escape`, and the `EXIT_INFRA_FAILURE` reservation —
  a caller supplies only a window title, a styled header block
  (`HeaderLine::bold`/`italic`/`muted`), and its own `RESULT_MARKER`
  prefix (the entry variant also takes its own dismiss-control label; the
  show variant also takes the code to display, but has no dismiss label
  of its own to take). A header line WRAPS at the window's own width
  (`win.width - 40`) and the window's height grows with the wrapped
  content: the window is a fixed-size hint (that hint is what makes
  Hyprland float it), so a line longer than 400px used to render at its
  natural width and get cut at both edges — live-proven on the pairing
  ask, which lost its node name and its request id that way. Extracted from `commands::secrets` (P3's original
  module) the moment a SECOND caller needed the identical entry
  component; the show variant followed when `commands::pair` needed a
  SECOND surface for its own reply-code display (originally a confirm
  surface for the ceremony's outbound direction, repurposed at R2 once the
  mutual-code redesign moved that direction onto the entry surface
  instead) — never a copy of either template. Writes the generated QML to
  a scratch temp path and spawns
  `quickshell -p <path>` as a genuinely standalone process
  (`rice::commands::reload`'s own IPC-reload beat only ever sends IPC into
  an ALREADY-running instance, by contrast); `PR_SET_PDEATHSIG`
  (`libc::prctl`) keeps a killed dialog from ever orphaning its own window
  (`AGENTS.md`'s own invariant has the full ownership-chain reasoning);
  `qml_escape` covers backslash/quote, `\n`/`\r`/`\t`, U+2028/U+2029 (JS
  line terminators even inside a string literal), and the remaining C0
  range, since every header line — and the show variant's own code —
  is treated as untrusted display text on principle. The generated QML
  lands under `$XDG_RUNTIME_DIR` when set (else `temp_dir()`), written
  `0600` from creation — matching `aoide-secrets`' own `store::
  secure_file` discipline even though these files only ever hold display
  data.
- `commands::secrets` — `strx secrets ask` (P3): the rice-shaped code-entry
  dialog `aoide secrets watch --popup` spawns in place of `zenity --entry`
  once it resolves (`aoide_secrets::watch::resolve_strillix_bin`). Owns only its
  own flags (`--secret`/`--consumer`/`--seconds`/`--reason`/`--from`) and
  header wording ("release `X` -> Y") on top of `commands::dialog_qml`'s
  shared entry surface; speaks zenity's own output contract byte for byte
  (code on stdout + exit 0; `Dismiss ask` on stdout + exit 1; a bare
  cancel, exit 1 with nothing on stdout) so `aoide-secrets`' own
  dialog-result parsing never needs to know which binary answered — that
  crate's `watch.rs` module doc has the wire-side half. `EXIT_INFRA_FAILURE`
  (exit `3`, reserved for a spawn failure or a marker-less quickshell exit,
  never folded into a bare cancel) is a re-export of `dialog_qml`'s own
  constant, kept at this path since `lib.rs`'s `special` hook already
  reads it here.
- `commands::pair` — the pairing ceremony's own TWO dialog commands,
  spawned by `aoide pair watch --popup` in place of the matching zenity
  invocation once `aoide_client::pair_watch::resolve_strillix_bin` finds this
  binary: **`strx pair ask`** is the six-box entry surface, on EITHER
  pairing direction now (the mutual-code redesign, R1) — `--id`/`--name`/
  `--context` only, no `--code` flag at all, because the whole gate is
  typing a value that arrives from elsewhere (the requester's own screen
  on the approver's inbound leg, the approver's reply code on the
  requester's outbound leg). **`strx pair show`** (R2) is the reply-code
  DISPLAY surface — `--id`/`--name`/`--context`/`--code` (the last
  REQUIRED: a display dialog with nothing to show would be a blank
  window), rendering this instance's own locally-derived reply SAS large
  and plain with a Copy control and a Done control, no reject control at
  all: it fires AFTER an approver's own commit already succeeded, so
  there is nothing left to approve or reject. Both share
  `commands::dialog_qml`'s underlying surfaces and a
  `AOIDE_PAIR_ASK_RESULT:` marker; `pair ask` alone carries the
  `"Reject request"` dismiss label, since `pair show` has no dismiss path
  to label. `--context` is pre-formatted ONCE by the caller (untrusted,
  node-supplied display text: a requesting host + short id, or a reply
  code's own recipient name) so zenity and the matching strx dialog
  render byte-identical wording, the same "one place this wording lives"
  discipline `aoide_secrets::watch::format_origin_line` holds for its own
  `--from`. **`pair show` is `pair confirm` (P-PV3, task #132),
  repurposed and renamed**: that command was originally the OUTBOUND
  leg's own Approve/Reject shape — a confirm surface reviewed and kept
  after an earlier pass on the same phase had tried reusing `pair ask`'s
  own entry surface for that direction too (with the code pre-shown as
  context, retyped into the boxes) and a review correctly called that
  copy-the-pixels theater, since the code was already on screen in the
  same window the boxes sat in. The mutual-code redesign (R1) later gave
  the outbound leg a genuinely SECOND code (the approver's own reply
  code, arriving from a different screen) to gate on, which is exactly
  the case that same review's own argument named as real typed entry —
  so the outbound leg moved onto `pair ask` above, and the confirm
  surface's only remaining job became showing the approver's reply code
  back, which is what `pair show` now is.
- `commands::stubs` — `rice declare`'s handler (and the `rice transpose`
  stub): `rice declare <name>` copies the composed rice into the checkout,
  once `strillix_rice::checkout_gate` finds git clean under that rice folder and
  the staged `rice.nix` is a v1 body (a refusal names the folder, or the file
  with `reason: old-rice-shape`); `rice declare <name> --host` (CLI door only)
  hands the name to `strillix_rice::host_declare`, which rewrites the host
  config's declared-rice line for the next rebuild and warns, never refuses,
  when the folder is dirty.
- `commands::preview` (P1, then a same-lane follow-up) — **`strx preview
  [<widget>]`** builds an ISOLATED preview root (default
  `$XDG_RUNTIME_DIR/aoide-preview`, chosen deliberately as a SIBLING of the
  live daemon's own `$XDG_RUNTIME_DIR/aoide/` socket dir, never a
  descendant of it — the live reaper and socket globs look there) and
  spawns a standalone `quickshell -p <root>/run/qml/WidgetPreview.qml`
  (P2's own file, not yet landed) against it: its own
  `/stage/theme.json`, its own
  `state/stage/{sessions,projects,hooks,notifications}.json` seeded from a named
  fixture set or a directory (falling back to empty-but-valid defaults
  while no fixture set has landed yet), and its own `run/qml/` — COPIES,
  never symlinks into the checkout: one copy per shell `*.qml`, the shell's
  resolved `icons/` tree (the canvas toolbar's Iconoir glyphs, `strx icon
  resolve` output already in the checkout), one copied
  tree per non-`_`-prefixed rice's `widgets/` dir (`stage_qml_copies`/
  `stage_rice_copies`, a stale symlink from an older root replaced), and
  `manifest.json`/`registry.json` copied from the live deployed tree when
  present else `{}`. `preview.json`'s `stage` map (`compute_stage_map`)
  pairs each `watch` entry with its copy so the canvas can refresh the
  copies before a reload; `resolve_widget_abs` maps a `widget` field to the
  CHECKOUT file it names (`rices/<s>/<rest>` → `rices/<s>/widgets/
  <rest>`), never to the copy. `resolve_root` refuses a `..` component and
  any `--root` at, under, or symlink-resolving into the live daemon's dir
  (`live_daemon_dir`: `$XDG_RUNTIME_DIR/aoide`, or storage's own
  `daemon_socket_path()` fallback when that's unset -- read directly rather
  than through `daemon_socket_path()` itself, since that function also
  honors `$AOIDE_DAEMON_SOCKET`, which the canvas's own child env repoints
  at `<root>/no-daemon.sock` and must never make a preview root look like
  the live dir from inside its own canvas) as a usage error. A running
  canvas (`preview.pid` live) blocks only a second LAUNCH on its root;
  `--no-launch` re-stages the copies and merges into the control file
  while it stays up. Every OTHER lane-read stage file
  the isolated root needs merely to exist gets written too:
  `/stage/mode.json` (`ThemeState.qml`'s own convention — `mode`,
  `rice`, `stagingRice`, `since`) is REWRITTEN on every build and on
  `preview set --rice`, always naming the current rice; `/stage/
  wallpaper.json`, `/stage/grimoire.json` (`GrimoireLedger`'s
  `launches: {}`, an object keyed by app id, never an array), and
  `state/usage.json` are seeded once, only if absent, and never clobbered
  by a later rebuild. `--theme <spec>` accepts FOUR
  forms, all resolved through the same
  `strillix_rice::theme::resolve::resolve`/`to_json_string` engine
  `strx theme resolve` itself uses: `live` (the resolved live stage twin,
  read-only), a bare rice name (that rice's own rices theme.json), a
  path to a theme file, or a path to a base16 scheme (JSON or the flat
  `key: value` line shape — no yaml crate in this workspace) synthesized
  into a palette per CONTRACTS.md §1's base16 column reversed; default is
  the preview's own `--rice`. `preview.json` carries `themeSource` (the
  spec as given, the one name for it) and `themes` (every rices rice
  with a theme.json of its own, sorted) — never a bare `theme` key: the
  P2 canvas treats `theme` as a legacy alias of `themeSource` and
  deletes it on every save of its own, so both the builder and `preview
  set` drop it on every write (migrating away any legacy key an older doc
  still carries) rather than ping-ponging against the canvas. `preview.json`
  also carries `strillix` (the BUILDING binary's own `std::env::current_exe()`,
  written once at root build and never touched by `preview set`) — the P2
  canvas spawns every rail action against that path instead of a bare
  `strillix` on PATH, since a deployed `strillix` there may predate `preview set`
  entirely and would fail every rail action silently; a control doc from
  before this field existed leaves the canvas falling back to PATH. It also
  carries `watch` (every `*.qml` under the current rice's
  own `widgets/` plus the previewed widget's own file when it lives
  elsewhere — the set the P2 canvas hot-reloads against). The child's env
  is rewritten (`AOIDE_ROOT`/`STRILLIX_ROOT`/`AOIDE_STATE_DIR` repointed at the
  isolated root, `AOIDE_STAGE_DIR` removed, `AOIDE_DAEMON_SOCKET` at a path
  that never exists, so no lane QML can reach the real `aoided` even bypassing a stub
  bridge, and every widget-slot env override — `QS_STAGE`,
  `CONDUCTOR_WIDGET`, etc. — stripped) and `PR_SET_PDEATHSIG`-armed
  exactly like `commands::dialog_qml::spawn_quickshell`'s own discipline,
  rewritten locally rather than imported (that fn is private to its own
  module). A `preview.pid` file (the INVOKING `strillix` process's own pid,
  not quickshell's) refuses a second concurrent `strx preview` against the
  same root. **`strx preview set`** edits that root's `preview.json`
  control document (widget size/aspect, viewport size/preset/aspect,
  anchor, margin, zoom, background, fixture, theme) without any canvas
  needing to be running — every given flag is validated (a bad value is a
  usage error naming the allowed set) and merged: existing keys survive,
  only the given ones are overwritten, so a relaunch (or a live edit while
  the P2 canvas hot-reloads off the same file via `FileView`) preserves
  whatever the human last set; `--widget`/`--rice` also recompute `watch`.
  Never touches the live desktop on its own — launching the canvas is the
  one side effect, and `--no-launch` skips even that, leaving a plain
  filesystem build a test or a CI check can run with no quickshell
  installed at all. **`strx preview declare [--slot <name>]`** is the
  canvas's own counterpart of `rice declare` (`commands/stubs.rs`,
  `Self-Ricing.md`'s stage-vs-commit split): it copies the previewed
  widget body (refusing a `widget` outside `run/qml/rices/` — never a
  lane symlink) and, unless `themeSource` is exactly the preview's own
  rice (its palette is already checkout truth), the resolved palette
  tiers (`palette`/`base16`/`bar`/`notif`/`window` only — every other
  authored key in the checkout's `theme.json` survives untouched, and
  `rice` is never written there) into `rices/<rice>/`. Byte-diffed
  so a repeat with nothing new is a true no-op; it never runs `git`,
  touches the live stage, `~/.aoide`, or `run/qml` — the commit and
  rebuild stay the user's own gate (house rule 2), same as `rice declare`.
- `commands::preview_tools` (P6) — shell-first agent tools over that same
  isolated canvas: the QML side only PAINTS what these three commands
  already expose (root `AGENTS.md` house rule 7's "delete every `.qml`"
  test). All three take `--root`/`--json` (same resolution as `preview`)
  and read `ROOT/preview.json`; widget/element operations shell the canvas
  through `qs -p ROOT/run/qml/WidgetPreview.qml ipc call preview <function>
  <args...>` (`qs`'s own error text can land on stdout, not stderr — every
  ipc call reads and joins both so a failure is never silently empty).
  **`strx preview shot [--what screen|canvas|widget|element] [--element
  <path>] [--annotated] [--out <png>]`** (default `--what widget`; `--out`
  defaults to `ROOT/shots/<what>-<utc-compact>-<pid>-<seq>.png`, a pid+seq
  tail so two shots in the same UTC second never collide; `--element`
  without `--what element` is a usage error): `screen`/`canvas` call
  `aoide_screen::capture::shot` directly (canvas by `--window <addr>`:
  `find_client_address(canvas_pid(root))` — class + title + the pid `qs
  list --all` reports for this root's `WidgetPreview.qml`, so two open
  roots never capture each other; "canvas not running" when absent) —
  its own sidecar (`<out>` with the extension swapped, `aoide-screen`'s
  own convention) is read, folded into ours under a `"capture"` key, and
  deleted, so no orphan `<stem>.json` is left beside `<out>.png.json`.
  `widget`/`element` ALWAYS call the canvas's `shot("widget", ...)` — the
  canvas's own element-kind ipc call is permanently broken (always a
  transparent/failed capture) and is never invoked — then poll for the
  canvas's own success/failure file pair: success writes `<out>` (the
  full widget image) AND `<out>.meta.json`; failure writes `<out>.error`
  (a plain-text message) and no image at all, surfaced verbatim as the
  command's own error. For `--what element`, strillix itself then resolves
  `--element`'s live rect through the same tree join used by `preview
  tree`, opens `<out>` with the `image` crate, crops it to that rect
  (scaled and clamped to the image bounds — an off-canvas or zero-area
  rect is an error, and the file is left as the uncropped widget capture)
  and overwrites `<out>` with the crop; `elementRect` in the sidecar is
  this crop's own pixel rect, never anything from the canvas.
  `<out>.json`/`<out>.meta.json`/`<out>.error` are all `--out`'s FULL value
  with the suffix LITERALLY APPENDED, never an extension swap — `--out
  widget.png` sidecars at `widget.png.json`, never `widget.json`. Every
  kind writes its own `<out>.json` sidecar —
  `{what, out, root, widget, rice, widgetWidth, widgetHeight, scale?,
  element?, elementRect?, source?, capture?, notes}` (`capture` is the
  screen/canvas kinds' folded-in `aoide-screen` sidecar, above); for
  widget/element the canvas's own `<out>.meta.json` is `{widgetRect,
  scale}` ONLY — `scale` is merged in, `widgetRect` has no home in this
  sidecar — and the meta file is deleted. `Outcome` data is the sidecar
  object. **`strx preview tree [--at x,y]`** (`--at` is validated as
  `x,y` before touching the canvas) calls the canvas's
  `tree(out)`, parses the live item tree (`{type, objectName, path,
  rect:{x,y,w,h}, visible, text?, children}`, widget-local coordinates —
  `path` is the canvas's OWN element-path string for that exact node,
  carried verbatim into every command that needs one, never re-derived),
  and joins it against a STATIC parse of the previewed widget's own QML
  source (`resolve_widget_abs`): a line matching `^\s*([A-Z][A-Za-z0-9_.]
  *)\s*\{` opens a node of that type (a dotted spelling like
  `MoodFaces.Face` strips to its last segment), OR a line matching
  `^\s*([a-zA-Z_]\w*)\s*:\s*([A-Z][A-Za-z0-9_.]*)\s*\{` opens one the same
  way while also recording the binding name (`delegate`, `contentItem`,
  `background`, `sourceComponent`, …) — `conductor.qml`'s own `Repeater`s
  all write their delegate as `delegate: Column { id: … }`, never a bare
  `Column {` on its own line, so this second form is not an edge case here,
  it's the ordinary shape a `Repeater`'s delegate takes; skipping it left
  the delegate's real children silently reattached one level too shallow,
  onto the `Repeater` itself (a live run: the `Repeater`'s own `id` field
  reading its delegate's `id: movement`). A binding-form opener's `Type`
  still comes from the SAME dotted-spelling rule; `property var x: { ... }`
  (a JS expression used as a property's value) is excluded with no special
  case — nothing but a plain `{` sits between its `:` and end of line, so
  there is no `Type` for either opener form to accept. `id:`/`objectName:`
  lines attach to whichever node is currently open, braces are counted per
  line with string literals and `//` comments blanked out first — no
  multi-line `/* */` support, and an opener (either form) must be the
  first thing on its own line; good enough for the style this workspace's
  widgets are written in, not a real QML parser (there is no such
  dependency in this workspace).
  Before any of that, at every node, a runtime node's own `type` is
  checked against a `ComponentMap` built from every `*.qml` file named in
  `preview.json`'s `watch` list (the rice's `widgets/` directory plus the
  previewed widget's own file, each parsed once): each file registers
  itself keyed by basename, AND every inline `component Name: Type {
  ... }` declaration found while parsing any of those files registers
  itself keyed by `Name` in the SAME map, GLOBALLY — not scoped to its
  declaring file, since QML component names are already expected to be
  unique project-wide; a name collision across files resolves by
  last-file-parsed-wins. A hit either way (`"file"`) switches the static
  context to that entry's own root node (the parsed file's top level, or
  the inline component's own base-type frame and its already-nested
  children) and the runtime children are matched against it for the rest
  of the recursion — this crosses `Loader`/component boundaries
  (`conductor.qml` -> `SessionMenu` -> `SessionCard` -> ...) AND inline
  component boundaries (`conductor.qml`'s own `component SessionCard:
  FocusScope { ... }` — the runtime type is the component's name,
  `SessionCard`, not its base type) that a single-file static parse can't
  see into on its own. Only below that, the join matches a runtime child
  to a static sibling by exact `objectName` first, else by position among
  remaining same-type static siblings under an already-matched parent
  (`"positional"`). Failing both, a runtime sibling of a static `Repeater`
  whose type equals that SAME `Repeater`'s own `delegate:`-bound child's
  type also resolves `"positional"`, against that nested delegate child
  directly — a `Repeater`'s stamped-out instances land in the runtime tree
  as its SIBLINGS, never its children, so ordinary same-level matching
  alone can't reach a delegate one static level down; every instance
  (there may be many) resolves to that ONE static child, sharing its line,
  never consumed the way a normal positional match consumes its static
  sibling. Failing that too, if the FULL static sibling list at that
  level (used or not) has no entry of that runtime node's exact type, the
  node is a framework-synthesized transparent wrapper (`"wrapper"`) —
  `Flickable`/`ListView`'s implicit `contentItem`, a `Loader`'s loaded
  item, and the like — with no static counterpart of its own: its
  `source` is inherited from the nearest ancestor that DID resolve, and
  its OWN children are joined against the SAME static sibling list it
  just failed to match against, rather than against nothing, so a real
  static child one runtime level down (e.g. a `Column` inside a
  `Flickable`'s synthetic `contentItem`) still resolves. A static sibling
  of that exact type existing at that level — even one already claimed by
  an earlier same-type runtime sibling — takes wrapper treatment off the
  table; that case is a genuine mismatch and stays `"none"`. The file
  check runs unconditionally ahead of all of this, so a `"wrapper"` or
  `"none"` node does NOT close off the static side for its descendants —
  only a subtree with no basename/component-typed node anywhere below it
  stays unresolved; this is ORTHOGONAL to `path`, which identifies the
  node, not its static counterpart. Human output is an indented `Type#objectName
  x,y w×h  -> file:line (match)` tree; `--at x,y` returns the DEEPEST
  visible node whose rect contains the point regardless of match kind
  (descending through every containing child, not just matched ones), as
  JSON `chain` (its ancestor path — a different concept from each node's
  own `path` key, named accordingly). **Element path grammar** (shared by `shot
  --element`, `notes --element`, and this join): `Type[i]` per level
  joined by `/`, `#objectName` suffix informational only — `i` counts
  among same-TYPE children at that level, from the previewed widget's own
  root down, in declaration/creation order; CANVAS-AUTHORITATIVE (a
  `Repeater`'s generated items land as SIBLINGS of the `Repeater` in
  `children`, so only the canvas's own numbering is guaranteed to get it
  right) — `resolve_element_path` matches a node's `path` verbatim rather
  than re-deriving it. **`strx preview notes`** (`[--json]` to read, `--add --text <t>
  [--rect x,y,w,h | --element <path>] [--shape rect|ellipse|arrow|line]`,
  `--done <n>`, or `--clear`) keeps `ROOT/notes.json` —
  `{schemaVersion: 0, notes: [{n, kind, shape?, rect?, element?, text,
  createdAt, done}]}`, atomic writes, `n` = max existing `n` + 1, never
  renumbered (`--clear` empties the list but keeps the file, so the next
  add after a clear restarts at 1 with no surviving max to add onto),
  `kind` = `highlight` for
  `--element`, `shape` for `--shape`, else `note`. On every read, a note's
  `element` is resolved to `source` (`file:line`) through the same tree
  join when the canvas is up; when it isn't, notes print with no `source`
  and the message says so. Human output is one `#n [kind] Type#obj
  (file:line) — text` line per note, done ones prefixed `✓` and dimmed
  (ANSI SGR faint).
- `commands::icon` (I1, `design/p-icon-brief.md`) — `icon collections`/
  `icon list`/`icon resolve`: pack-independent widget icons, Iconify
  `<collection>:<name>` identifiers resolved against a pinned, hashed local
  copy of each collection's IconifyJSON (`pkgs/iconify-data`, `--data <dir>`/
  `$AOIDE_ICON_DATA`), never a network fetch at resolve time. `resolve` is
  the only writer — `<out>/<collection>/<name>.svg` (or `<out>/custom/
  <slug>.<ext>` for a `file:<path>`/bare-path asset) plus one `<out>/
  catalog.json` — byte-diffed (`preview declare`'s no-op discipline) and
  PRUNING any asset the current selection no longer names. The SVG assembly
  is a direct port of upstream Iconify's own `@iconify/utils` alias-chain
  fold and `iconToSVG`/`iconToHTML` string math (module doc cites the
  fetched source lines). Registered after `preview_tools` in
  `commands::all()`.
- `xdg` — the workspace's one freedesktop key-file reader: `keyfile` (the
  grammar that `.desktop` files, icon-theme `index.theme` files and
  `gtk-3.0/settings.ini` share), `entry` (desktop entries by id, the first
  data dir wins, and the listing filter) and `icon_theme` (the theme chain and
  icon lookup at one nominal size). Everything takes an `Env`, never the
  process environment, so a test builds one over temp dirs. `commands::apps` (and the
  planned `strx launch`) read through it; neither carries a parser of its own.
- `commands::apps` — `apps list`/`apps show`/`apps publish [--run]`: the
  installed desktop apps with their icons resolved to files, and
  `/stage/apps.json` (CONTRACTS.md §4), which `apps publish --run` keeps
  current (the `aoide-apps` user unit that runs it is planned). `list` and `show` build fresh and
  never read the file; `publish` replaces it atomically, and only when the
  document minus `at` changed. docs/Aoide-Wiki/concepts/cli/Apps-Commands.md
  has the commands.
- `commands::clipboard` — `clipboard delete <id>`/`clipboard clear`: one
  cliphist history entry or all of them, plus the launcher's cached
  thumbnails under `$XDG_CACHE_HOME/aoide-clipboard/previews`, over the
  `cliphist` on `PATH`. The bridge `AoideClipboard.qml`'s `deleteById`/`clear`
  run. docs/Aoide-Wiki/concepts/cli/Clipboard-Commands.md has the commands
  and their `data.reason` codes.
- `commands::component` — `component <slot> <verb> [arg]`, the one door for a
  cross-component call. The shellbridge's `component` command runs it, and a
  terminal can too. It reads `run/qml/components.json` and resolves in this
  order: a `strillix` provider runs `quickshell -p run/qml/shell.qml ipc call
  component act <slot> <verb> <arg>` (the shell answers `true` when the slot's
  live item ran the verb, `false` when the item is not loaded or lacks it); a
  provider whose `actions.<verb>` is an argv runs that argv with the arg
  appended, detached, no shell; anything else is a taught refusal, exit 1 (`no
  provider fills <slot>`, or `provider <p> declares no <verb>`). It shares
  `lock`'s `ipc_argv` and `run_ipc`, so the shell's IPC spelling lives in one
  place.
- `commands::lock` — `lock` and `lock hint [yes|no]`, the one lock entry
  (`SUPER+ESCAPE`, shellbridge's power-menu `lock`, the `lock` alias). A
  staged rice whose OWN manifest entry carries `lockscreen` is locked by the
  running shell when `/etc/pam.d/aoide-lock` exists:
  `quickshell -p run/qml/shell.qml ipc call lock lock` (the shell answers
  `false` when its PAM conversation cannot start, and then never locks),
  then `ipc call lock secure` until it answers `true` (the compositor
  confirmed the lock) inside 5 s. The budget is 5 s, not 2 s, because each
  `quickshell ipc` client takes about 1.2 s to start on a slow machine (the
  graphical VM test) and a lock is two of them at least. A rice with no
  face, a missing PAM service, and a shell that is down, refuses, has no
  lock host, hangs or never confirms, runs `hyprlock` in its own process
  group; a hyprlock that exits within 500 ms is a failure (`hyprlock-failed`,
  exit 1), never a lock — unless the shell, asked `secure` once more, now
  answers `true` (a confirmation that came late), which is a shell lock.
  `lock --hyprlock` skips the shell: LockHost's hand-over when its PAM
  conversation fails under a held lock — Hyprland
  (`misc:allow_session_lock_restore`) gives hyprlock the held lock, and the
  shell then drops its own without unlocking. `LockedHint` marks the shell's lock only: hyprlock
  sets none (locked-state readers see it by its process), so once hyprlock
  holds the screen a hint still at `yes` — a dead shell lock — is cleared
  (`hintCleared`). `lock hint yes|no` sets logind's `LockedHint` with
  `busctl` on `aoide_protocol::dialog::lock_session_id()`'s session, and
  bare `lock hint` reads it with `loginctl`; `LockHost.qml`
  (pkgs/strillix-shell/qml) sets it on lock and unlock and reads it at start,
  re-locking on `yes`.

## The desktop bridge (`src/bridge/`)

`strillix shellbridge --run` is the desktop process. It calls
`aoide_conduct::shellbridge::run(bridge::VERBS)`: conduct's socket loop owns the
socket, the core verbs and the stage seeding, and `bridge::VERBS` serves the nine
desktop verbs. `bridge/mod.rs` holds the table, `parse_command` and `serve_line`;
`power.rs`, `component.rs`, `rice.rs` and `workspace.rs` hold the work their verbs
drive. `focuswindow`, `focussession` and `power` are listed with their wire shapes
in `docs/Aoide-Wiki/concepts/cli/Doors-and-Nodes.md`.

- `workspaceaction` is a third closed whitelist of its own, two actions
  (`set` / `clear`, `src/bridge/workspace.rs::WorkspaceBinding`) for the bar's bind
  click: zero-session too, and it plans the exact argv the
  CLI takes — `aoide workspace set <ws> <project> [--new]` /
  `aoide workspace clear <ws>` — so no second binding exists anywhere. It is
  the one acknowledged action that resolves a field itself: an omitted
  `workspace` means the FOCUSED workspace, read in-process through the
  compositor adapter (`aoide_conduct::graph::focused_workspace`, the same function the
  CLI's own caller-side resolution uses) because `workspace clear` takes an
  id and no caller can be expected to know the focused one, so the child
  always receives an integer. A host with no adapter is ANSWERED
  (`reason: "no-compositor"`, the CLI's own sentence) rather than dropped,
  because a click is parked on a reply. Its one reply line is
  `{ok, message, action, workspace, project?, data?}` — the CLI's `message`
  and `data` verbatim, keyed by the RESOLVED workspace id, `project` riding
  only on `set`. `workspace` is ABSENT on the `no-compositor` refusal
  (`{ok:false, action, reason, message}` — nothing was resolved to name), and
  both `workspace` and `action` are absent on a `bad-request` refusal
  (nothing parsed to echo) — and a malformed line that NAMES this verb is
  ANSWERED with that refusal rather than dropped, the same rule
  `sessiontrace` holds: one rule for every parked caller.

- `{"cmd":"component","slot","verb","arg"?}` is a cross-component call,
  fire-and-forget (`ShellBridge.qml`'s `component`). `slot` is `[a-z][a-z0-9-]*`,
  `verb` is an ASCII letter then ASCII letters or digits (`openTab` is
  camelCase), and `arg`, when present, is 1 to 64 of `[A-Za-z0-9._-]` and never
  starts with `-`; a name outside those charsets is dropped. The bridge spells
  one line, `strx component <slot> <verb> [arg]`, and runs it through
  `hyprctl dispatch exec`, as `power lock` does: a provider's action never
  inherits this unit's NoNewPrivileges. The audit line is `component <slot>
  <verb>: spawned` or the spawn error.

- **The RICE menu's four verbs (`src/bridge/rice.rs`: `RiceMenu`,
  `RiceMode`, `RiceDraft`, `RiceDeclare`).** Any click on the bar's RICE cell opens the one
  menu; nothing switches until a row is chosen. All four re-exec the sibling
  `strillix` (`daemon::bin::rice_bin()`): `rice` lives there and conduct never
  depends on the rice crate, so the name regexes, the minting and every refusal
  stay the CLI's.
  `{"cmd":"ricemenu"}` is the second READ, answered on its own connection and
  thread like `sessiontrace`: `dispatch_rice_menu` runs `rice list --json`
  under one 4 s budget (`RICE_MENU_TIMEOUT`, under the QML client's 5 s) and
  passes its `data` through (`mode`, `rice`, `draft`, `stagingRice`,
  `declared`, `rices`) with this machine's `host` name and `hostConfig` (the
  file `rice declare --host` rewrites, from `aoide_storage::fs::host_config`,
  null when none resolves) beside it; then, when `drafts_rice` names a rice from the mode
  marker (`stagingRice` first while declarative is locked, since an unlock
  lands there; `rice` first otherwise), runs `rice draft list <rice> --json`
  with the budget left. The answer adds `draftsRice` and `drafts:
  [{name, savedAt, current}]`, or is `{ok:false, reason: cli-failed|no-answer,
  message}`: a child that did not answer is never an empty list, and a failed
  `rice list` starts no second child. With no rice `drafts` is empty and one
  child ran.
  `{"cmd":"ricemode","action":"stage|declarative","name"?}` is the switch,
  fire-and-forget: `stage` takes a `name` that is one argv token and runs `rice
  mode stage <name>`; `declarative` refuses one and runs a bare `rice mode
  declarative` (the CLI resolves the declared twin); anything else is dropped.
  The outcome is a toast through the shared `notify`, on success and failure;
  the audit line is `ricemode action <stage|declarative>: <ok|failed>` and
  carries no rice name.
  `{"cmd":"ricedraft","action":"enter|new|save|drop","name"?}` is
  fire-and-forget like `ricemode`, its outcome a toast through the shared
  `notify`. The set is closed (`RiceDraftAction::from_wire`: `enter` and `drop`
  take a `name` that is one argv token, `new` and `save` refuse one, so a mistaken caller is dropped rather
  than read as success), planned by the pure `rice_draft_plan` and run step by
  step by `dispatch_rice_draft`, each step `strx rice … --json`, unbounded,
  stopping at the first failure — `partial` when an earlier step had
  succeeded, which nothing rolls back. From declarative, `enter` and `new` first
  run a BARE `rice mode stage` (house rule 10: it restores `stagingRice`,
  never the declared rice), and `save` and `drop` are taught refusals,
  `declarative mode is locked — unlock first, then save the draft` / `…then
  delete the draft`: the stage there is the declared rice, `rice draft
  save`/`drop` act on the rice the stage names, and the listing names
  `stagingRice`, so a save would land, and a drop would delete, where the menu
  never looks (unlocking first would change the mode as a side effect).
  Elsewhere `drop` is one step, `rice draft drop <name>`; a missing draft or
  the draft in force is the CLI's refusal, toasted as its own message.
  `enter` is enter-or-create: `rice mode draft` forks the stage into a name
  that does not exist yet, and the bridge cannot tell. The audit line is one
  per action (`ricedraft action <enter|new|save|drop>: <ok|partial|failed>`) and carries
  neither a draft nor a rice name.
  `{"cmd":"ricedeclare","name":"<rice>"}` is the Declare row, ANSWERED on its
  own connection and thread like `ricemenu` because the surface paints the
  outcome as a balloon: `name` is a rice name (lowercase letters, digits,
  hyphens; anything else is dropped with no reply), and `dispatch_rice_declare` runs `strx rice declare <name>
  --host --json`, unbounded like every mutation, which rewrites the host
  config's `habit.rice.declared` line and never rebuilds. `rice_declare_reply`
  answers `{ok:true, message, name, host, changed, stagedDiffers}` or
  `{ok:false, reason, message}`, the message carrying the refusal's what and
  why, `reason` the CLI's own (`not-built-in`, `staged-draft`,
  `host-line-count`, …) or `cli-failed`. The audit line names the rice and
  whether the host file changed: `ricedeclare action <rice>: <ok changed|ok
  unchanged|failed>`.

## What it consumes

`aoide-protocol`, `strillix-rice`, `aoide-conduct` (for the `shellbridge` socket
loop and the `notifications` registry line), `aoide-screen`, `aoide-server` (for `mcp
serve --stdio`'s door loop).

## Start-up migration

Every `strillix` run calls `aoide_storage::fs::migrate_root_once` first (state
and log only). The rice tree moves from `$AOIDE_ROOT/song/` into
`$STRILLIX_ROOT` only when asked: `STRILLIX_MIGRATE=1` (the home activation sets
it on every switch) or `strx migrate`. A plain run with old state prints one
line naming `strx migrate` and moves nothing. `strx migrate --check` reports
the rice state (`settled`, `due`, or `both`) and moves nothing; `strx migrate`
moves it and reports before and after. A `song` that is a marker file (a regular
file, not a directory) means that move is done. Rice state held by both roots
is refused: nothing moves, and `strillix` still runs on the strillix root.

## How it composes

Command paths: onboard/rice/draft/mode/wallpaper/theme/quickshell/screen/
shellbridge/notifications/history/element/migrate/secrets ask/pair ask/pair show/preview/
preview set/preview declare/preview shot/preview tree/preview notes/icon/
apps/clipboard/lock/lock hint — everything that paints, or that only a desktop needs.
`element seed` (L-E1,
docs/architecture/ELEMENTS.md) renders a rice's committed
`elements/*/element.json` (non-QML rice targets — waybar, dunst, anything
with a config file) into `run/elements/`. Never depends on
`aoide-client`/`aoide-conductor` — no A2A client, no TUI; those stay
core-only. May depend on Nix (`rice::widgets`'s `nix-instantiate` over the
shipped generator) — the
one binary allowed to (root `AGENTS.md`, "core is nix-independent").
