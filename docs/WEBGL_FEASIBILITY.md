# WebGL Backend Feasibility Note (v0.24.0)

Status: Design note. Non-normative. No WebGL dependency is added in v0.24.0.

This note records what a WebGL output backend would require on top of the v0.24
render execution boundary (spec §24.6) and the new draw-list backend, so a future
release can scope the work without re-deriving it.

## Where WebGL would attach

WebGL is an emission concern, not a planning concern. It attaches at the same
seam as the SVG and draw-list backends: it consumes a planned `RenderScene`
(layout rectangles, trained scales, per-panel geometry, legends) and never sees
the source AST. The draw-list backend already proves a non-SVG backend can be
driven from that scene. A WebGL backend would consume an *extended* draw list
rather than invent its own scene walk.

## What WebGL needs beyond Canvas and raster

1. **A primitive draw list rich enough to include per-datum marks.** ✅ *Satisfied
   in v0.29.* Geometry emission (`crate::geom`) and guide emission
   (`crate::guide`) now write structured primitives — rectangles, circles, paths,
   polygons, lines, and text runs — through a shared mark sink (`crate::sink`)
   instead of raw SVG strings, so the draw list carries a per-datum op for every
   mark plus all guides (spec §24.6). The render-model raster backend already
   consumes this list. A WebGL backend would consume the same per-mark list.

2. **Batched primitives, not per-element calls.** SVG and Canvas emit one element
   or one call per mark. WebGL wants vertex buffers: all points in one buffer, all
   bar quads in another, drawn with one `drawArrays`/`drawElements` per geometry
   layer. The draw list would therefore need a *batched* view — primitives grouped
   by type with shared style — derived from the per-mark list.

3. **Text via a glyph atlas.** WebGL has no text primitive. Titles, tick labels,
   and legend labels would need either a pre-rasterized glyph atlas uploaded as a
   texture or an SVG/HTML text overlay composited above the canvas. The overlay
   approach reuses the existing text layout and is the cheaper first step.

4. **A device-pixel-ratio and projection transform.** Scales already map data to
   plot-space pixels during planning. WebGL needs those pixel coordinates mapped
   into clip space (`[-1, 1]`) plus a DPR scale, which is a small uniform
   transform applied per draw — no change to planning.

5. **A host runtime that is not required for CLI builds.** Like the draw-list
   backend, the WebGL backend must remain optional. The CLI and library builds
   must not depend on a GL context. The natural shape is a separate, feature-gated
   client (browser/WASM) that consumes the serialized batched draw list; the core
   crates stay GL-free.

## Which marks and scales benefit most

- **High-cardinality `Point` scatter** is the clearest win: tens of thousands of
  circles are cheap as a single instanced/point-sprite draw, where SVG produces
  one `<circle>` per row.
- **`RectTile` / `Bin2D` / `HexBin` heatmaps** with many cells batch into one quad
  buffer.
- **Dense `Line` series** (many series or many vertices) batch into line-strip
  buffers.
- **Continuous color and size scales** map naturally to per-vertex attributes, so
  gradient fills and size encodings are essentially free on the GPU.

Marks and guides that are **low-count or text-heavy** (axes, legends, annotations,
faceted small multiples with few marks each) get little from WebGL and are best
left to the SVG/overlay path.

## Recommendation

WebGL is feasible at the existing seam. Its chief prerequisite — a per-mark draw
list (item 1) — is satisfied as of v0.29, and the render-model raster backend
proves a non-SVG, non-Canvas backend can be driven entirely from that list. The
remaining WebGL-specific work is items 2–5: a batched view of the per-mark list
(primitives grouped by type/style), glyph-atlas or overlay text, a clip-space +
DPR transform, and an optional feature-gated host runtime. Treat WebGL as an
optional client that consumes a batched form of the existing draw list. Do not
add a GL dependency to the core crates.
