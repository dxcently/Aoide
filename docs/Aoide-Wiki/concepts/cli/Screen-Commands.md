---
type: concept
created: 2026-08-19
updated: 2026-08-25
tags: [aoide, cli, screen, computer-use]
---

# Screen Commands — Capture, OCR & Pointer Synthesis

The `screen` command group covers the desktop see-and-act surface: query the
compositor (`screen info`), capture pixels (`screen shot`), recover text from
a capture (`screen ocr`), act by name or pixel (`screen point *`), verify an
act mechanically (`screen diff`), and hand a capture to another agent
(`screen send`). It is a `lyra` command group. Registrations live in
`pkgs/aoide/crates/screen/src/commands.rs`; handlers and domain logic
in `pkgs/aoide/crates/screen/src/{hypr,capture,point,synth,ocr,diff,text,send}.rs`.
See [[Screen-Control]] for the usage narrative; the sidecar field contract,
the image↔screen scale contract, and the reason-code vocabulary are
CONTRACTS.md §8.

Boundaries:

- **Compositor queries and the one warp** shell out to `hyprctl -j
  <monitors|cursorpos|activeworkspace|clients|layers>` and `hyprctl dispatch
  movecursor X Y` (`screen/hypr.rs`). hyprctl failure surfaces as
  `data.reason` `hyprctl-unavailable` / `hyprctl-failed`.
- **Pixel acquisition** shells out to `grim` (`-g "X,Y WxH" -t <fmt> [-q N]
  -s <scale> [-c] <dest>`); region picking shells out to `slurp`
  (`screen/capture.rs`). Failures surface as backend-agnostic `capture-*`
  reasons.
- **OCR** shells out to `tesseract <img> stdout --psm 11 tsv`
  (`screen/ocr.rs`), the only place tesseract is named.
- **Pointer synthesis** is native, in-process `zwlr_virtual_pointer_v1` via
  `wayland-client` (`screen/synth.rs`) — no shell-out. Each command assembles a
  `Seq` and hands it to one `synthesize()` call, which opens a Wayland
  connection (`Connection::connect_to_env()` — `$WAYLAND_DISPLAY` /
  `$XDG_RUNTIME_DIR`), creates the virtual pointer with `seat = None` (falling
  back to a bound `wl_seat` if the compositor kills the connection), walks the
  steps, and on every exit path releases any held button. A press and its
  release never split across two calls (the stuck-button contract).
- **Image decoding** for `screen diff` uses the `image` crate (`image::open`
  → RGBA8), not an external binary.

Path resolution (`pkgs/aoide/crates/storage/src/fs.rs`): the state dir is
`$AOIDE_STATE_DIR` when set to an **absolute** path, else `~/Aoide/state/`.
Captures land in `~/Aoide/state/captures/`, the saved pointer position in
`~/Aoide/state/pointer-pos.json`. The session store `screen shot --session`
reads is `<stage>/sessions.json` where the stage dir is `$AOIDE_STAGE_DIR`
(absolute) else `~/Aoide/song/stage/`. Every JSON write in this group routes
through `aoide_storage::fs::atomic_write` — temp file `<stem>.tmp.<pid>`,
fsync, then rename (symlink-transparent; stale temps swept).

Every command takes `--json`. Without it the CLI prints the human `message`
line on stdout; with it, an envelope `{status, command, message, gated,
changed?, data?}` (`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes:
0 ok, 1 error, 2 usage, 64 not-implemented. All fourteen commands below are
`implemented: true` and `gated: false` in the schema.
Structured errors carry a machine-checkable `data.reason` from the §8
vocabulary (`pointer-*`, `sidecar-*`, `from-shot-*`, `text-*`, `diff-*`,
`capture-*`, `ocr-*`, `hyprctl-*`, plus misc codes); `screen send` is the
exception and tags its own envelope instead.

### lyra screen info

```
lyra screen info [--json]
```

- **Reads:** five `hyprctl -j` spawns — `monitors`, `cursorpos`,
  `activeworkspace`, `clients`, `layers`.
- **Output:** ok → `"<n> monitor(s), <n> client(s) on workspace <name>, <n>
  layer surface(s)"`; data is `ScreenInfo` — `{monitors: [{name, origin, size,
  scale, transform, reserved, usable}], cursor: {x, y}, activeWorkspace: {id,
  name}, clients: [{address, class, title, at, size, pid, ...}],
  layers: [...]}`. `usable` is the monitor rect minus `reserved` (bar/dock
  exclusion zones).
- **Notes:** read-only; writes nothing, touches no synthesis boundary. Any
  hyprctl failure → exit 1, `hyprctl-unavailable`/`hyprctl-failed`.

### lyra screen shot

```
lyra screen shot [--output <name> | --region "X,Y WxH" | --pick | --window <addr> | --session <id>]
  [--format png|jpeg] [--quality 0-100] [--scale F | --fit WxH] [--cursor]
  [--out <path>] [--comment "text"] [--json]
