# Songbook Learnings

Cross-cutting observations that apply across songs, not any one song's
`design/` folder. The agent reads this before every ricing iteration and
appends after declare/reject decisions (the write-back that is the "self" in
Self-Ricing). Sparse today — this is the first entry.

## `default` (retired 2026-08-14) — the Windows-7/Aero aesthetic

The shipped standard song was originally named `default`, not `sonata`.
Retired outright when `sonata` became the shipped baseline; its files are
git-recoverable (last present at `song/songbook/default/`), not preserved
in the tree.

What it recorded, distilled from its `design/intent.md` Iteration Log:

- **Palette:** Catppuccin Mocha (base16) — chosen for wide ecosystem
  support (Stylix consumes it natively), legible bg/fg contrast, and being
  well-represented in agent training corpora (so an agent composing a new
  song `--from default` could reason about transpositions by name).
- **Component tier:** left fully null — every surface fell back to the
  palette, a deliberately clean baseline to compose from.
- **Aesthetic:** a Windows-7-sidebar homage — the gadget dock's right-edge
  column of gadgets (TERMINALS/DAG/CLOCK/METERS) in ASCII/box-drawing chrome
  (`╔═[ TITLE ]═╗`, tree-limb rules, `[▓▓▓░░░]` gauges), realized as what
  later became the Gadget-Dock. This is the aesthetic the user declared and
  the agent was meant to inherit across generations.
- **Design grammar:** `default` also owned the cross-cutting Pantheon
  wireframe-depth + glyph grammar (hollow 3D outline stacks, neon leaders, a
  vanishing point) layered over the Aero-glass base — relocated, not
  deleted, to `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md` as
  historical reference. `sonata` draws its own, deliberately divergent
  grammar instead (`song/songbook/sonata/design/greek-grammar.md`).

Full history — every dated log line, the actual `rice.nix`/`livery.json` — is
in git; `git log --follow -- song/songbook/default/` finds it.

## `cadenza` staged live (2026-09-26)

- **`rice stage <song>` reads the RUNTIME songbook**
  (`$AOIDE_ROOT/song/songbook/<song>/widgets`), not the checkout. A song
  written straight into a checkout stages its stale compose scaffold until
  the runtime copy is refreshed from the committed tree. The slot owner map
  (`manifest.json`) comes from `nix eval` of `AOIDE_FLAKE_ROOT`, so it only
  sees committed files.
- **`rice stage <song>` does not survive a `RICE` toggle.** The bar's RICE
  cell runs `rice mode declarative` then `rice mode stage`, and the latter
  re-stages the song recorded in `stage/mode.json`, which `rice stage`
  left on the previous song. Pin with `rice mode stage <song>` instead.
- **Staging swaps every slot a song provides at once.** "One surface at a
  time" is a checking order, not a staging order.
- **The preview canvas cannot catch edge clipping.** A pane whose title is
  cut into its top rule at y=0 loses the glyph tops at the window edge; the
  canvas shot looks the same, so it passed review. Give a surface root a
  half-cell top inset.
- **Measured at rest, live:** quickshell 1.4% CPU over 10s and 266 MB RSS
  with tier-0 glow on, under a 99%-busy machine (noisy). Glow stays on.
- **Derived ties are empty on a normal desktop.** Deriving jack links from
  `graph.json` + `sessions.json` `windowAddress` works, but most spawned
  children are windowless subagents, so no edge joins two jacks. The
  switchboard stays bare live until an agent spawns a windowed session on
  another workspace or core publishes `ties`; prove the derivation with
  edges added in the preview root, and say so.
- **A wallpaper is a cover until song wallpaper slots are hosted.**
  Generate it with a helper in the preview (fixed seed, exact viewport
  size), commit the PNG, and `lyra cover set` the runtime songbook's copy.
- **An inner glow lit from the resting rule colour is invisible.** At
  0.10 alpha of `dim` on CRT black the edge rose by ~6/255. Light the glow
  from `title` and let alpha, not colour, carry rest vs focus.
- **Staging reaps preview canvases.** `rice mode stage` kills stray
  quickshell processes, so an open preview canvas goes with them. Don't
  restage while anyone is shooting in a canvas.
- **`run/qml/songs/<song>/` keeps deleted helpers.** The stage sync copies
  and overwrites but never removes, so a helper deleted from the song stays
  in the generated copy. It's harmless while nothing imports it; don't read
  it as proof the file is still in use.
- **Raw OSC writes race the program on the pty.** Pushing colours by
  writing escapes to `/dev/pts/N` can interleave with the program's own
  output and get eaten. Use the terminal's control socket (`kitty @`)
  instead.
