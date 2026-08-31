# Glass Stretch on a Rotated Monitor — the hyprglass layer temp-FBO allocation bug

Record of the defect found 2026-08-31 on yomi-strix: the symptom, the four
accounts that turned out wrong, the mechanism, and the patch. Fixed by
`52cf066` (`pkgs/hyprglass/layer-temp-fbo-native-alloc.patch`), live-proven on
generation 181 the same day.

The rule the whole record reduces to: **allocation frame ≠ write frame.** A
framebuffer allocated in one coordinate frame and written in another loses
every pixel past the shorter edge, and no choice of source rectangle recovers
them.

## Symptom

`hosts/yomi-strix/default.nix` declares `HDMI-A-1,1920x1080@60,0x0,1,transform,3`
— a panel hung in portrait, effective 1080x1920. Three layer-shell surfaces
render visibly stretched and smeared: `aoide-dock`, `aoide-launcher`,
`aoide-powermenu`. Every other aoide surface, and every window, draws
correctly.

Those three namespaces are exactly the `layers { namespaces = … }` list in the
`plugin:hyprglass` block of `modules/facets/compositor/default.nix`. The
correlation is the first half of the diagnosis: the set of broken surfaces and
the set of glass-managed layer surfaces are the same set.

The defect has a second face, reached by patching the source rectangles while
leaving the buffer undersized (`b45d179`, since reverted). The smear resolves
into a hard vertical cliff: the `aoide-launcher` layer reports `0 36 1080 1884`
but paints only `x ∈ [0, 598)`, with the surface content upright and crammed
into the top-left, and flat clamp-grey `srgb(104,101,100)` filling the rest.
Both faces are the same undersized buffer.

## The mechanism

A monitor carries two coordinate frames, and on an odd transform they are each
other with the dimensions swapped (`src/output/Monitor.cpp:699-701`):

```cpp
Vector2D xfmd     = m_transform % 2 == 1 ? Vector2D{m_pixelSize.y, m_pixelSize.x} : m_pixelSize;
m_size            = (xfmd / m_scale).round();
m_transformedSize = xfmd;
```

```
m_pixelSize        1920 x 1080     native panel pixels
m_transformedSize  1080 x 1920     logical, post-rotation

transform 0 / 180  →  equal
transform 1 / 3    →  swapped
```

**Hyprland writes into any mid-pass framebuffer in native window
coordinates.** The viewport is set once per render pass at `m_pixelSize`
(`src/render/OpenGL.cpp:689` and `:735`, both
`setViewport(0, 0, pMonitor->m_pixelSize.x, pMonitor->m_pixelSize.y)`) and the
`RPT_MONITOR` projection once at `src/render/Renderer.cpp:1828`. Neither is
rebound when a plugin swaps `m_renderData.currentFB`. A framebuffer substituted
mid-pass inherits the monitor's native frame whatever size it was allocated at.

`GlassLayerSurface.cpp` owns `m_surfaceTempFramebuffer`, a scratch buffer the
window path does not have. `sampleAndRedirect` allocates it from
`monitor->m_transformedSize` (`:176-177`, allocation at `:190`) and binds it as
`currentFB` (`:194`). Hyprland's own `renderLayer` then draws the surface into
it — in native coordinates, out to x=1920 — through a buffer 1080 pixels wide.
**Every write past native x=1080 is clipped by the buffer edge.** That clipping
is the defect. The pixels are gone before any sampling happens.

The mask divisors compound it. `compositeAndRestore` builds `SMaskInfo` at
`:260-266`, dividing `transformBox` — which `transformedLayerBox` (`:15-19`)
produced in the `m_pixelSize` frame — by the same `m_transformedSize` locals
(`:247-248`). The UV then addresses the wrong region of an already-truncated
buffer, and the glass shader resamples it across the whole quad under
`GL_LINEAR`.

Upstream's choice of box at both sites is correct. Only the two sizes are
wrong.

Window decorations cannot reach this code. `GlassRenderer.hpp:24` states that
windows pass `mask=nullptr`, `GlassDecoration.cpp:230` omits the trailing mask
argument, and `GlassDecoration.cpp` contains no framebuffer allocation at all.
The bug is structurally impossible for them, which is why the symptom set is
exactly the layer namespaces.

## Accounts that were wrong

Four, in the order they were believed. Each one sent work in a wrong direction,
and the last one shipped as a patch and a commit body that had to be retracted.

