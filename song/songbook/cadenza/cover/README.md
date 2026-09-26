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
