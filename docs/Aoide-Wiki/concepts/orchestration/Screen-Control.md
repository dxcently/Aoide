---
type: concept
created: 2026-08-17
tags: [aoide, agent, cli, screen, pointer, vision, computer-use]
---

# Screen Control — the `aoide screen` family

`aoide screen` is aoide's computer-use surface: an agent's whole loop for
looking at the desktop, grounding a target on it, acting on it, and
verifying the act landed — fourteen verbs
(`info`/`shot`/`ocr`/`diff`/`send`, plus nine `screen point <verb>`s) behind
one CLI group, registered like every other command
([[aoide-cli]]/[[Agent-Interface]]). Implementation:
`pkgs/aoide/crates/conduct/src/screen/`. Contract: `CONTRACTS.md` §8 (the
capture sidecar's field shape, the image↔screen scale contract, and the
full reason-code vocabulary this page's examples draw from).

## The loop

```
   shoot                 ground                  act                  verify
┌──────────┐  sidecar  ┌──────────┐  screen-px ┌──────────┐  re-shoot ┌──────────┐
│  screen  │ ────────▶ │ screen   │ ─────────▶ │  screen  │ ────────▶ │  screen  │
│  shot    │  origin/  │ ocr  or  │  (x,y) or  │  point   │  same rect│  diff    │
│ --fit    │  scale    │ read the │  a picked  │  <verb>  │           │          │
│ 1280x800 │           │  image   │  pixel     │--from-shot│          │changed?  │
└──────────┘           └──────────┘            └──────────┘           └──────────┘
```

1. **Shoot** — `screen shot --fit 1280x800` downscales to a model-friendly
   frame, never upscaling, and records the forward scale factor (e.g. `0.667`)
   in the capture's sidecar; `--from-shot` is what inverts it later, at use
   time. Add `--region "X,Y WxH"` first, on a prior capture's
   sidecar coordinates, to re-shoot a small area at native resolution when
   the downscaled frame lost detail worth reading.
2. **Ground** — either read `screen ocr <capture>`'s word list by eye/by
   name, or pick a pixel off the (possibly downscaled) image directly. Both
   land in IMAGE space.
3. **Act** — hand that image-space coordinate to any coordinate-taking
   `screen point <verb>` with `--from-shot <capture>`: it converts through
   the same sidecar before moving anything.
4. **Verify** — `screen diff <before-capture>` re-shoots the identical rect
   and reports whether anything actually changed, mechanically — never an
   LLM guess about whether a click landed.

## Verb reference

| Verb | Does |
| --- | --- |
| `screen info` | Read-only desktop readout: monitors, cursor position, active workspace, its clients, every layer surface — typed `hyprctl -j` parses, no capture involved. |
| `screen shot` | Capture via grim: full layout, one monitor (`--output`), a rectangle (`--region`), a human-picked region (`--pick`), a window (`--window`), or a conducted session's window (`--session`); writes the JSON sidecar. |
| `screen ocr <capture>` | Tesseract text extraction over an existing capture; writes per-word text + bounding boxes (already in ABSOLUTE SCREEN coordinates) into that capture's own sidecar. |
| `screen point move <x> <y>` | Real synthesized motion (not a warp) to an absolute target, verified on landing; drift refuses rather than continuing blind. |
| `screen point click [button] [x y]` | A button press; the optional `x y` is a pre-flight guard — refuses unless the pointer is EXACTLY there. `--count` repeats it 1-10 times. |
| `screen point scroll <dy> [dx]` | Wheel events on one or both axes, one notch = one real wheel detent; both axes clamp to ±100 notches per call. |
| `screen point idle [samples] [timeout]` | Blocks until N consecutive identical cursor samples are seen (nobody is touching the mouse) or timeout elapses. Read-only. |
| `screen point save` | Persists the current cursor position to state. Read-only. |
| `screen point restore` | Warps the cursor back to the last `save`d position (a warp is fine here — returning to a known spot, not synthesizing a gesture). |
| `screen point drag <x1> <y1> <x2> <y2>` | Move to the start (verified), then ONE atomic press→interpolated-motion→release sequence to the end, then verify the release landed. |
| `screen point hover <x> <y>` | Move to target, hold `--settle-ms`, report what layer surfaces/windows appeared, disappeared, or got retitled while parked there. |
| `screen point text <text>` | Click a word/phrase an earlier `screen ocr` already located, by NAME — no picked-by-eye pixel. `--dry-run` resolves the match with no motion at all. |
| `screen diff <before-capture>` | Re-shoot the identical rect after an optional settle delay, pixel-diff the two images, report a changed bounding box plus an inventory delta. |
| `screen send <capture>` | Hand a capture (path + comment + OCR text) to a conducted session or a registered A2A agent. |

## Mapping from the Anthropic computer-use vocabulary

| computer-use action | aoide verb(s) |
| --- | --- |
| `screenshot` | `screen shot --fit 1280x800` |
| `left_click(x, y)` | `screen point move x y` then `screen point click left x y` (the second call's `x y` re-verifies the pointer is still exactly there before pressing) |
| `left_click_drag` | `screen point drag x1 y1 x2 y2` |
| `double_click` / `triple_click` | `screen point click --count 2` / `--count 3` |
| `scroll(direction, amount)` | `screen point scroll ±dy ±dx` (positive `dy` = down, positive `dx` = right) |
| `mouse_move` / hover | `screen point move x y` / `screen point hover x y` |
| `zoom(region)` | `screen shot --region "X,Y WxH"` (native resolution — re-shoot the area of interest instead of digitally zooming a downscaled frame) |
| `wait` | `screen point idle` — not an exact analog: it blocks on the cursor going quiet (or a timeout), not a plain sleep; the `timeout` argument is the closest bounded-wait knob this CLI has |

**Deliberately absent, and why:**

- **`left_mouse_down` / `left_mouse_up`** — not built. A button held open
  across two separate process invocations has nothing watching it if the
  second one never runs (a crash, a killed agent) — a physically stuck
  button on a desk a human uses. `screen point drag` is the substitute for
  any drag-shaped gesture: press, move, and release all live inside ONE
  synthesize call, so the pointer backend's stuck-button cleanup covers the
  whole thing.
- **`hold_key` / modifier-clicks** (ctrl+click, shift+click, etc.) — not
  built. The native pointer backend (`screen::synth`) speaks
  `zwlr_virtual_pointer_v1` only; a modifier key needs a
  `zwp_virtual_keyboard_v1` client, which this crate does not implement.

## Coordinate spaces

Two coordinate spaces exist and `screen point`'s `--from-shot <capture>`
flag is the bridge between them:

- **Screen space** — logical (Hyprland) pixels, what `screen info` reports
  and what every `screen point` verb ultimately acts in.
- **Image space** — pixels in a capture file, which only equal screen space
  when that capture's `scale` is `1.0` (the default; `--fit`/`--scale`
  change it).

`--from-shot <capture>` on `move`/`click`/`drag`/`hover` means "the x/y I'm
giving you are IMAGE pixels off that capture — convert before acting,"
reading the named sidecar's `origin`/`scale` to do it
(`screen = origin + image_px / scale`, `CONTRACTS.md` §8).

**`screen point text` diverges on purpose.** It also takes `--from-shot
<capture>`, but there it names the OCR SOURCE — which sidecar's `ocr.words`
to search — never a coordinate space to convert. `screen ocr` already writes
word bounding boxes in ABSOLUTE SCREEN coordinates (it applies the same
image→screen transform once, at OCR time), so running them through the
transform a second time would silently double-apply `origin`/`scale` and
click the wrong spot. `screen point text` never calls that transform; it
reads `sidecar.ocr` directly.

## Verification: `screen diff`

`screen diff <before-capture>` re-shoots the exact rect/scale/format/quality
recorded in a prior capture's sidecar, decodes both images, and counts a
pixel changed when any of its R/G/B channels moves past `--threshold`
(default 8, absorbing JPEG noise and antialiasing) — alpha is ignored.
**Nothing changed is a normal, present-tense success (`changed: false`), not
an error** — a no-op is a fact an agent reads off the result, not an
exception to catch. Alongside the pixel bounding box (reported in both
screen and image coordinates), it reports a `hyprctl`-level inventory delta
— windows/layers that appeared, disappeared, or got retitled — since a
composited change (a tooltip opening) and a genuine window-list change are
both signal a pure pixel diff alone would under- or over-report. The same
result object is written into the AFTER-capture's own sidecar `diff` field.

## Safety

- **The guard on `click`.** An optional `x y` on `screen point click` is a
  pre-flight check, not a target: it refuses to press unless the pointer is
  EXACTLY there, catching a human bump in the gap between a `move` and the
  press that follows it.
- **Drift refusal.** `move`/`drag`/`hover`/`point text`'s live path all
  re-read the cursor after synthesizing motion and compare against the
  intended target — any mismatch, however small, is refused rather than
  treated as close enough, since a real synthesized move either lands
  exactly or something interfered (a human bump, an off-screen target).
- **The idle interlock.** `screen point idle` is a genuine "nobody is
  touching this mouse" gate (a human's hand tremor breaks the streak of
  identical samples) — the tool an agent reaches for before acting on a
  desk it might be sharing with a person.
- **The stuck-button invariant.** The native pointer backend tracks which
  button codes are currently held as it walks a synthesized sequence and,
  on every exit path — success or failure alike — issues a release for
  every held code and flushes with `WouldBlock` retry before it ever tears
  down the connection. A flush that ultimately fails surfaces as
  `pointer-failed`. See `CONTRACTS.md` §8.
- **Live-proof status.** Every `screen point` verb that moves the pointer or
  presses a button is unit-tested up to (never across) the pointer-synthesis
  boundary — it has not yet been run live against a real compositor. Live
  verification is user-gated and pending; nothing about the CLI surface or
  this page's mappings depends on it landing.

## Related

- [[aoide-cli]]
- [[Agent-Interface]]
- [[Hyprland]]
