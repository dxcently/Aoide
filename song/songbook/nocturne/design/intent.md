# nocturne — Design Intent

**Rice:** nocturne
**Palette/geometry:** deep navy/indigo, pale moonlight-silver accent — a
throwaway dark palette, not a real design pass

---

**Throwaway proof-of-concept.** `nocturne` exists solely as Phase 12's
end-to-end live proof for two things at once: (1) the `kind = "dock"`
arrangement-registry widget type, live for the first time (Phases 8-10 only
unit-tested/no-op-verified it), and (2) `StagingEngine.resolveSong`'s
baseline-fallback inheritance — nocturne authors NO `bar.qml` or
`herald-center.qml` of its own; those slots fall through to sonata's own
committed bodies wholesale (identical structure/layout, only the palette
differs). It declares one `vigil` dock widget (`widgets/vigil.qml`) as the
only file this song authors beyond its notes. Not a real dressed song — no
design intent beyond proving the pipeline. See `CONTRACTS.md` §5's "declared
widget-type registry" subsection and `modules/facets/quickshell/qml/slots.md`'s
"Declared slots" section for the mechanism this proves.

## Palette Rationale

(not yet written — this rice was scaffolded from `sonata` via `aoide rice compose`, not designed)

## Component Tier

(not yet written)

## Geometry

(not yet written)

## Iteration Log

## How to fill this rice

- Slot catalog (which slots a host wires today, what each expects): `modules/facets/quickshell/qml/slots.md`
- Per-song widget contract: `CONTRACTS.md` §5, "Per-song flavor widgets"
- Songbook playbook: `song/songbook/update-playbook.md`
- Drop a `widgets/<slot>.qml` here to dress a slot — any file under `widgets/` becomes a slot named for its basename; nothing renders until a host surface embeds a `WidgetSlot` anchor for that name.
