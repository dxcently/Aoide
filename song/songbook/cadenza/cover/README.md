# cadenza cover — the circuit board

`pcb-<w>x<h>.png` is the wallpaper of intent §3.9: a routed board (tracks in
`base02`, hollow pads and vias in `base03`) on the CRT black, quiet in the
middle. Each PNG is a shot of `widgets/CoverPcb.qml` at that exact monitor
size. The generator uses a fixed seed (`0x00CADE2A`), so the same size always
gives the same pixels. Tracks are generated per size, so do not scale one PNG
to fit another size. Shoot that size instead.

## Regenerate (after a palette change, or for a new monitor size)

```sh
export AOIDE_FLAKE_ROOT=<checkout>
R=$XDG_RUNTIME_DIR/cadenza-cover
W=$AOIDE_FLAKE_ROOT/song/songbook/cadenza/widgets/CoverPcb.qml
lyra preview "$W" --song cadenza --livery cadenza --root "$R" --no-launch
# set "autoReload": false in $R/preview.json, then:
setsid -f lyra preview "$W" --song cadenza --livery cadenza --root "$R"
lyra preview set --root "$R" --viewport 1080x1920 --width 1080 --height 1920 --anchor tl --margin 0
lyra preview shot --root "$R" --what widget --out /tmp/pcb.png
magick /tmp/pcb.png +dither -colors 16 PNG8:cover/pcb-1080x1920.png   # a few AA shades, ~55 KB
magick identify cover/pcb-1080x1920.png                                # must read 1080x1920
```

The canvas warns that CoverPcb has no `clipboard`, `dock`, `ledger`,
`powermenu`, `shared`, `stagePath` or `stagingEngine` property. The canvas
passes the same props to every widget, so these warnings are expected.

Stage live with `lyra cover set <abs path>`. Declaring the cover into
`aoide.livery.wallpaper` for the rebuild is the User's call.

## The live board — `widgets/wallpaper.qml`

The PNG is the still shot. The board itself is also drawn LIVE: the lane
anchors a song-owned `wallpaper` slot inside the per-screen Background
surface (`pkgs/lyra-shell/qml/slots.md`), and `widgets/wallpaper.qml` fills
it — `CoverPcb` for the copper, one transparent Canvas over it for the light
(`widgets/Trace.js` is the engine; intent §3.9 is the grammar). This widget is
what a live shell draws; the PNG above is what draws where no such slot
exists, and what `lyra cover set` stages.

The slot has NO baseline floor (`slots.md`): the board draws only for the song
that authors it, and only while that song's own cover is what is staged. A
`lyra cover set` pick is what shows, full stop — the shell destroys the board
rather than hiding it, so no repaint runs under the chosen image (a cover also
carries the song it was staged for, and one naming another song is ignored,
CONTRACTS.md §4).

Same board, same seed, same size — the two cannot disagree: the widget
generates from `CoverPcb`'s own generator at the output's exact size, which is
what each PNG was shot from.

Look at it on the canvas (a fixture set drives the agents, so no live machine
is needed):

```sh
export AOIDE_FLAKE_ROOT=<checkout>
R=$XDG_RUNTIME_DIR/cadenza-wall
lyra preview "$AOIDE_FLAKE_ROOT/song/songbook/cadenza/widgets/WallpaperPreview.qml" \
    --song cadenza --fixture "$AOIDE_FLAKE_ROOT/song/songbook/cadenza/design/fixtures/board" --root "$R"
lyra preview set --root "$R" --viewport 1920x1080 --width 1920 --height 1080 --anchor tl --margin 0
lyra preview shot --root "$R" --what widget --out /tmp/board.png
```

Knobs (rate/interval/pulseCap/idleFloor/seed) come from
`$R/wallpaper-preview.json` — WallpaperPreview.qml's header lists them. The
engine's maths is checked without a compositor at all:
`node song/songbook/cadenza/design/trace.test.js` — a MANUAL check (nothing in
the flake runs it; the preview above is the manual LOOK).