**QML geometry.** The monitor rotation also exposed real geometry bugs in the
bar, dock and powermenu (fixed separately in `9a62055`), which made client-side
sizing the obvious suspect. Two executors edited
`song/songbook/sonata/widgets/dock.qml` and observed no change, because the
facet's `AoidePanel.qml` was the file painting the dock. The property tree and
the pixels disagreed — the signature of a compositor-side fault.

**Hyprland's own blur.** `hyprctl keyword layerrule "blur off, match:namespace
aoide-launcher"` leaves the stretch in place, which appeared to clear blur of
suspicion. It disables *Hyprland's* blur pass, not the *plugin's* layer pass.
The toggle that isolates hyprglass is `plugin:hyprglass:layers:enabled 0`.

**`GlassRenderer::sampleBackground` blitting without transform correction.** It
corrects correctly: `currentFB` is allocated at `m_pixelSize`, `transformBox`
is in that same frame, and the blit rect is clamped against
`sourceFramebuffer->m_size`.

**The source rectangles were in the wrong frame.** `b45d179` swapped
`transformBox` for `rawBox` at both sites and left both divisors at
`m_transformedSize`, on the reasoning that the temp FBO's allocation frame is
the frame its content should be addressed in. The allocation frame was the
thing that was wrong. The patch traded a smear for a hard clip and its commit
body is a falsified account; it is reverted by omission, not by a revert
commit.

This account also claimed the change was inert on every landscape monitor. It
is not: transform 180 is even, so the two frames have equal size, but
`CBox::transform`'s 180 arm computes `x = w - temp.x - temp.width`, a point
reflection about the canvas centre. `rawBox` and `transformBox` differ there.

## The fix

Both sites allocate and divide by `m_pixelSize`. The alloc, its clear-box
intersection, and both mask divisors are the same two locals, so all four
self-correct:

```diff
-    int monitorWidth  = static_cast<int>(monitor->m_transformedSize.x);
-    int monitorHeight = static_cast<int>(monitor->m_transformedSize.y);
+    int monitorWidth  = static_cast<int>(monitor->m_pixelSize.x);
+    int monitorHeight = static_cast<int>(monitor->m_pixelSize.y);
```

Applied at `sampleAndRedirect` (`:176-177`) and `compositeAndRestore`
(`:247-248`). The patch applies on the pinned v0.7.0 rather than a version bump
or a fork, because a Hyprland plugin is ABI-locked to the compositor it was
built against.

The change is identity wherever the transform is even: `Monitor.cpp:699` sets
`m_transformedSize = m_pixelSize` for transform 0 and 180 and their flipped
variants, so the two expressions are the same number. Unlike the box swap it
replaces, it alters nothing at 180.

**Live proof, generation 181, 2026-08-31.** With
`lf1i2wj8kq6bvr97lx6hz957cq6jfq43-hyprglass-0.7.0` loaded, the launcher paints
the full layer width — `x=598` and `x=700` and `x=1070` all read the scrim
(`srgb(104,101,100)`, `srgb(108,100,104)` at the last), where the clipped build
read terminal beige `srgb(220,213,203)` from x=599 outward. The grimoire is
centred, upright, and at 1:1 scale.

## Deployment

A Hyprland plugin is `dlopen`'d once at compositor startup. A rebuild writes
the new store path into `hyprland.conf` and leaves the running compositor
holding the `.so` it mapped at login, so the patched plugin takes effect only
after the compositor restarts.

**`switch-to-configuration test` cannot deliver this fix, and its failure looks
like a regression.** `test` activates without writing a boot entry, so the next
reboot returns to the previous generation with the previous plugin. This lane
lost two proof attempts to it. A change that needs a compositor restart needs
`boot` or `switch`.

Until the restart, `hyprctl keyword plugin:hyprglass:layers:enabled 0` disables
the faulty pass and the three surfaces are pixel-correct. That A/B is also the
isolation proof. The setting is runtime-only and reverts on `hyprctl reload`,
which a rebuild triggers, so it needs reapplying after every switch.

## Reporting upstream

The defect is upstream's, in `hyprnux/hyprglass` — not Hyprland, not the
compositor facet's configuration, not the QML. Everything below is verified
against the pinned source.

**Affected build.** hyprglass v0.7.0, commit
`c96940a86e6c5c9290dacb9fde204e4172186a96`, built against Hyprland 0.56.2
(`efb50993780079460b0cbed1363e2166a2de1d9f`) and hyprutils 0.14.0.
hyprglass's own `hyprpm.toml` pin table stops at Hyprland 0.55.4, so this
pairing comes from nixpkgs' `mkHyprlandPlugin` building against the hyprland
the same flake input ships, not from the table. The ABI handshake at plugin
load is what gates compatibility, and it passes.

