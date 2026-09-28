# cadenza — Design Intent

**Song:** cadenza (composed from `sonata`, then drawn from nothing — no
sonata widget body or gadget slot is reused; cadenza draws its own)
**Register:** a hacker console on a green-phosphor CRT, wired like a
telephone switchboard. Box-drawn termui panes with the title cut into the
top rule, braille line charts, block sparklines, bar gauges, a monospace
grid everywhere; workspaces are jacks on a switchboard, joined by tie lines,
lit by lamps.
**References:** `refs/ref-termui.png` (the pane grammar: box rules, green
titles, braille/dot charts, sparklines, a gauge) and `refs/ref-crt.jpg` (the
light: green phosphor on near-black, a faint bloom, nothing else).
**Polarity:** dark. Needs a livery field the lane does not read yet (§5).
**Cover:** a static circuit board on the CRT black (§3.9): dim copper-green
tracks, pads and vias, generated from the palette. Staged live with
`lyra cover set`; the wallpaper note stays `null` until the cover is
declared for the rebuild.
**Glass:** none. No compositor blur, no hyprglass (§5).

Companion files: `coverage.md` (every bridged surface and cadenza's answer),
`refs/` (the references). Data names follow the core-seams proposal
(workspace binding, `ties`, `state/usage/now.json`, the board, `boardpost`).

---

## 1. The aesthetic in one paragraph

Everything is a terminal. Every surface is a termui **pane**: a single-line
rule (`┌─ TITLE ───┐ │ │ └───┘`) with the title cut into the top edge in
phosphor green, content on a monospace character grid, no radius (the switchboard's round pads aside), no glass,
no drop shadow, and one gradient only: the phosphor glowing just inside a
pane's rule. Colour is spent the way a 16-colour terminal
spends it: green is the default voice, amber is the one live thing, and a
small set of terminal contrast colours each carry exactly one kind of
highlight. The bar is a switchboard: workspaces are **jacks**, the real
edges between them are **tie lines**, the bar's bottom rule is the **trunk**
every tie hangs off, and activity lights a **lamp** that runs down a line.
At rest nothing moves.

### Vocabulary (one name per thing)
| word | what it is | layer |
|---|---|---|
| **jack** | a workspace, drawn `[3]` on the switchboard | paint only; the data noun is core's *workspace* (an integer) |
| **tie** | a real edge between two workspaces, kind `project` or `spawned` | core's noun (`graph.json` `ties`); the paint draws it as a tie line |
| **trunk** | the bar's bottom rule; every tie line drops from it | paint only |
| **pad** | the outline a jack's number sits in; on the cover, a copper ring | paint only |
| **lamp** | the lit dash that runs a line when its activity advances | paint only; core publishes `activeAt` (until then, `hooks.json` `updatedAt`) |
| **track** | a copper line on the circuit-board cover | paint only |
| **board** | the dock: the per-project message board | core's noun (`aoide project board`) |
| **pane** | one termui box | paint only |

"node" is never used for a workspace (it means a mesh machine); "trace" and
"channel" are never used for paint (they mean the session trace and the
Conductor Channel).

## 2. Grammar — the drawing vocabulary

### Faces and the grid
- One face for paint: **JetBrainsMono Nerd Font Mono** (installed; box
  drawing and block elements and braille U+2800–28FF all native, no
  fallback). No proportional text anywhere.
- One face for the terminal: **ShureTechMono Nerd Font Mono**, set as
  `aoide.livery.fonts.monospace` — squared-off, single-weight, a console
  rather than the desktop's humanist serif. A pane is the song's cell, the
  terminal is where an agent lands; both are fixed-pitch machine faces.
  Coverage checked against the installed face: box drawing and rules
  (U+2500, U+2502, U+250C, U+2510) and block/shade (U+2588, U+2591). This
  tier is baked — a face change lands on a rebuild, never on `rice stage`.
- Every pane lays out on a character cell measured once from `FontMetrics`
  (never hard-coded); pane sizes are whole cells, so a rule always closes.
- Three type tiers: **title** (bold, accent, UPPERCASE in the rule),
  **body** (regular, fg), **dim** (regular, base03 — labels, axes, times).

### Pane chrome
```
┌─ AGENTS ─────────────── 3/5 ┐     title cut into the top rule, left
│ ● rook      working  12s    │     optional right-hand stat in the rule
│ ◐ minerva   awaiting  2m    │
└─────────────────────────────┘
```
- Rules are 1px `Rectangle` hairlines (`base03` at rest, `accent` when the
  pane has focus), NOT box-drawing text — text rules cannot close at
  fractional scale. Box-drawing glyphs are used INSIDE panes (tree limbs
  `├─ └─`, separators) where the cell grid is guaranteed.
- A **borderless block** (termui's magenta one) holds a single message that
  is not a pane — a herald toast, one board item.
- Panes are opaque `base00` at 0.94. Black glass, not frost.
- **Inner glow:** the phosphor green (`title`) blooms inward from all four
  edges and fades softly to nothing, visible ~28px in — the phosphor lit
  just inside the tube's frame (`refs/ref-inner-glow.png`). It runs all the
  way round, under the title and the stat too, and its corners are round and
  even. It is `title` whatever the rule's colour, so a box at rest is lit
  too; a focused pane's is about twice as bright. It is a gaussian baked
  once by one `Canvas` in the kit's Pane, behind the content: painted on a
  size or palette change only, never per frame. Focus and reveal move the
  canvas's opacity and never repaint it. No shader, no live blur, nothing
  redrawn at rest. It is the only soft fall-off in the song.

### Instruments (termui's widgets, re-drawn)
| instrument | glyphs | used by |
|---|---|---|
| gauge | `█▉▊▋▌▍▎▏` eighth-blocks on a `░` track, % right-aligned | CPU, memory, battery, context window, volume |
| sparkline | `▁▂▃▄▅▆▇█` one column per sample | per-jack CPU, tokens/min |
| braille chart | U+2800 block, 2×4 dots per cell | cost over a range |
| bar chart | full-block columns, value on the base | token share per workspace/project |
| list | `[n]` index in dim, text in fg; the SELECTED row's index turns amber (the one live thing) | launcher, tray, sinks |
| state lamps | `●` working · `◐` awaiting · `○` idle · `■` stopped · `✕` failed | sessions everywhere |

### Glyphs (the icon set)
Icons are **Nerd Font glyphs** from the one face: text, one cell wide, on
the grid. Never an image, never an icon theme, never a tinted picture.
- A glyph stands in for a word label, and only where a label is needed to
  say what a value is. A value that reads on its own stays bare: the
  clock, the `$` figure, the tray, and every list row (launcher, clip,
  ledger, herald, board).
- It takes the colour of the value it labels. It never introduces a colour
  job of its own.
- The glyphed bar cells: agents `󰚩 3/13`, notifications
  `󰂚 2`, volume `󰕾 62%` (`󰖁` muted), Bluetooth `󰂯` (`󰂲` off), network
  `󰈀` wired, `󰖩` wifi, `󰖪` none, battery `󰁹 88%` (the glyph follows the
  level), and the rice mode `󰏘 stg`. `⏻` stays. Pane titles keep their
  words.
- The glyph table lives in ONE place, the kit (`Kit.js`), so every surface
  draws the same icon for the same thing.

### Colour — roles and highlights (hexes in `rice.nix`)
The ground and the ramp are phosphor; each highlight colour has ONE job, so
a colour on screen always means the same thing:

| colour | slot | job |
|---|---|---|
| phosphor green | fg / base05 | body text, idle rules, chart ink |
| bright phosphor | accent / base0B | titles, the focused pane's rule, the active jack |
| dim phosphor | base03 | axes, labels, rules at rest, timestamps |
| **amber** | hot / base0A | the ONE live element: the lamp, the traced session, a text cursor. Never two amber things at rest |
| **red** | urgent / base08 | blocked, failed, low battery, a summons waiting on you |
| **orange** | base09 | warnings short of trouble: costs past a threshold, a `costPartial` figure |
| **cyan** | base0C | numbers worth reading: counts, token totals, durations |
| **blue** | base0D | paths, project names, workspace numbers in running text |
| **magenta** | base0E | a person's words: your posts, mail from a human |
| **white-hot** | base07 | a search match / the selected row's text |

Every text colour clears 4.5:1 against both `base00` and `base01` (WCAG
relative luminance):

| colour | hex | vs base00 | vs base01 | vs base02 |
|---|---|---|---|---|
| fg / base05 | `#86f2a0` | 14.46 | 13.61 | 11.55 |
| accent / base0B | `#39ff6a` | 14.90 | 14.03 | 11.90 |
| dim / base03 | `#4a905d` | 5.17 | 4.86 | 4.13 |
| mid / base04 | `#62b377` | 7.82 | 7.36 | 6.25 |
| amber / base0A | `#ffb000` | 10.89 | 10.25 | 8.70 |
| red / base08 | `#ff4d4d` | 6.10 | 5.74 | 4.87 |
| orange / base09 | `#ff7a2e` | 7.67 | 7.22 | 6.13 |
| cyan / base0C | `#3fe0d0` | 12.15 | 11.43 | 9.70 |
| blue / base0D | `#4aa8ff` | 7.92 | 7.45 | 6.33 |
| magenta / base0E | `#d65cff` | 6.49 | 6.11 | 5.19 |
| bright / base06 | `#b6ffc8` | 17.22 | 16.21 | 13.75 |
| white-hot / base07 | `#e4ffea` | 18.80 | 17.70 | 15.02 |

`dim` is the floor: it carries labels and timestamps, so it is held just
above 4.5 on the ground and the block fill. `mid` sits one clear step above
it. On the `base02` selection band, dim text falls to 4.13, so a selected
row draws its text in `match`, never `dim`. `borderInactive` (`#2e5a3a`) is
a window border, not text, and stays darker.

### Motion
- At rest nothing in the UI animates. No blinking cursor unless a field has
  focus. No scanline roll, no flicker, no idle shimmer. The one surface that
  is never fully still is the cover: the board keeps a floor of light with
  nothing running (§3.9 — a tube that reads dead is the one thing this key
  does not do).
- **Lamp:** when a jack's or a tie's activity advances between two reads
  (`activeAt`, or `hooks.json` `updatedAt` until core publishes it), a
  12px amber dash runs the tie line (or drops from the
  trunk into the jack) in 600ms linear, then the line goes dark. One
  `NumberAnimation` on one `Rectangle`; at most one lamp in flight per line,
  later advances coalesce into it. The walker under that dash is `Trace.at`
  (`widgets/Trace.js`) — the same one the cover's pulses run on.
- **Board light:** the cover's own motion — slow scans of light that arrive
  at a pad, bounce back or die, and pads that breathe in their agent's state
  colour. One engine, three looks, all of it data: §3.9.
- **Reveal:** a pane draws its rule clockwise in 160ms then fills (the CRT
  "paint"); closes in 120ms. Within sonata's documented bands.
- **Board lines** appear whole — no typewriter effect.

### Glow — the budget
Text should "glow a little, only if cheap". Three tiers, drawn by
`GlowText` (`design/kit.md` §3):

1. **Tier 0 (`outline`, the default):** the text's own `style: Text.Outline`,
   `styleColor` its colour at 0.18. There is no offscreen buffer and no
   shader.
2. **Tier 1 (`bloom`, titles only, baked):** a hidden copy of the title fed
   to one `MultiEffect { blur }`. It is rendered once and reused until the
   title text changes. Never on an animating item, never on a list delegate.
3. **Never:** an animated blur, a per-line blur on board rows, a
   full-surface bloom shader.

**Measured** on a board-shaped bench: a 200-row list of the board's row
anatomy (time, lamp, name, text with untrusted words) under 4 pane titles.
It ran in the preview canvas with its own root, 8s per window after a 6s
settle, sampling CPU from `/proc/<pid>/stat` and frame work
(sync+render) from `qt.scenegraph.time.renderloop`. Scroll is a continuous
contentY sweep of the whole list every 4s.

**These numbers are software-rendered and relative-only.** They come from
the offscreen canvas (`QT_QPA_PLATFORM=offscreen`, the Qt Quick *software*
renderer). They rank the tiers against each other; they are not the GPU cost
on the desktop.

| glow | idle CPU | idle frames | scroll CPU | scroll frame work p50 / p95 / max | RSS |
|---|---|---|---|---|---|
| off | 0.0% | 0 | 25.9% | 3 / 3 / 4 ms | 113–114 MB |
| tier 0 on rows and titles | 0.0% | 0 | 50.1% | 6 / 8 / 11 ms | 112–115 MB |
| tier 0 rows + tier 1 titles | 0.0% | 0 | 49.6% | 6 / 8 / 11 ms | 112–115 MB |
| tier 1 on every row (the "never") | — | — | 56.2% | 6 / 8 / 12 ms, uneven frame rate | 121 MB |

**Verdict.**

- At rest, every tier costs nothing: zero frames and 0.0% CPU. The static
  scene is not redrawn, blurred titles included.
- Tier 1 on titles is free relative to tier 0. The same scroll cost, the
  same memory, because the four blurred titles are baked once. It stays, on
  titles only.
- Tier 0 on scrolling rows roughly doubles the software renderer's per-frame
  work (2.6 → 6.3 ms mean). That is still well inside a 16.7 ms frame, with
  no dropped frames at 62 fps and p95 of 8 ms, so it does not trip the
  "visible hitch" line. Tier 0 therefore stays the default, board rows
  included, **provisionally**. The software renderer draws an outline as
  extra glyph passes, which is its worst case; the GPU number decides. Until
  a GPU run confirms it, a long scrolling list may pass `glow: "off"` on its
  rows at no loss to the look of its titles.
- Tier 1 on rows costs more CPU and ~6 MB more for 200 rows, and paces
  frames unevenly. It stays banned.

The GPU re-run is the same bench with the real canvas.

## 3. The surfaces

### 3.1 Bar — the switchboard (`bar`)
One 28px line in the tmux/termui idiom, left to right:
- `[⏻]` — the power key (opens `powermenu`).
- the **switchboard** (§3.2), the widest element.
- the active window title, dim, truncated.
- right cells on the grid, glyphed per §2 Glyphs: `󰚩 3/5` and `󰂚 2`,
  each opening the board on its tab (OVERVIEW, NOTIF); then `󰕾 62% 󰂯`
  (one cell, one pane: sound and bluetooth), `󰈀` (the network pane),
  `󰁹 88%`, the tray, `󰏘 stg` (the rice-mode toggle), and the clock
  `14:02:31` (the calendar pane). CPU and spend are not on the bar; they
  live on the board's SYS tab.
- **The rice-mode toggle is one click.** A click sends the toggle once and
  the cell reads a dim `󰏘 …` (padded to the word, so nothing moves) at
  once; further clicks do nothing until the mode (`song/stage/mode.json`
  `mode`, through `livery.riceMode`) actually changes, or 10s pass. A
  switch reloads the shell; a toggle that changed nothing in 10s simply
  returns the cell to its word and says no more.
- **The sound + bluetooth pane.** One pane, two sections. SOUND: the output
  and input with volume and mute (click toggles mute, wheel steps volume),
  and the default device picker. BLUETOOTH: a power toggle `[on]`/`[off]`,
  then the known devices, each row `󰂯 name  connected`/`paired` with a
  click to connect or disconnect. With bluetooth off or no adapter, the
  section says so in one dim line. The cell's `󰂯` is dim when bluetooth
  is off, ink when on, title when a device is connected.
- The bar's bottom rule is the **trunk**.

### 3.2 The switchboard (inside the bar)
- **Every jack is a pad.** Each workspace number sits inside a round pad,
  the way a board's pads and vias are round: a 1px circle hugging a
  one-digit number (the bar's text height across), stretched to a pill of
  the same height for two digits. It is drawn for every jack whether or
  not it is tied, so the row reads as a line of pads on a board. Pads are
  the one rounded shape in the song; every pane and block keeps radius 0.
  An occupied jack's pad is `ink` with the number `ink`; an empty
  jack's pad and number are `dim`; the active jack's pad is filled `title`
  with the number in `ground`; a jack with an awaiting/blocked session has
  a red pad.
- A bound jack carries its project after it, blue: `[2]aoide` (the core's
  `workspaces[].project`, the binding; an unbound jack carries none).
- **Special workspaces draw no pad.** A negative workspace (`-98` scratch,
  `-99`) is never a jack, whether the row comes from core's `workspaces` or
  from the derivation, and a tie with a negative end is dropped from the
  drawing. Their sessions still count in the agents and notification cells.
- **Tie lines are a schematic**, drawn in the band between the jack row
  and the trunk (the bar's lower ~9px). Every line is 2px in phosphor fg
  (idle rule ink), never dim: a tie must read at 1:1.
  - **Leads:** a tied jack's pad grows a short lead from the middle of its
    bottom edge down into the band. Wires start and end on leads; an
    untied jack's pad has none.
  - **Bus:** a `project` tie is a solid wire on the first lane, joining
    the leads directly. The core publishes a clique for a project shared by
    3+ jacks; the paint draws it as ONE bus through all of its pads, not
    n² lines.
  - **Spawned wires:** a `spawned` tie drops from its lead, runs dashed on
    a lower lane, and rises into the other lead — the schematic's
    side-wire.
  - **Junctions:** a filled dot `●` marks every point where a wire meets
    another (a spawned wire leaving a bus, two buses meeting), the
    schematic convention; a plain crossing without a dot is not a
    connection.
  - At most 2 lanes below the jacks; a further tie collapses into a
    `+n` badge (cyan) on its left jack.
- Lamps run on activity (§2 Motion).
- **A working pad breathes.** While any session on a jack has `hooks.json`
  phase `working`, its pad breathes slowly: the ring toward `bright`, a
  faint `title` fill (the active pad's fill toward `bright`), 0 → 1 → 0
  over 2s on a cosine, and it stops the moment no session on it works. It
  follows the working state, never a tool call. One shared breath drives
  every working pad, stepped at 12.5 fps (the bar redraws 12.5 times a
  second while an agent works, not every vsync), and nothing runs when no
  pad works. An urgent pad keeps its red ring; amber stays the lamp's. The
  session's jack is the same join the ties use (below); no window, no
  pulse.
- **A send runs a lamp.** The core publishes recent conductor sends in
  `graph.json` as `sends: [{from, to, at}]` (a short ring, newest last). An
  entry new between two reads (never on the first read that carries a
  ring, so a ring that appears never fires its backlog) runs a lamp from
  the sender's jack to the receiver's: along a standing tie that joins the
  two when one is laid, otherwise along a **transient** wire, `dim`, on a
  lane free over the span (else down to the trunk and along it), drawn for
  the lamp's run only and gone with it. It is never a tie. A send within
  one jack, or with one end on no jack, lights that jack's own lamp.
  Until core publishes `sends`, nothing runs.
- Hovering a jack opens the **jack insight pane** (§3.4).
- **Binding a jack to a project** happens in that pane, on its PROJECT row:
  ```
  ─ project ─────────────────────────
  [aoide] [melete] [mneme] [clear]
  [+ new]
  name: scratch▏             ⏎ bind  esc
  ✕ workspace 2 is not bound to any pr…
  ```
  One chip per registered project (`projects.json` order), the bound one lit
  (a `title` band with `ground` text, the active pad's own fill), `[clear]`
  while the jack is bound, and `[+ new]`. Every click sends ONE
  `workspaceaction` line pinned to that pane's jack (`"workspace": N`), so
  it binds the pad the pane belongs to, never the focused one: a chip is
  `{action:"set", project, workspace}`, `[clear]` is `{action:"clear",
  workspace}`, and `[+ new]` opens an inline `name:` field (Enter sends
  `set` with `"new": true`, Escape or a click outside cancels; a name must be
  non-empty, at most 40 characters, with no whitespace, no control
  character and no leading `-`, and the field's hint says which rule a typed
  name breaks). The bar layer never takes keys, so the field holds a
  compositor focus grab on the pane's popup for exactly as long as it is
  open. The clicked chip turns amber while the line is in flight; the lit
  chip moves only when `graph.json` rewrites with the new `project`, and the
  amber mark clears then or after 5s. When the bridge answers the line
  (`bridge.workspaceAction`), an `ok: false` reply becomes one dim line with
  core's own `message` (else `reason`), plain text. **Honest partial:** the
  lane's bridge answers no `workspaceaction` today, so the line goes
  fire-and-forget (`bridge.sendCommand`); success still shows through
  `graph.json`, and a refusal says nothing: the amber mark just clears
  after 5s.
- **Where ties come from.** When `graph.json` carries the core's
  `workspaces`/`ties`/`activeAt`, the bar draws those. Until then it
  derives real ties from what is published today, and nothing else:
  - a session's jack: its `sessions.json` `windowAddress` matched to the
    Hyprland toplevel's workspace (a session without a window has no jack);
  - a `spawned` tie: a `graph.json` `spawned` edge whose two sessions sit
    on different jacks;
  - a `project` tie: sessions anchored to the same project (`anchors`
    edges) on 2+ jacks — one bus per project;
  - a project label: the project anchoring a jack's sessions (the most
    sessions wins; ties show none);
  - activity: a session's `hooks.json` `updatedAt` advancing lights its
    jack, and the spawned tie to its parent.
  The derivation lives in the bar until core publishes ties; it is paint
  over published facts, never a new fact (coverage.md records the bend).
  No edge is ever invented: no window, no jack; no edge, no tie.

### 3.3 Dock → the board (`dock`, `aoide-dock`)
A right-edge pane, full height under the bar, tabbed. The dock is cadenza's
own board; it mounts none of sonata's gadget slots.

```
┌─ BOARD ───────────────────────────────────── 12 live [x] ┐
│ OVERVIEW │ aoide │ melete │ mneme │ SYS │ NOTIF           │
├──────────────────────────────────────────────────────────┤
```

- **OVERVIEW** (first tab) — its panes stacked, the column scrolling when
  long: **AGENTS** (the agent cards, grouped by project), **TERMINALS**
  (the terminal cards), **PROJECTS** (registered projects, the jacks each
  is bound to, live counts), and, once the mail read is published, **MAIL**
  (active mail threads: mailbox, subject, last sender, age).
- **The cards** carry sonata's conductor and terminals data, field for
  field, in termui rows; the look is cadenza's.
  ```
  aoide ────────────────────────────── 4 agt · 2 working · 361k tok
  ● phase 5 slice S8 — the board read op                 [2]   #02
    claude / claude-opus-5-5                                working
    rook-lantern · …0001 · yomi · aoide
    » land the board read op behind hasBoardFeed
    ▸ Bash: cargo test -p aoide-conduct -- graph::ties
    The ties block needs the spawned edge before the anchors edge,
    otherwise the clique collapses into one bus; reading graph.rs…
    ~/Aoide/…/crates/conduct        ctx ██████░░ 142k/200k  up 1h14m
  ├─ ● subagent: read the mail store             up  16m   #2.1
  │    general-purpose / claude-opus-5-5 · shiny-kite · …000a  working
  │    » read the mail store index and list the open threads
  │    ▸ Read: ~/.aoide/state/mail/index.json
  │    Reading the index; three threads are open, the newest from…
  │
  └─ ◐ subagent: audit the fixtures              up   9m   #2.2
       …
       ! quick-wren wants to run Bash          [approve] [deny]
  ```
  - An **agent card** is one live session, nine lines, fixed: the title
    (else the harness), the jack and the `#NN` ordinal; harness / model
    and the state word; petname · short id · host · project; the prompt
    (`»`); the current tool (`▸`, one line); the agent's words in a fixed
    three-line box whose last line elides; then the cwd (the tail kept)
    beside the context gauge and the time up. Streaming data never moves
    it. The hook phase, when there is one, is the card's state.
  - A **subagent** hangs under its nearest live main (its
    `parentSessionId`, else the `graph.json` `spawned` edge), indented on
    a tree limb (`├─` `└─`), six lines: no place line, no cwd, no gauge,
    a two-line box, its `#NN.k` and time up on the title line.
  - The **summons lane** is the one line that comes and goes: it exists
    only while `herald.json` holds a permission summons for that session,
    with `[approve] [deny]` answering through the NOTIF tab's own
    `heraldverdict` (click only).
  - Groups follow `projects.json`; sessions outside every project fall in
    a dim **unanchored** group, and with no project registered there is no
    group rule at all.
  - A **terminal card** is one window, two lines: what runs in it (the
    agent's title, else the harness; a bare shell's live command, else
    `shell`), its state, jack and time up, then the cwd. The live agent in
    a window wins over the shell hosting it.
  - A click on a card focuses its window (a subagent's, its parent's). A
    right-click, or `j`/`k`, selects a card and opens its action line:
    focus, project, undying, kill (kill asks `y/N`; subagents cannot be
    killed).
  - Only lamps move: a working or awaiting card's lamp breathes (2s), off
    one shared step at 12.5 fps that stops when nothing works or waits.
- **one tab per project** — today that project's agent cards (its mains,
  their subagents under them) and its terminal cards as two panes across
  the full width. Once the board feed is published, the
  project's feed (chatter, mail, receipts, its summonses) takes the left,
  the agents and terminals move to a narrow rail on the right, and, once
  posting is published, the composer sits at the bottom.
  ```
  │ 14:02 rook      ● turn settled               │ ● rook     │
  │ 14:02 minerva   ◐ Bash: cargo test           │ ◐ minerva  │
  │ 14:03 ↳ eidolon settled end_turn · 12 calls  │ ─ tty ──── │
  │ 14:05 khoa      re: phase 5 slice S8         │ ○ zsh [2]  │
  ├──────────────────────────────────────────────┴────────────┤
  │ to: aoide ▾ │ > _                                          │
  ```
- **SYS** — machine and spend on one tab: the machine's CPU and memory
  gauges (the kernel's `/proc/stat` and `/proc/meminfo`, read as sonata's
  meters reads them), the account block (`state/usage.json`), and, once
  `state/usage/now.json` is published, per-jack sparklines and
  per-workspace/project token and cost totals. The account block carries
  the Claude limit gauges and, when the user has opted in, usage.json's
  `ollama` block as one more gauge row, `OLLAMA … month`: the share of the
  month's included credits, the track stopping at 100% while the figure
  keeps counting in orange past it (no reset date, no dollars — Ollama
  publishes neither). `ok: false` draws one dim `ollama: <error>` line; no
  `ollama` key draws nothing.
- **NOTIF** — the herald: toasts and summonses as borderless blocks, quoted
  plain text, `[y] approve [n] deny` on a summons (the existing
  `heraldverdict` / `heralddismiss` commands, nothing else).
- **Opening** is sonata's dock's: `toggle()` from `SUPER+G`, and the bar's
  cells open it on a named tab.
- **Closing**, any of: the `[x]` cut into the BOARD rule beside the stat
  (dim at rest, `title` on hover); Escape; `SUPER+G`; the bar cell of the
  tab already showing (a cell toggles: on another tab it switches); a click
  anywhere off the board. The last is a transparent full-screen catcher
  (`aoide-dock-scrim`), one layer under the board and clear of the bar's
  reserved zone, mapped only while the board is up. It draws nothing and
  consumes the click that closes: that click does not reach the window
  under it. The bar is outside it and stays clickable.

**Untrusted text (house rule 4).** Every board `text`, `summary`, `body`,
`subject` is rendered `Text.PlainText`, never linkified, never actionable.
The composer is never pre-filled from an item.

**Until the seams land.** A part whose source is not published is not
drawn at all: no "bridge not wired" pane on the live board. Each hidden
part keeps its code behind one switch in `BoardBody.qml`, false until the
source exists; turning the part back on is that one line, and the fixture
harness (`BoardPreview.qml`) flips them to show the full board.
| part | real today | hidden until (switch) |
|---|---|---|
| OVERVIEW agent + terminal cards | `sessions.json`, `hooks.json`, `herald.json` summonses, `graph.json` `spawned` edges | — |
| OVERVIEW projects | `projects.json` (bindings column `—`) | S1 fills the bindings |
| OVERVIEW mail | — | S9/S10 (`hasMailRead`) |
| project tabs: agent + terminal cards | `projects.json`, `sessions.json`, `hooks.json`, `herald.json`, `graph.json` | — |
| project feed | — | S8/S10 (`hasBoardFeed`) |
| composer | — | S11 agents, S12 project (`hasBoardPost`); drawn disabled until `boardpost` exists |
| SYS machine CPU/mem | `/proc/stat`, `/proc/meminfo` | — |
| SYS account usage | `state/usage.json` (its `ollama` block only when opted in) | — |
| SYS per-jack CPU/mem/tokens/cost | — | S5/S6 (`hasJackUsage`) |
| NOTIF | `stage/herald.json` | — |

### 3.4 The jack insight pane
Dropped from a hovered jack: `JACK 2 · aoide`, then tokens and cost for the
sessions on that workspace (sparkline + total, `costPartial` in orange),
CPU and memory (gauges from `now.cpuPct` / `now.rssBytes`), and the session
list. Reads only `now.json`'s `by: "workspace"` row. Not a process viewer:
a `btop` row at the bottom launches the real one. Until S5/S6: the session
list is real (Hyprland + `sessions.json`), the numbers read `no usage data
— bridge not wired`.

### 3.5 Launcher (`launcher`, `aoide-launcher`)
A centred command pane: `> ` prompt, fuzzy list with `[n]` indices, modes as
tabs in the rule (`apps │ clip │ ledger`) — clipboard and the grimoire ledger
stay the lane data seams they are.
- **Search first, then the number.** A query is searched in every mode,
  digits included: apps and ledger match an app's name AND its desktop id
  (so `2048` finds 2048), clip matches the preview text. Only when a query
  that is wholly a positive integer n finds NOTHING in the current mode, and
  n is no more than that mode's list at rest, does it name a row: the list
  at rest shows, row n takes the selected styling (the one amber `[n]`),
  and Enter fires row n. Any real match always wins; a number past the list
  is a plain "no match".

### 3.6 Power menu (`powermenu`, `aoide-powermenu`)
A centred pane like a shell prompt: `┌─ SHUTDOWN ─┐` with `[l] lock
[s] suspend [r] reboot [p] poweroff [o] logout`, keyboard-first (the letter
fires); the chosen row goes amber; a destructive row asks `y/N` inline.

### 3.7 Herald toast (`herald`)
A borderless block top-right under the bar: `herald ▸ <app>` dim, then the
quoted body; a summons carries `[y] approve [n] deny`.

### 3.8 Calendar (`calendar`)
A `cal`-style month grid in a pane from the clock cell, today in accent.

### 3.9 Cover — the circuit board (`cover/`)
The wallpaper is a still image of a circuit board under the tube.
- Tracks run orthogonally with 45° bends, in `base02` (the darkest
  phosphor that still reads), 2–3px wide. Pads and vias are hollow rings
  in `base03`. A few long buses run parallel, the way a real board routes
  them. No labels, no text, no component silkscreen.
- Density falls off toward the screen centre, so windows sit on quiet
  black and the tracks live at the edges and corners.
- It is generated, not drawn: `widgets/CoverPcb.qml` (an uppercase helper,
  never a slot) renders it from the livery with a fixed seed, and the
  preview canvas shoots it at each monitor's size into
  `cover/pcb-<w>x<h>.png`. Regenerating after a palette change is one shot.
- Live: `lyra cover set <abs path>` (a hot swap). Declaring it into
  `aoide.livery.wallpaper` for the rebuild is the User's to admit.

**The live board — `widgets/wallpaper.qml`.** The cover is the song's
`wallpaper` slot body (the lane anchors it inside the per-screen Background
surface, `slots.md`), so the board is drawn, not just photographed: the
copper by `CoverPcb`, and over it the light. The still PNG stays what it is —
the fallback under the widget, and the shot the canvas keeps for a venue whose
shell has no such slot.

- **One engine, not an animation per effect.** `widgets/Trace.js` holds the
  whole mechanism: the polyline walker (`at`), the scheduler (`reconcile`),
  the step (`advance`), the budget (`want`), and the agent→node map (a hash
  of `sessionId`, so an agent keeps its pad). It is pure JS, and
  `design/trace.test.js` runs those exact bytes under `node` — it is also
  where the bar's own lamp walks from now on, so there is one
  implementation of "where is the light now", not two.
- **The light.** A pulse is a RECORD (`{ track, u, dir, leg, legs, speed,
  trail, hue }`), never a QML object: one transparent Canvas over the copper
  repaints every pulse, every node and every bloom in a single pass on one
  clock. The looks are data — a scan that arrives and dies (`legs = 1`), a
  slide that bounces back and forth (`legs > 1`, velocity eased at both ends,
  alpha lost a step per leg), a node that just breathes (`legs = 0`). Speed,
  tail length, direction and legs are drawn per pulse, so nothing moves in
  unison.
- **The machine drives it.** Every live session claims ONE ring, lit in its
  state's colour (`kit.lampColor`) and breathing — more processes, more lit
  nodes. The pulse budget is `Trace.want`: a floor of two slow dim pulses
  with nothing running (a tube never goes fully dark), then two per working
  agent and one per awaiting, capped at fourteen. An agent whose `hooks.json`
  `updatedAt` advances earns a burst on its own node, brighter and faster.
- **Colour.** working = `title`, awaiting = `urgent`, idle = `dim`. Amber
  (`hot`) is deliberately unused here: §2 gives it to the ONE live element,
  and the board carries many lights by design.
- The cover's light obeys §2's glow budget and its rule that text never
  animates: nothing here is text, nothing here is a window.

## 4. Preview fixtures

Unbuilt feeds are drawn with fixtures in the preview canvas ONLY
(`design/fixtures/<set>/`, passed as `lyra preview --fixture <dir>`), shaped
exactly after the core-seams proposal: `graph.json` with `workspaces` +
`ties` + `activeAt`, `state/usage/now.json`, and a board answer. The live
widget keeps its honest empty state until each slice lands; no adapter
reads a fixture path.

## 5. Hazards and open questions

- **Polarity (needs a core change).** The stylix module sets `polarity =
  lib.mkDefault "light"` and reads no polarity from the song; a song may
  only set `aoide.livery` (house rule 5). Proposed: an
  `aoide.livery.polarity` field (nucleus option, CONTRACTS §1, livery
  schema/lint), read by the stylix dendrite that phase 5 S5 creates in
  place of the lane. cadenza then declares `"dark"`.
- **Glass (needs a core change).** The compositor module loads hyprglass,
  its config block and the `blur on` layer rules unconditionally;
  `geometry.blurEnabled = false` only stops Hyprland's blur. Proposed, the
  dxflake pattern: when phase 5 S5 splits the compositor into dendrites,
  hyprglass becomes its own file gated on the song's livery (`blurEnabled`,
  or a `glass` field). dxflake's own
  `dendrites/compositor/hyprland/hyprglass.nix` also loads it whenever the
  quickshell module is on, so it needs the same gate.
- **Bar height** 28px vs sonata's 36; the lane reads it back, so it is the
  song's call.
- **Unbuilt seams** — every "bridge not wired" above names its slice.

## Iteration Log

- 2026-09-26 — composed from sonata; design drafted (this file,
  `coverage.md`, the livery key). Nothing staged.
- 2026-09-26 — khoa: no sonata gadgets (cadenza draws its own); the dock is
  a tabbed board — OVERVIEW first (agents, projects, terminals, mail), a
  tab per project, one SYS tab (CPU + usage), a NOTIF tab; terminal
  contrast colours for highlights; dark mode; no blur, no hyprglass.
  Nouns: switchboard (jack · tie · trunk · lamp), adopting core's `tie`.
- 2026-09-26 — first bar shots: 1px dim ties vanished at 1:1. khoa chose
  brighter ties on the same 28px bar, drawn as a circuit schematic —
  hollow pads under tied jacks, a solid bus per project, dashed spawned
  side-wires, filled junction dots (§3.2).
- 2026-09-26 — approved and staged live. khoa asked for icons: Nerd Font
  glyphs from the one face, coloured by what they label, one table in the
  kit (§2 Glyphs). Bar cells trade their word labels for glyphs.
- 2026-09-26 — khoa: not everything needs an icon. Glyphs only replace a
  needed word label (status cells, board cells, RICE); the clock, `$`, the
  tray and every list row stay bare.
- 2026-09-26 — khoa: the ties were missing live (core's `ties` field is
  unbuilt), so the bar derives them from published edges + windows until
  core does (§3.2); a patch panel of agents with lamps on activity at the
  bar's right end (§3.2a); a circuit-board cover (§3.9); and an inner
  border glow on every pane after a reference shot (`refs/ref-inner-glow.png`).
- 2026-09-26 — khoa: the agent dots after the clock confused; the agent
  map moves to a live cover once core anchors a wallpaper slot (§3.9), and
  leaves the bar now. CPU and spend leave the bar for the board's SYS tab.
  Every jack becomes a pad with its number inside, tied or not (§3.2).
  Sound and bluetooth share one cell and one pane, with a bluetooth power
  toggle and per-device connect (§3.1).
- 2026-09-26 — khoa: the jack pads are round (a circle, a pill for two
  digits), like a board's pads; the only rounded shape in the song.
- 2026-09-26 — khoa: a jack's pad breathes slowly while a session on it
  works (per state, not per tool call), and a conductor send runs a lamp
  from the sender's jack to the receiver's, on a transient wire when no
  tie joins them, read from core's coming `graph.json` `sends` (§3.2).
- 2026-09-26 — khoa: the board shows agents and subagents as cards, with
  the data sonata's conductor and terminals cards show, drawn in cadenza's
  theme, on OVERVIEW and every project tab (§3.3 "The cards").
- 2026-09-26 — khoa: core's workspace binding is live, so a jack binds from
  its insight pane (a PROJECT row of chips, `[clear]`, an inline `[+ new]`
  name, one `workspaceaction` line pinned to that jack); special (negative)
  workspaces draw no pad; the switchboard fixture follows core's project
  rule (explicit > owner > workspace default > cwd) (§3.2).
- 2026-09-26 — khoa: a digits-only launcher query still searches first
  (names and desktop ids); only when it finds nothing does `n` name row n,
  lit amber, and Enter fire it (§3.5). The RICE cell turns a dim `…` on the
  click and swallows further clicks until the mode changes or 10s pass, so
  a double click can no longer send two toggles (§3.1).
- 2026-09-28 — khoa: the terminal should read hacker-y too, as part of the
  key. Landed as the livery font tier (`aoide.livery.fonts.monospace`, a v0
  additive tier, nucleus option + CONTRACTS §1 + the stylix lane's source of
  truth), set here to ShureTechMono Nerd Font Mono; panes keep
  JetBrainsMono. Baked only: stylix is the one reader, so the face lands on
  the rebuild and staging cannot re-face a terminal (§2 Faces and the grid).
- 2026-09-28 — khoa: the circuit background should MOVE — scans of light
  arriving at a node, sliding back and forth, driven by what the machine is
  doing, varied rather than everything at once, alive even with nothing
  running. Landed as the live board: the lane anchors the `wallpaper` slot
  inside its existing Background surface, `widgets/wallpaper.qml` draws the
  copper plus the light, `widgets/Trace.js` is the one engine (budget from
  `sessions.json`, bursts from `hooks.json`, one hash-picked node per
  session), the bar's lamp walks the same code, and
  `design/trace.test.js` runs it under `node` (§2 Motion, §3.9).