```

- **Reads:** `hyprctl -j monitors` (layout bounds; out-of-bounds regions are
  clamped to it and the outcome says so via `clamped`). `--output` matches a
  monitor name off the same list. `--window` resolves an address off `hyprctl
  -j clients`; `--session` reads `song/stage/sessions.json` (missing file →
  empty registry) and resolves the session's `windowAddress`, falling back to
  its pid, against `hyprctl -j clients`. A full desktop snapshot (cursor +
  clients + layers via `hyprctl -j cursorpos|clients|layers`) is gathered at
  capture time into the sidecar's `desktop` field; a failed snapshot degrades
  the field, never the capture.
- **Writes:** the capture image via `grim` — auto-named
  `state/captures/screenshot-<unix_ts>-<pid>-<seq>.<ext>` (`~/Aoide/state/
  captures/` at runtime) unless `--out` is given — plus a JSON sidecar at
  `<dest>.json` (same stem), written atomically (temp-then-rename). Sidecar
  shape per CONTRACTS.md §8: `schemaVersion: "0"`, `capturedAt`, `origin`,
  `size` (device px), `scale`, `region` (logical px, verbatim), `format`,
  `quality`, optional `monitor`/`session`/`window`/`class`/`title`/`comment`/
  `cursorDrawn`, `desktop`, and `ocr: null`/`diff: null` placeholders.
- **Output:** ok → `"captured <w>x<h> (<fmt>) to <path> — sidecar <path>"`
  (plus clamp/fit/scale warnings); data `{path, sidecarPath, origin, size,
  regionRequested, clamped, format, quality, scale, bytes, desktop}`.
  `changed` lists the image path.
- **Notes:** usage errors (exit 2): conflicting region-source flags,
  `--scale` with `--fit`, unknown `--output`, a region wholly outside the
  layout, `--out` ending in `.json` (collides with the sidecar path).
  `--pick` cancelled in slurp is an **ok** outcome with
  `data.reason: "pick-cancelled"`. A sidecar write failure after a successful
  capture is exit 1 (`sidecar-write-failed`) but still lists the landed image
  in `changed`. `--cursor` is off by default because a drawn cursor registers
  as pixel change to `screen diff`. Not idempotent by design — every call is a
  new file.

### lyra screen point move

```
lyra screen point move <x> <y> [--from-shot <capture>] [--json]
```

- **Reads:** `hyprctl cursorpos` before and after the move. `--from-shot`
  reads the capture's sidecar `<capture>.json` and converts image px to
  screen space (`screen = origin + image_px / scale`).
- **Output:** ok → `"moved to <x>,<y>"`; data `{x, y}`, plus `image: {x, y}`
  when `--from-shot` converted.
- **Pipes to:** one `synthesize()` call emitting relative motion over native
  `zwlr_virtual_pointer_v1` (fires hover/motion inside entered surfaces,
  unlike a warp).
- **Notes:** drift (landing anywhere but exactly on target — a human moved
  the mouse, or the target is off-screen) → exit 1, `pointer-drift` with
  `{x, y, wanted}`. Sidecar problems → `sidecar-missing`/`sidecar-corrupt`;
  invalid sidecar scale → `from-shot-bad-scale`; converted point off-layout →
  `from-shot-out-of-bounds`. Writes nothing.

### lyra screen point click

```
lyra screen point click [<button>] [<x> <y>] [--count 1-10] [--from-shot <capture>] [--json]
```

- **Reads:** `hyprctl cursorpos` (only when the guard `x y` is given).
  `--from-shot` converts the guard off the named capture's sidecar.
- **Output:** ok → `"click <button>"` / `"click <button> x<count>"`; data
  `{button, count}` (plus `image` when converted).
- **Pipes to:** one `synthesize()` call — press+release per click, 60 ms
  between clicks for `--count > 1`.
- **Notes:** the optional `x y` guard fires exactly once, before any click:
  unless the pointer is **exactly** there, refuses with exit 1,
  `pointer-refused` — the one irreversible action gets the one guard. Guard
  requires both coordinates; `--from-shot` without a guard is a usage error
  (exit 2). Button must be `left|right|middle` (default left).

### lyra screen point drag

```
lyra screen point drag <x1> <y1> <x2> <y2> [--button left|right|middle] [--steps 1-200] [--from-shot <capture>] [--json]
```

- **Reads:** `hyprctl -j monitors` (both endpoints must be on the layout,
  checked before anything is pressed), `hyprctl cursorpos` around the
  pre-move and after release. `--from-shot` converts both endpoints off the
  capture's sidecar.
- **Output:** ok → `"dragged <button> from <x1>,<y1> to <x>,<y>"`; data
  `{button, x, y}` (plus `image: {start, end}` when `--from-shot` converted).
  Drift after release is still exit 1 (`pointer-drift`) but its message
  states the button is "not stuck, already released".
- **Pipes to:** a verified move to (x1,y1) (refuses **without pressing** on
  drift), then ONE atomic `synthesize()` call: press, `--steps` interpolated
  relative motions to (x2,y2) (default 20), release — press and release never
  split across calls (stuck-button safety contract).
- **Notes:** endpoint off-layout → exit 1, `pointer-out-of-bounds` with
  `wanted`/`bounds`. Writes nothing.

### lyra screen point hover

```
lyra screen point hover <x> <y> [--settle-ms 1-10000] [--from-shot <capture>] [--json]
```

- **Reads:** `hyprctl` desktop snapshot (clients + layers) before the move;
  `hyprctl cursorpos` before/after; a second snapshot after the settle hold
  (default 500 ms). `--from-shot` as on `move`.
- **Output:** ok → `"hover at <x>,<y> — <n> surface(s) appeared, ..."`; data
  `{x, y, appeared, disappeared, retitled}` (plus `image` when converted). A
  tooltip/menu opening IS a new layer surface — that delta is the command's
  purpose. An empty delta's message explicitly warns that `xdg_popup`s
  (tooltips, menus) are invisible to hyprctl and says to verify with
  `screen diff`.
- **Pipes to:** the same move+verify `synthesize()` path as `move`; drift
  refuses (exit 1, `pointer-drift`).
- **Notes:** writes nothing.

### lyra screen point scroll

```
lyra screen point scroll <dy> [<dx>] [--json]
```

- **Reads:** nothing (no cursor read, no sidecar).
- **Output:** ok → `"scroll <n> notch(es) <down|up>[, <n> notch(es)
  <left|right>]"`; data `{dy, dx, dyDirection, dxDirection, requestedDy,
  requestedDx, clampedDy, clampedDx}`.
- **Pipes to:** one `synthesize()` call emitting per-notch wheel frames (one
  notch = one physical detent: `axis` ±15.0 + `axis_value120` ±120 per frame).
- **Notes:** both axes clamp to ±100 notches per call; a clamp is announced
  in the message and the `clampedD*`/`requestedD*` data keys. Writes nothing.

### lyra screen point idle

```
lyra screen point idle [<samples>] [<timeout>] [--json]
```

- **Reads:** polls `hyprctl cursorpos` every 250 ms until `<samples>`
  consecutive identical samples (default 10) or `<timeout>` seconds (default
  60).
- **Output:** ok → `"idle at <x>,<y>"`; data `{x, y, samples}`. Timeout →
  exit 1, `pointer-not-idle` ("a human appears to be using this mouse").
- **Notes:** read-only; never touches the synthesis boundary. A genuine
  "nobody is touching the mouse" gate — hand tremor breaks the streak.

### lyra screen point save

```
lyra screen point save [--json]
```

- **Reads:** `hyprctl cursorpos`.
- **Writes:** `state/pointer-pos.json` (`~/Aoide/state/pointer-pos.json` at
  runtime), body `{"x": …, "y": …}`, atomic temp-then-rename. One flat file,
  no history — each save overwrites.
- **Output:** ok → `"saved <x>,<y> to <path>"`; data `{x, y, path}`;
  `changed` lists the file. Write failure → exit 1, `pointer-save-failed`.

### lyra screen point restore

```
lyra screen point restore [--json]
```

- **Reads:** `state/pointer-pos.json`; `hyprctl cursorpos` after the warp.
- **Output:** ok → `"restored to <x>,<y> (wanted <x>,<y>)"`; data `{x, y,
  wanted: {x, y}}`.
- **Pipes to:** spawns `hyprctl dispatch movecursor <x> <y>` — a warp, not
  synthesis (returning to a known spot doesn't need to fake a gesture).
- **Notes:** missing file → exit 1, `pointer-nothing-saved`; unparseable file
  → exit 1, `pointer-state-corrupt`. Writes nothing.

### lyra screen point text

```
lyra screen point text <text> --from-shot <capture> [--nth N] [--button left|right|middle] [--dry-run] [--json]
```

- **Reads:** the `--from-shot` capture's sidecar `<capture>.json` — its
  `ocr.words` (written by `screen ocr`) are the only text source searched.
  **Note:** `--from-shot` here names the OCR SOURCE, not a coordinate space —
  OCR word bboxes are already absolute screen coordinates, no transform runs.
  Also reads `hyprctl cursorpos` on the live path.
- **Output:** ok (live) → `"clicked <button> at <x>,<y> on "<text>""`; ok
  (`--dry-run`) → `"would click …"` with no pointer motion at all. Both carry
  data `{match: {nth, text, bbox, centre}, button}`.
- **Pipes to:** live path = the same verified move + guarded click as
  `move`+`click` (drift → `pointer-drift`, guard miss → `pointer-refused`),
  then one `synthesize()` click.
- **Notes:** usage errors (exit 2): missing `<text>`, trailing unquoted
  words (quote a multi-word phrase as one argument), missing `--from-shot`,
  bad `--nth`. Matching is case-insensitive against whole words / consecutive
  same-line words. Errors: no `ocr` block in the sidecar → `text-no-ocr`;
  malformed ocr block → `sidecar-corrupt`; zero matches → `text-not-found`
  (data `{needle, wordsSearched}`); more than one without `--nth` →
  `text-ambiguous` (data `{needle, candidates: [{nth, text, bbox, centre}]}`
  ordered top-left-first). OCR words carry tesseract's punctuation verbatim —
  `"Save:"` won't match a search for `"Save"`.

### lyra screen ocr

```
lyra screen ocr <capture> [--json]
```

- **Reads:** the capture image and its sidecar `<capture>.json` (required —
  `origin`/`scale` drive the coordinate transform). Shells out to `tesseract
  <img> stdout --psm 11 tsv` (PSM 11 sparse text, chosen by measurement over
  the PSM 3 default).
- **Writes:** the same sidecar, atomically rewritten with `ocr` populated:
  `{text, words: [{text, conf, bbox}]}` — bboxes in ABSOLUTE SCREEN
  coordinates (`origin + image_px / scale`); every other sidecar field passes
  through untouched.
- **Output:** ok → `"ocr: <n> word(s), mean confidence <c> — sidecar
  <path>"`; data `{sidecarPath, wordCount, meanConfidence, text}`; `changed`
  lists the sidecar.