**Precondition, both halves required.** A monitor on transform 1 or 3, and at
least one namespace under `plugin:hyprglass:layers:namespaces`. Windows on a
rotated monitor render correctly; layer surfaces on an unrotated monitor render
correctly. Only the intersection fails, which is the likely reason it survived
upstream testing.

**Reproduction.**

1. `monitor = <name>,1920x1080@60,0x0,1,transform,3`
2. `plugin:hyprglass { layers { enabled = 1; namespaces = <a layer-shell namespace> } }`
3. Open that surface. Its content renders stretched and smeared along one axis.
4. `hyprctl keyword plugin:hyprglass:layers:enabled 0` — the surface is correct.
5. `hyprctl keyword plugin:hyprglass:layers:enabled 1` — the smear returns.

Steps 4 and 5 are the isolation: the surface's reported geometry is unchanged
across the toggle, so the fault is in the glass pass, not the client's sizing.

**Expected.** The glass mask constrains the effect to the region where the
layer has visible content, at 1:1 scale.

**Actual.** Surface content beyond the swapped width is clipped away during the
redirected render, and the mask samples the wrong region of what survives.

**Root cause.** `CGlassLayerSurface::sampleAndRedirect` allocates
`m_surfaceTempFramebuffer` from `monitor->m_transformedSize`
(`src/GlassLayerSurface.cpp:176-177`, alloc at `:190`) and installs it as
`g_pHyprRenderer->m_renderData.currentFB` (`:194`). Hyprland's render pass
writes in native window coordinates throughout — `setViewport(0, 0,
pMonitor->m_pixelSize.x, pMonitor->m_pixelSize.y)` at `src/render/OpenGL.cpp:689`
and `:735`, `RPT_MONITOR` projection at `src/render/Renderer.cpp:1828`, neither
rebound when `currentFB` changes. On a 90°/270° monitor the buffer is
`m_pixelSize.y` wide while writes address out to `m_pixelSize.x`, so everything
past the narrower edge is clipped.

`compositeAndRestore` then divides `transformBox` — produced by
`transformedLayerBox` (`:15-19`) in the `m_pixelSize` frame — by the same
`m_transformedSize` locals (`:247-248`) when building `SMaskInfo.uvOffset` and
`.uvScale` (`:260-266`).

| symbol | line | what is in the wrong frame |
|---|---|---|
| `CGlassLayerSurface::sampleAndRedirect` | `:176-177` | temp FBO allocation size, and the `clearBox` intersection bound at `:199` |
| `CGlassLayerSurface::compositeAndRestore` | `:247-248` | mask UV divisors |

**This is a known trap in this codebase.** `GlassRenderer.cpp:143-146` already
carries the comment *"monitor sizes are wrong here on 90°/270° monitors, where
`m_transformedSize` is swapped relative to the framebuffer's native orientation
(#41)"*, and fixes that site by taking the framebuffer's own
`callerFramebuffer->m_size`. The layer temp FBO is the same confusion at a site
issue #41 did not reach.

**Scope.** Window decorations cannot hit this. `GlassRenderer.hpp:24` states
that windows pass `mask=nullptr`, `CGlassDecoration`'s call
(`src/GlassDecoration.cpp:230`) omits the trailing mask argument, and the file
allocates no framebuffer — there is no temp FBO to size wrongly.

**Fix.** Allocate and divide by `m_pixelSize` at both sites, matching the frame
the content is written in. Four uses collapse to two locals, so the alloc, the
clear-box bound and both mask divisors correct together. The local patch is
`pkgs/hyprglass/layer-temp-fbo-native-alloc.patch`, two hunks against
`src/GlassLayerSurface.cpp`. Identity on transform 0 and 180 and their flipped
variants, where `Monitor.cpp:699` makes the two sizes equal.

An alternative that also holds: read `m_surfaceTempFramebuffer->m_size` back at
the composite site, the shape issue #41 settled on. It fixes the divisors
without fixing the allocation, so it needs the allocation change anyway.

**Check before filing.** Whether a hyprglass newer than v0.7.0 already corrects
this. A newer release cannot be adopted here without moving the Hyprland pin
with it, but it decides whether the report is a fix or a duplicate.

## Related

- [[Gadget-Dock]]
- [[Widget-Maker]] — surface sizing takes its dimensions from content, never
  from the screen
- [[Self-Ricing]]