- **Check before toggling a surface to screenshot it.** `aoide:dock` is a
  toggle; firing it to "open" the board closed the one the user already had
  open. Read the state (or ask) first.

## Song widget costs: Canvas threads and the herald clock (2026-10-07)

- **Heavy Canvas work never runs on the GUI thread.** A Context2D
  `shadowBlur` of 34 costs 3.9 s per paint at 330x200 and 12 s at the dock's
  562x1043 frame (12 ms with no blur), and a default Canvas paints on the GUI
  thread. Cadenza's Pane glows froze the whole shell for tens of seconds per
  scene build: the bar clock, the herald sweep, queued shortcuts and the
  `rice stage` reload IPC all waited. `renderStrategy: Canvas.Threaded` gives
  byte-identical pixels; the paint moves to the engine's render thread, so the
  result lands late.
- **A hidden Canvas still paints.** `visible: false` cost the same CPU as a
  visible one: 4.0 s of user time at 330x200 either way. Cadenza's
  `innerGlow: false` therefore instantiates no canvas (a `Loader` with
  `active`), and the property now saves the paint it names.
- **Threaded canvases share one render thread.** Every `Canvas.Threaded` in an
  engine paints on the same thread, one paint at a time, and repeated
  `requestPaint()` calls are not coalesced: ten height changes of one 330x200
  glow queued eleven 4 s paints (the last done 46 s in), and every other
  Threaded canvas, CoverPcb's included, waited behind them. A resize also
  repaints a canvas by itself, so the debounce is a single-shot `settle`
  Timer plus `if (settle.running) return` in `onPaint`: the same burst costs
  two paints (the last done 8.6 s in), and the skipped paints leave the last
  texture on screen, stretched. CoverPcb takes the same debounce on its worker
  request: three size changes 200 ms apart queued three boards, and the last
  landed 6.7 s after the final size; one request now lands 3.3 s after it.
- **Seconds of JS belong in a WorkerScript, and so does the canvas that
  strokes the result.** CoverPcb generated its board in `onPaint`: 3.3 s per
  output. It now answers from `CoverPcbWorker.js` with an identical board;
  stroking the board is another ~110-150 ms, so that canvas is Threaded too.
  Measuring only the generator (17 ms) hides that second cost. The worker is
  `.js`, not `.mjs`: a WorkerScript runs an `.mjs` as a strict ES module,
  while `design/trace.test.js` lifts the same block through `new Function`
  (sloppy), so `.js` keeps the test and the shell on the same semantics. A
  WorkerScript prints one `QObject::connect(QJSEngine, QtObject): invalid
  nullptr parameter` warning; it is Qt's own and harmless.
- **A transient toast's clock anchors to its own arrival.** `now + timeoutMs`
  at first sight replays the stored ledger as fresh toasts on every QML
  reload, which is every song switch that writes bodies. The deadline is
  `Date.parse(receivedAt) + timeoutMs` (first sight only when it does not
  parse), and a record already past it lapses inside `ingest`, because the
  500 ms sweep starts only after something shows. Sonata, fugue (quodlibet
  borrows it) and cadenza carry the same clock. Fixture toasts carry
  `timeoutMs: 0` for the same reason: their dates are fixed and long past.
- **Measure GUI-thread stalls offscreen, with the shell's own binary.**
  quickshell with `QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software`, a
  scratch `XDG_RUNTIME_DIR` and `AOIDE_ROOT`, `WAYLAND_DISPLAY` unset, and a
  10 ms Timer logging every gap over 60 ms. The floor of a bare window is
  15 ms. The gap does not see the render thread: also log `onPainted` per
  canvas (count, and time to the last one), where a queued backlog shows. A
  `PanelWindow` root cannot load there (no layer-shell backend): swap it for
  a `FloatingWindow` in a scratch copy and the clock logic runs
  unchanged. The software backend says nothing about the GPU path, so
  `Canvas.Threaded` on the live shell stays unmeasured until a live pass.
- **A built-in song's runtime copy is read-only.** `~/.aoide/song/songbook/
  sonata` is seeded from the nix store at 0444/0555. `chmod u+w` the one file
  and write it in place (a temp-and-rename needs the directory); `lyra rice
  declare` then carried it into the checkout like any other song.
- **`lyra rice declare` copies; it does not delete.** A file renamed in the
  runtime tree arrives under its new name, and the old name stays in the
  checkout until it is removed there (`CoverPcbWorker.mjs` to `.js`).