- **Notes:** TSV filtering: word-level rows only (`level == 5`), empty text
  dropped, confidence below 10.0 dropped. Text assembly: space-joined within
  a visual line (vertical bbox overlap — PSM 11 splits one line's words
  across block numbers), newline-joined across lines. Errors: missing capture
  → `capture-not-found`; missing/corrupt sidecar → `sidecar-missing`/
  `sidecar-corrupt`; spawn/run failure → `ocr-unavailable`/`ocr-failed`;
  write failure → `sidecar-write-failed` (exit 1; the OCR itself succeeded).
  Re-running is idempotent in effect (same transform, same sidecar field).

### lyra screen diff

```
lyra screen diff <before-capture> [--settle-ms 0-60000] [--threshold 0-255] [--out <path>] [--json]
```

- **Reads:** the before-capture image and its sidecar `<before>.json` —
  origin/scale/size/format/quality/region are recovered from there, never
  re-asked (a pre-`region`-field sidecar falls back to inverting `size`
  through `scale`, lossy ±1 px for downscaled captures, announced when
  scale < 1.0). Decodes both images with the `image` crate. Sleeps
  `--settle-ms` (default 250) before the after-shot.
- **Writes:** re-shoots the identical rect/scale/format/quality via grim and
  the same `write_capture` tail as `screen shot`: after-image at `--out` or
  auto-named under `state/captures/`, plus its own sidecar — the SAME result
  object is then written into the **after**-capture's sidecar `diff` field
  (never the before-capture's), both writes atomic.
- **Output:** ok always covers "nothing changed" — `"no change within
  threshold <t> after <ms>ms settle"` is present-tense success, not an error.
  Data `{changed, changedFraction, changedRect (screen px) | null,
  changedRectImage | null, appeared, disappeared, retitled, afterPath,
  sidecarPath}`; `changed` lists the after-image and its sidecar. A pixel
  counts when any RGB channel's delta exceeds `--threshold` (default 8;
  alpha ignored). The inventory delta diffs the before-sidecar's `desktop`
  snapshot against the after-capture's fresh one (null, with a note, when
  either side lacks one).
