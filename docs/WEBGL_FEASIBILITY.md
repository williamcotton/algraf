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

1. **A primitive draw list rich enough to include per-datum marks.** The v0.24
   draw list covers only the chart frame (background, plot panels, facet strips,
   titles). WebGL is only worth it when there are many marks, so the first
   prerequisite is promoting geometry emission (`crate::geom`) and guide emission
   (`crate::guide`) to write structured primitives — rectangles, circles, line
   strips, polygons, and text runs — into the draw list instead of raw SVG
   strings. This is the same prerequisite the Canvas/raster backends need for
   full parity, and it is the largest single piece of work.

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

WebGL is feasible at the existing seam but is gated on the per-mark draw list
(item 1 above), which is also the prerequisite for full Canvas and raster parity.
Sequence the per-mark draw list first; treat WebGL as an optional, feature-gated
client that consumes a batched form of that list. Do not add a GL dependency to
the core crates.
