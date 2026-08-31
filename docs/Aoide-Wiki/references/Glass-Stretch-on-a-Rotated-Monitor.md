# Glass Stretch on a Rotated Monitor — the hyprglass layer-mask frame bug

Record of the defect found 2026-08-31 on yomi-strix, its two wrong diagnoses,
the mechanism, and the patch. Fixed by `b45d179`
(`pkgs/hyprglass/layer-mask-transform-frame.patch`).

## Symptom

`hosts/yomi-strix/default.nix` declares `HDMI-A-1,1920x1080@60,0x0,1,transform,3`
— a panel hung in portrait, effective 1080x1920. Three layer-shell surfaces
render visibly stretched and smeared: `aoide-dock`, `aoide-launcher`,
`aoide-powermenu`. Every other aoide surface, and every window, draws
correctly.

Those three namespaces are exactly the `layers { namespaces = … }` list in the
`plugin:hyprglass` block of `modules/facets/compositor/default.nix`. The
correlation is the whole diagnosis: the set of broken surfaces and the set of
glass-managed layer surfaces are the same set.

## Two diagnoses that were wrong

**QML geometry.** The first hypothesis blamed widget sizing, since the
monitor rotation also exposed real geometry bugs in the bar, dock and
powermenu (fixed separately in `9a62055`). Two executors edited
`song/songbook/sonata/widgets/dock.qml` and observed no change, because the
facet's `AoidePanel.qml` was the file painting the dock. The property tree
and the pixels disagreed, which is the signature of a compositor-side fault
rather than a client-side one — a surface whose reported geometry is correct
and whose rendering is not.

**Hyprland's own blur.** `hyprctl keyword layerrule "blur off, match:namespace
aoide-launcher"` leaves the stretch in place, which appeared to clear blur of
suspicion. It does not: that disables *Hyprland's* blur pass, not the
*plugin's* layer pass. The toggle that isolates hyprglass is

```
hyprctl keyword plugin:hyprglass:layers:enabled 0
```

With the layer pass off the three surfaces are pixel-correct. That A/B is the
proof, and it also serves as the interim mitigation (see Deployment below).

**The first root-cause claim was also wrong.** `GlassRenderer::sampleBackground`
was accused of blitting without transform correction. It corrects correctly:
`currentFB` is allocated at `m_pixelSize` (`Monitor.cpp:2790`), `transformBox`
is in that same frame, and `sampleBackground` clamps its blit rect against
`sourceFramebuffer->m_size`. The real defect is one frame deeper and lives
only on the layer path.

## The mechanism

A monitor carries two coordinate frames, and on an odd transform they are each
other with the dimensions swapped (`Monitor.cpp:698`):

```
m_pixelSize        1920 x 1080     native panel pixels
m_transformedSize  1080 x 1920     logical, post-rotation

transform 0 / 180  →  the two are equal
transform 1 / 3    →  the two are swapped
```

`GlassLayerSurface.cpp` owns `m_surfaceTempFramebuffer`, a scratch buffer the
window-decoration path does not have. It is allocated at `m_transformedSize`
(`:190`, off the locals at `:176-177`) — the same frame `rawBox` / `*layerBox`
already live in. `transformedLayerBox()` produces `transformBox` in the
`m_pixelSize` frame.

Two sites addressed that temp FBO with `transformBox`:

| site | field | consequence |
|---|---|---|
| `compositeAndRestore` | `maskInfo.uvOffset` / `uvScale` | `uvScale` off by the box's aspect ratio |
| `sampleAndRedirect` | `clearBox` | intersected against an `m_transformedSize` bound — a second, independent frame mismatch |

The mask UV is the visible one. Wrong by the aspect ratio, the mask samples a
thin sliver of the temp FBO's content, and the glass shader resamples that
sliver across the whole quad under `GL_LINEAR`. That resample is the stretch.

Window decorations never reach this code — `GlassRenderer.hpp:24` records that
windows pass `mask = nullptr`, and `GlassDecoration.cpp` has no temp-FBO
redirect at all. The bug is structurally impossible for them, which is why the
symptom set is exactly the layer namespaces.

## The fix

Both sites take `rawBox` / `*layerBox`, matching the frame the temp FBO is
actually allocated in. The patch applies on the pinned v0.7.0 rather than a
version bump or a fork, because the plugin is ABI-locked to Hyprland 0.56.0 by
the pin table in `pkgs/hyprglass/default.nix`.

`CBox::transform`'s `HYPRUTILS_TRANSFORM_NORMAL` arm is a literal `x = temp.x;
y = temp.y` copy independent of its `w,h` arguments, and `Math::invertTransform`
maps `NORMAL → NORMAL`, so `rawBox == transformBox` exactly at transform 0 and
the patch is inert on an unrotated monitor.

**Transform 180 is not identity.** It is even, so no dimension swap occurs and
the two frames have equal size, but the 180 arm computes `x = w - temp.x -
temp.width` — a point reflection about the canvas centre. The patch changes
real behaviour there. By the same frame reasoning it is likely a second fix,
and it is untested; no machine in this fleet runs 180.

## Deployment

A Hyprland plugin is `dlopen`'d once at compositor startup. `nixos-rebuild
switch` writes the new store path into `hyprland.conf` and leaves the running
compositor holding the `.so` it mapped at login. The patched plugin therefore
does not take effect until the compositor restarts, and a rebuild alone leaves
the symptom exactly as it was.

Until the restart, `hyprctl keyword plugin:hyprglass:layers:enabled 0` disables
the faulty pass. The setting is runtime-only and reverts on `hyprctl reload`,
which a rebuild triggers — so the mitigation has to be reapplied after every
switch until the compositor is restarted.

## Related

- [[Gadget-Dock]]
- [[Widget-Maker]] — surface sizing takes its dimensions from content, never
  from the screen
- [[Self-Ricing]]