- **Notes:** errors: missing capture → `capture-not-found`; bad sidecar →
  `sidecar-*`; invalid sidecar scale → `from-shot-bad-scale`; undecodable
  image → `diff-decode-failed`; before/after size disagreement (layout
  changed mid-flight) → `diff-size-mismatch`; `--out` ending in `.json` is a
  usage error.

### lyra screen send

```
lyra screen send <capture> (--session <id> | --agent <name>) [--comment "text"] [--yes] [--json]
```

- **Reads:** the capture (must exist; canonicalized to an absolute path so a
  receiving agent in another cwd can open it literally) and, leniently, its
  sidecar `<capture>.json` — a missing/corrupt sidecar never fails the send,
  it just tags `data.sidecarStatus: "missing"|"corrupt"` and drops the
  enrichment. The stored `comment` and OCR `text` enrich the message;
  `--comment` overrides the stored one (blank counts as absent). For
  `--agent`, the registry at `state/a2a-agents.json` (`~/Aoide/state/
  a2a-agents.json` at runtime) resolves the name.
- **Output:** the composed message is
  `screenshot: <abs-path>` + optional `comment: …` + optional `ocr text:\n…`.
  Envelope data `{target: {kind: "session"|"agent", id}, message, state,
  sidecarStatus, path, inner}` — `state` is `"held" | "delivered" |
  "sent-to-agent" | "error"`, and `inner` nests the underlying door's whole
  `data` (a failed send's real reason lives at `data.inner.reason`, e.g.
  `"unknown-agent"`). The inner `status`/`message`/`changed` ride through
  unchanged — a failed delivery is never reported as ok.
