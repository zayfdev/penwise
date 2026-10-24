# M2 editing engine: algorithm research

Scope: `penwise-core/src/edit`, `fill`, `shape` (PLAN.md code layout). All code is pure Rust, deterministic on
native + wasm (AGENTS.md: `libm` for transcendentals, no `HashMap` order in output), permissive deps only.
Researched 2026-10-24. Crate facts come from the crates.io API on that date.

## Dependency summary

| Crate | Version | License | Use | Verdict |
|---|---|---|---|---|
| [`rstar`](https://crates.io/crates/rstar) | 0.13.0 (MSRV 1.85) | MIT OR Apache-2.0 | R*-tree item index | **Adopt** (deps: `heapless`, `num-traits`, `smallvec`) |
| [`i_overlay`](https://docs.rs/i_overlay/latest/i_overlay/) | 9.1.0 (MSRV 1.88) | MIT OR Apache-2.0 | polygon boolean ops / cleanup | **Adopt, scoped** (fill cleanup, fill erase). Integer core; float math in `i_float` 5.0.0, which depends on `libm` |
| [`geo`](https://crates.io/crates/geo) | 0.33.1 | MIT OR Apache-2.0 | RDP/VW, booleans | Skip: large dependency tree, std float math |
| [`imageproc`](https://docs.rs/imageproc/latest/imageproc/contours/index.html) | 0.27.0 | MIT | Suzuki–Abe `find_contours` | Skip: pulls in `image`. Use it as a reference implementation only |
| [`contour`](https://crates.io/crates/contour) | 0.13.1 (2024) | MIT OR Apache-2.0 | marching-squares isobands → `geo-types` | Skip: stale, extra types; ~150 LOC to write ourselves |
| [`kurbo`](https://crates.io/crates/kurbo) | 0.13.1 | Apache-2.0 OR MIT | Affine, ellipse, rounded rect | Skip: std float math by default. Our shapes are small |
| [`tiny-skia`](https://docs.rs/tiny-skia/latest/tiny_skia/) | 0.12.0 | BSD-3-Clause | server raster (already planned) | Masks via `Mask` / `BlendMode::DestinationOut` |

Floating-point determinism: `+ - * / sqrt` are correctly rounded in IEEE 754 on x86-64, aarch64 and wasm. Rust
never fuses into FMA implicitly. So any algorithm below that uses only those ops plus `libm` produces identical
results on every target. Sort every query result by `(z, item_id)` before it reaches output.

---

## 1. Precise eraser for outline strokes

**Options**
- **(A) Split in raw-sample domain.** Drop samples covered by the eraser, interpolate cut points, and emit new stroke
  items that keep raw samples. This is what Xournal++'s standard eraser does: it keeps a union of erased intervals
  per stroke and clones the remaining sections with interpolated ends
  ([guide](https://xournalpp.github.io/guide/tools/eraser/),
  [`Stroke.cpp`](https://github.com/xournalpp/xournalpp/blob/master/src/core/model/Stroke.cpp) `cloneSection` /
  `intersectWithPaddedBox`; GPL, so read it for the algorithm only and copy no code).
- **(B) Polygon boolean.** Compute stroke outline minus eraser outline (`i_overlay` `OverlayRule::Difference`).

**Recommendation: (A).** (B) turns a stroke into a bare polygon. That discards raw samples, which breaks PLAN's "raw
samples + brush params are the source of truth, brush improvements re-render old notes". It also breaks lasso by
samples and recognition replay, and it leaves slivers where the outline self-overlaps.

**Coverage rule.** Sample *i* (centre `c_i`, half-width `h_i` from the brush dynamics) is erased when
`dist(c_i, E) < r_e + h_i`. Here `E` is the eraser centreline polyline (stamped circles of radius `r_e` joined into
a capsule chain). With this rule nothing visible survives inside the eraser path, and the round cap on a new end
sits exactly tangent to the eraser boundary.

**Cut points.** On each segment `(s_i, s_{i+1})` that goes from inside to outside, solve
`f(t) = dist(lerp(c_i, c_{i+1}, t), E) − (r_e + lerp(h_i, h_{i+1}, t)) = 0` by fixed-count bisection (12 iterations
gives 1/4096 of a segment, and a fixed count keeps it deterministic). Lerp every channel at `t`: x, y, pressure,
tilt, and time. Samples arrive at about 482 Hz (pen-recordings.md), so segments are short. Still subdivide segments
longer than `r_e/2` before testing, as Xournal's manual warns.

**Tapers and smoothing at new ends (main pitfall).** Outline geometry depends on context. Smoothing (streamline/EMA)
uses earlier samples, and taper/thinning is measured from the natural start and end. A naive piece starting at the
cut would (1) re-taper the cut end, so a visible thin spike appears, and (2) re-smooth from cold, so the remaining
ink shifts. Fix: each piece stores context plus a trim.
```rust
struct StrokeData { samples: Vec<Sample>, brush: BrushParams, seed: u64,
                    trim: Option<Trim> }             // None = whole stroke
struct Trim { start: SampleParam, end: SampleParam,  // u = index + frac, visible range
              start_cut: bool, end_cut: bool }       // cut ends: no taper, cap at real width
```
- Copy samples from `start − W` to `end + W`, where `W` covers the smoothing window. With EMA factor `a`, pick `W`
  so that `a^W` is below the quantization step. Render the full context, then emit the outline only for `[start, end]`.
- On cut ends, skip taper and use a round cap (perfect-freehand's model has separate `start/end: {taper, cap}`
  options; [README](https://github.com/steveruizok/perfect-freehand), MIT). Natural ends keep their tapers.
- Stamp-family code does not apply. Stamp strokes are masked (§2).

**Ops.** One gesture produces one undo group: `DeleteItem(original)` plus an `AddItem(piece)` for each piece. Drop
pieces whose visible arc length is under about `0.5·h`. Pieces get fresh ULIDs and carry `origin: original_id`.

**Concurrency pitfall.** Two devices split the same stroke concurrently. Both delete the original and add pieces,
so ink one side erased reappears through the other side's pieces. Convergence still holds, because state is
identical everywhere. Mitigation (optional, deterministic): when live pieces from different splits share an
`origin`, the renderer intersects their visible ranges in the origin's sample-param space. This is rare (tablet vs
agent), so log it as a known limitation for M2.

**Sketch** (`edit/erase_split.rs`): `fn split(stroke, eraser: &[Pt], r_e) -> SplitResult {keep: Vec<Trim>}`.
1. Prefilter with AABB vs eraser AABB inflated by `r_e + h_max`.
2. Classify samples with point–polyline distance against `E`'s segments (use the rstar index if `E` is long).
3. Run-length the covered/uncovered flags.
4. Bisect each boundary to get its cut param.
5. Merge with any existing `Trim`.

---

## 2. Precise eraser for stamp strokes and fills (masks)

Stamp strokes are many textured, seeded dabs. Splitting them changes dab spacing and PRNG phase, and boolean ops on
textured dabs make no sense. So they get a **mask**: an `erase` item (PLAN data model) rendered with
destination-out.

**Data model (recommended).**
```rust
struct EraseData { target: ItemId, path: Vec<Pt>, radius: f32 }  // path in target's LOCAL frame
```
- **One erase item per (gesture, target).** Masks attach to their target and never act on "everything below in z".
  Otherwise ink drawn later over the area would vanish, and masks would detach when the target is moved.
- Store the path in the target's local, pre-transform frame. A later `UpdateItem(transform)` on the target then
  carries its masks with it.
- Undo of the gesture is `DeleteItem` of its erase items.
- **GC:** if, after a gesture, every dab circle of the target lies inside the union of its masks (test the dab
  centres: `dist(dab, mask) + dab_r ≤ r_mask`), emit `DeleteItem(target)` instead. Fully erased items should not
  linger in hit testing.

**Fills: recommend polygon difference instead of masks.** A fill is already a vector polygon with holes, so
`fill − buffer(eraser path, r)` with `i_overlay` (`OverlayRule::Difference`; its float API supports stroke/outline
padding) keeps it a plain polygon. Hit testing stays exact, PNG and SVG need no mask machinery, and the result is
emitted as `UpdateItem(polygons)` or `Delete` + `Add`. This deviates from PLAN, which says masks for fills: Main
must decide. Fallback: treat fills like stamp strokes (mask).

**Rendering per surface** (only masks whose AABB intersects the target AABB, so layers stay small):

| Surface | How |
|---|---|
| Android | `canvas.saveLayer(bounds, null)`, draw target, draw mask path with a `Paint` using `PorterDuffXfermode(PorterDuff.Mode.DST_OUT)`, `restore()` ([PorterDuff.Mode](https://developer.android.com/reference/android/graphics/PorterDuff.Mode)) |
| Web canvas | offscreen layer, `ctx.globalCompositeOperation = 'destination-out'` for the mask ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/globalCompositeOperation)) |
| Server PNG (tiny-skia) | per-item `Pixmap` layer, mask stroked with `BlendMode::DestinationOut`, then `draw_pixmap`. Or build an A8 [`Mask`](https://docs.rs/tiny-skia/latest/tiny_skia/struct.Mask.html), invert it, and pass it as the `mask` argument of `fill_path` |
| SVG export (later) | `<mask>` element with a white rect and black eraser stroke ([MDN](https://developer.mozilla.org/en-US/docs/Web/SVG/Element/mask)) |

**Vector-only consumers.**
- **Hit test:** an item is hit at `p` iff its geometry contains `p` **and**, for every mask `m`,
  `dist(p, m.path) > m.radius`, with the mask path transformed by the item transform.
- **Lasso:** count only samples or dabs that are not masked (§4).
- **Bounds:** keep conservative unmasked bounds (cheap; slightly loose selection boxes are acceptable).
- **MCP text / recognition:** these use rasterized PNGs, which already apply masks.

---

## 3. Fill bucket on a vector scene

**Pipeline** (`fill/`, each step its own file):

1. **Grid.** Use a fixed page-unit cell `FILL_CELL` (e.g. 0.5 unit; it must not depend on zoom, so every device
   gets the same result). Snap the grid origin globally: `origin = floor(tap / FILL_CELL) * FILL_CELL - half_window`.
2. **Bounded windows (infinite pages).** Start with a 512² window around the tap. If the flood reaches the window
   border, retry at 1024², then 2048², then 4096² (16 M cells, 2 MB as a bitset). If it still leaks, return
   `FillError::OpenRegion`. On fixed pages, clip the window to the page rect and treat the page edge as a wall.
3. **Rasterize walls** with core's own binary scanline rasterizer (cell-centre sampling, nonzero rule, only
   `+ - * /`, so deterministic and independent of `raster/`):
   - outline polygons of visible ink strokes and shape strokes on the `ink` layer;
   - stamp strokes as a capsule chain at the dab radius where `opacity·hardness` exceeds a threshold;
   - minus erase masks.
   - Exclude highlighter, existing fills, and the agent/text layers. If the tap hits an existing fill, recolor it
     instead (`UpdateItem`).
4. **Gap closing = morphological closing.** Compute the exact Euclidean distance transform of the walls with
   Felzenszwalb–Huttenlocher (integer squared distances, linear time, deterministic;
   [paper](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf)).
   - Dilated walls = cells with `d² ≤ g²`, where `g` = gap tolerance in cells.
   - Flood fill from the seed over non-dilated cells: scanline fill, explicit `Vec` stack, 4-connected.
   - Grow back: region' = region dilated by `g`, clipped to non-wall cells. This is the "dilate walls, fill, erode"
     closing, and it gets most of the region back up to the true lines.
   - If the seed falls inside a dilated wall, move it to the nearest free cell within `g`, or fail.
   - Krita 5.3's "Close Gap" uses a distance/gap map from MyPaint and tracks distance along the flood to detect
     narrow passages ([MR !2050](https://invent.kde.org/graphics/krita/-/merge_requests/2050),
     [manual](https://docs.krita.org/en/reference_manual/tools/fill.html)). It loses less in sharp corners. Upgrade
     path if closing rounds corners visibly. Krita caps gaps at 32 px. Cap `g` similarly, e.g. 16 cells.
5. **Tuck under ink.** Dilate the final region by 1–2 cells into the walls, so no anti-aliased halo shows between
   fill and ink. Render fills in the band below ink (like highlighter).
6. **Contour tracing.** The flood output is one 4-connected component, so its boundary is one outer ring plus hole
   rings. **Marching squares** ([Wikipedia](https://en.wikipedia.org/wiki/Marching_squares)) on the binary mask
   yields closed rings directly.
   - Saddle cases: always separate diagonal cells, which is consistent with 4-connected fill.
   - Classify rings by signed area (shoelace): outer positive, holes negative. Assign each hole to the outer ring
     containing its first vertex.
   - Suzuki–Abe border following ([DOI 10.1016/0734-189X(85)90016-7](https://doi.org/10.1016/0734-189X(85)90016-7),
     the algorithm behind OpenCV `findContours` and `imageproc::contours`) is only needed when several components
     with nested holes must be traced in one pass. Not needed here, so skip it.
7. **Simplify each ring with Ramer–Douglas–Peucker** ([Wikipedia](https://en.wikipedia.org/wiki/Ramer%E2%80%93Douglas%E2%80%93Peucker_algorithm)),
   ε ≈ 0.5–0.75 cell.
   - For closed rings, anchor deterministically at the min `(y, x)` vertex and the vertex farthest from it.
   - Use iterative RDP with an explicit stack, so recursion depth cannot blow up.
   - Visvalingam–Whyatt ([Wikipedia](https://en.wikipedia.org/wiki/Visvalingam%E2%80%93Whyatt_algorithm)) gives
     smoother output but costs about 30× more in benchmarks
     ([comparison](https://martinfleischmann.net/line-simplification-algorithms/)). The staircase hides under ink
     anyway, so **RDP**.
8. **Validity.** Simplifying rings independently can make them cross. Run the result through
   `i_overlay` (self-union, `FillRule::NonZero`). Its output is canonical: outer rings CCW, holes CW.
9. **Output.** `fill { polygons: Vec<Polygon{outer, holes}>, color, opacity }` in page coords, quantized like the
   codec. The polygon is stored in the op, so replay never re-runs the fill. Determinism matters for tests and for
   identical results from tablet and agent.

**Pitfalls**
- Thin strokes narrower than one cell leak. Rasterize walls with at least 1 cell of width (`max(h, FILL_CELL)`).
- Taps exactly on a line: report "tap inside a line" and do not fill.
- Performance: a 2048² window means EDT + flood + marching squares over 4 M cells, an estimated tens of ms on the
  Tab S9 [INFERENCE]. Reuse buffers through a `FillScratch` struct the caller owns.

---

## 4. Hit testing and lasso selection

**Spatial index: `rstar` 0.13.0** ([docs](https://docs.rs/rstar), [repo](https://github.com/georust/rstar), MIT OR
Apache-2.0, no_std + alloc).
- One `RTree<Entry>` per page, with `Entry { aabb: AABB<[f32; 2]>, id: ItemId }` implementing `RTreeObject`.
- Build with `bulk_load` on page open. `insert`/`remove` on AddItem, DeleteItem and transform.
- Query with `locate_in_envelope_intersecting(&AABB::from_corners(..))`.
- Tree shape depends on insertion order, so **sort the hits by `(z desc, id)`** before use.
- Why not a uniform grid: infinite pages make the cell size and extent awkward, and it would need a `BTreeMap`
  for deterministic order. rstar is small and maintained by GeoRust.
- Long strokes: index the item AABB, then test per-segment AABBs inside the item (chunk 32 samples per AABB, cached
  with the derived geometry).

**Primitive tests** (`edit/hit.rs`):

| Item | Test |
|---|---|
| Outline/stamp stroke (tap, stroke eraser) | **Tapered capsule** on the centreline: `dist(p, seg_i) ≤ lerp(h_i, h_{i+1}, t) + tol`, with `t` the projection param. More robust than point-in-outline, because perfect-freehand-style outlines self-overlap. Apply masks (§2) |
| Stroke eraser vs stroke | segment–segment distance (min of 4 point–segment distances, or 0 if the segments intersect, from the orientation test) `≤ r_e + h` |
| Fill / filled shape | point in polygon with the **nonzero winding number** ([Wikipedia](https://en.wikipedia.org/wiki/Point_in_polygon)), half-open edge rule, holes included |
| Shape outline | distance to the flattened, transformed path `≤ stroke_h + tol` |

`tol` is a screen-space input (e.g. 8 dp) converted to page units by the caller's zoom: core takes `tol` in page
units and does no I/O. Tap select returns the topmost hit (highest z).

**Lasso rule.** Auto-close the lasso polygon (Excalidraw does the same, closing back to the start point and testing
with a nonzero winding point-in-polygon; [PR #9169](https://github.com/excalidraw/excalidraw/pull/9169)).
- Pre-filter with the rstar query on the lasso AABB.
- Resample each candidate at uniform arc length (about 2 units, plus all vertices of shapes and fills). Raw samples
  are denser where the pen moves slowly, which would bias the count.
- Select if `inside / total ≥ 0.75`, counting only unmasked samples. Make the threshold a parameter and tune it at
  the M4 feel check.
- Dots and tiny items (arc length < 4 units): select if the centroid is inside.
- Fills: select if ≥ 75% of their outer-ring resample is inside.
- Skip hidden agent layers.

---

## 5. Shapes with handles

**Transform model.** Use a full affine 2×3 for every item kind (`[a c e; b d f]`, stored as `f32`, composed in
`f64`). Shapes are defined in a canonical local frame and mapped by the transform. **Stroke after transform**:
flatten in local space, map the points through the affine, then run the outline brush in page space. Stroke width
stays uniform even under non-uniform scale or shear: group-scaling a rotated rectangle gives a parallelogram with
the same pen width, which is what the user dragged. For ink strokes, the transform maps centreline samples and
widths scale by `sqrt(|det|)`.

```rust
enum ShapeKind { Rect, RoundedRect { radius: f32 },        // radius in page units, clamp ≤ min(w,h)/2
                 Ellipse, Line, Arrow { head: ArrowHead },
                 Polygon { points: Vec<Pt> } }              // regular n-gon is generated into points at creation
struct ShapeData { kind: ShapeKind, transform: Affine2, style: BrushParams, fill: Option<Color> }
```
**Local geometry.**
- Box shapes use the unit square `[-0.5, 0.5]²`, so size lives in the transform.
- Line and arrow use local endpoints `(-0.5, 0)` → `(0.5, 0)`. The 2 endpoint handles then edit the transform
  directly.
- **Ellipse:** 4 cubic Béziers with `k = 0.5522847498`
  ([derivation](https://spencermortensen.com/articles/bezier-circle/)). Map the control points through the
  affine, which is exact for Béziers, then flatten adaptively by subdivision. No trig at all.
- **Rounded rect:** 4 lines + 4 quarter-arc Béziers. Under the affine, corners stay round only if the radius is
  applied after mapping. So build the rounded rect in page space from the transformed corners when there is no
  shear. With shear, map the local construction.
- **Arrow:** shaft plus head polyline. Head length = `clamp(4·stroke_w, 8, 0.4·len)`, half-angle 25–30°. Compute
  the head direction from the transformed shaft vector with no trig: rotate the unit vector using the precomputed
  `cos`/`sin` of the fixed angle (constants).
- **Regular polygon:** generate points with `libm::cosf/sinf` once at creation; after that they are data.
- **Closed shapes** are stroked as closed paths with no tapers. Generate synthetic samples with constant pressure
  along the flattened path and reuse the outline builder in closed mode (a builder requirement for M1/M2).

**Handles.**
- Box shapes: 8 resize handles (corners + edge midpoints), 1 rotate handle offset above the top edge, body = move.
- Line and arrow: 2 endpoint handles.
- Hit radius is a constant in screen space (e.g. 12 dp), passed in page units.
- Test in **local space** after applying the inverse transform. Handle hit areas are circles in screen space,
  though, so test against the handle's page-space position.
- Priority order: rotate > corner > edge > endpoint > body.

**Resize about an anchor.** The anchor is the opposite handle (opposite corner, or opposite edge midpoint).
1. Take the pointer in local coords: `q = T⁻¹ · p`.
2. Set the new local extent along the handle's axes: `s_x = (q.x − anchor.x) / (handle.x − anchor.x)`, `s_y` the
   same way. Edge handles constrain one axis. Negative values flip the shape.
3. **Constrain** (square/circle toggle): `s_x = s_y = sign · max(|s_x|, |s_y|)`. On edge handles it scales both.
4. Compose `T' = T · Tr(anchor) · S(s_x, s_y) · Tr(−anchor)`. The anchor stays fixed in world space by
   construction.

**Rotate about the pivot** (centre `c_w = T · (0, 0)`):
1. `θ = atan2(p − c_w) − atan2(p0 − c_w)` with `libm::atan2`.
2. Snap to 15° multiples when within 3°.
3. `T' = Tr(c_w) · R(θ) · Tr(−c_w) · T`. Build `R` from `libm::cos/sin` of `θ`.

Multi-selection uses the same maths on the group AABB, applied to each item's transform.

**Ops.** `UpdateItem(transform)` always carries the **absolute** new transform, never a delta, because the sync
layer is LWW per field. Emit it once at gesture end. Live drag is a local preview only.

---

## 6. Undo/redo over a multi-writer append-only log

**Model.** Undo never removes or rewrites log entries. Each local user action becomes an **action**: a group of ops
sharing an `action_id`. The local `UndoStack` (per device × page, in memory, depth ~200, not synced) stores what is
needed to reverse it. Undo and redo **emit new ops** with fresh `op_id` and HLC. They sync like any other edit, so
the existing convergence tests cover them.

This is the shape every multi-user editor converges on:
- Figma: undo only reverses your own actions; "if you undo a lot, copy something, and redo back to the present,
  the document should not change"; an undo rewrites redo history at the time of the undo
  ([blog](https://www.figma.com/blog/how-figmas-multiplayer-technology-works/)).
- Liveblocks ([blog](https://liveblocks.io/blog/how-to-build-undo-redo-in-a-multiplayer-environment)).
- Yjs `UndoManager` with `trackedOrigins` to undo only local-origin changes, and `captureTimeout` grouping
  ([docs](https://docs.yjs.dev/api/undo-manager)).

**Inverses** (computed from state just before the action applies):

| Forward | Inverse |
|---|---|
| `AddItem(id)` | `UpdateItem(id, deleted = true)` (equivalently `DeleteItem`) |
| `DeleteItem(id)` | `UpdateItem(id, deleted = false)`. Needs tombstones: `deleted` is an LWW field, and items are never hard-removed while referenced |
| `UpdateItem(id, f = new)` | `UpdateItem(id, f = old)` |
| precise split (§1) | undelete original + delete pieces |
| mask erase (§2) | delete the erase items |

**Only undo your own ops; never clobber others.** On undo, for each field the action wrote, check the page state's
per-field LWW stamp.
- If the current winner is still **our** op (same `op_id`), emit the inverse.
- If a collaborator or agent has written that field since, **skip that field**. Their newer edit wins, which is the
  Figma/Liveblocks behaviour.
- If every field of an action is stale, drop the entry and undo the next one, so the user never sees a no-op undo.
- At undo time, capture the *current* values as the redo entry (Figma principle), not the values recorded
  originally. Redo applies the same staleness check in reverse.

```rust
pub struct UndoStack { undo: Vec<Action>, redo: Vec<Action>, me: DeviceId }
pub struct Action { id: ActionId, writes: Vec<FieldWrite> }          // (item, field, op_id, before, after)
impl UndoStack {
    pub fn record(&mut self, a: Action);                             // clears redo
    pub fn undo(&mut self, page: &PageState) -> Vec<OpPayload>;      // skips stale fields
    pub fn redo(&mut self, page: &PageState) -> Vec<OpPayload>;
}
```
- **Grain:** one pen-down → pen-up gesture is one action: a stroke, an erase gesture (many splits and masks), or a
  transform drag. Fill tap = 1 action. Lasso + delete = 1 action. Do not use time-based merging (unlike Yjs's
  500 ms default). Gestures are the natural boundary.
- **Agents** (`author = agent:<name>`) are other writers. Tablet undo never touches their ops. Users remove agent
  ink by erase or hide.
- **Pitfalls:**
  1. Server compaction must keep tombstones, or undelete targets vanish.
  2. Undo of a transform must restore the absolute old transform (LWW).
  3. A remote delete of an item we transformed makes our undo stale: skip it.
  4. The undo stack holds `ItemId`s, never indices or z positions.

---

## Open decisions for Main
1. Precise erase on **fills**: use polygon difference (recommended) instead of PLAN's "mask".
2. Concurrent split resurrection (§1): accept it as a documented limitation in M2, or add origin-range intersection.
3. Constants to tune at the M4 feel check: `FILL_CELL`, max gap `g`, lasso threshold 0.75, handle hit radius.