- **Pipes to:** `--session` routes through the exact `graph send --id <id>
  --submit [--yes] -- <message>` handler (`--submit` is fixed on): held
  pending by default, delivered on `--yes`, on `AOIDE_CONDUCT_AUTOGATE`
  ∈ {1,true,yes,all}, or when the sender is the target's parent session —
  delivery writes into the target session's control socket
  `$XDG_RUNTIME_DIR/aoide/session-<id>.sock`, injecting the text into the
  conducted PTY's stdin. `--agent` routes through the `a2a agent send`
  driver (`aoide_client::commands::handle_agent_send`): a `curl` shell-out
  POSTing the JSON-RPC `message/send` body to the registered agent's URL —
  delivers immediately, no hold; non-200, curl failure, or a JSON-RPC
  `error` in a 200 response all surface as errors at `data.inner.reason`
  (`send-failed`/`send-http-error`/`agent-error`). Exactly one of
  `--session`/`--agent` is required; both or neither is a usage error. The
  global `--audit-log` flag is honoured and forwarded to the session gate.
- **Notes:** `gated: false` in the schema — the gating lives inside the
  `graph send` door it reuses, not on this command. Writes nothing itself.

## Related

- [[Screen-Control]] — the concept page this contract backs
- [[Session-Graph]] — `graph send`, the session gate `screen send --session` reuses
- [[Terminal-Commander]] — conducted sessions and their control sockets
- [[aoide-cli]] — the CLI envelope, exit codes, and `--json` convention
