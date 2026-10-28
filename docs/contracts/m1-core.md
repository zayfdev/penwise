> **Historical contract (pre-rewrite fan-out); current behavior: docs/rewrite-design.md, docs/pages.md and the code.**

# M1 contract: `penwise-core` ink engine

Status: committed contract for the M1 fan-out (PLAN "Phase loop" step 2). Implementers code against it and never
change it; contract changes go back to the orchestrator. Sources: `PLAN.md` ("Code layout", "Data model",
"Brushes & tools"), `docs/research/ink-rendering.md`, `docs/research/core-determinism.md`,
`docs/research/pen-recordings.md`.

## Rules for every slice

- Edit only the files your slice owns (table below). `lib.rs`, `Cargo.toml`, `clippy.toml` and the contract
  files are contract-owned; nobody else edits them. Need a new dependency, module, pub item or a signature change?
  Ask the orchestrator.
- **Frozen:** every `pub` item (names, fields, signatures, documented behaviour) and every `const` that defines a
  stored format (quantization grid, byte tags, PRNG, blob layouts, canonical bytes).
- **Slice-internal:** items that are private or `pub(super)` inside your own files (e.g. `outline::freehand`,
  `stamp::spacing`, `stamp::dab`, private fields of engine/state structs). You may add, reshape or delete them.
- Each file starts with `//! Owner: slice <x>`; replace each `todo!()` with the documented behaviour.
- Tests: sibling `tests.rs` (`#[cfg(test)] mod tests;` at the end of your own file) or `crates/penwise-core/tests/`.
  Test behaviour (outputs, round trips, chunk invariance), not internals.
- AGENTS.md limits: ≤ 300 lines per source file (hard 500), ≤ 80 lines per fn, ≤ 7 params; no
  `unwrap`/`expect`/`panic!` outside tests; `thiserror` errors; no I/O, clocks, logging; no allocation per sample
  on hot paths (reuse buffers held by the builder/engine).
- Do not run gates mid-flight; the orchestrator runs `cargo fmt` and `just check` once all slices land.
- `lib.rs` carries a temporary allow-list (`clippy::todo`, `unused_variables`, `dead_code`,
  `clippy::missing_panics_doc`, `clippy::unused_self`, `clippy::needless_pass_by_value`) for the stubs. The
  orchestrator removes it after the last slice lands, so finished code must be clean without it.

## Ownership map

| Slice | Owns | Scope |
|---|---|---|
| a | `sample.rs` (`DigitizerRanges::map` only), `smoothing.rs`, `dynamics.rs`, `builder.rs` | input mapping, One Euro, pressure/velocity/tilt response, `StrokeBuilder` |
| b | `outline/**` | perfect-freehand port, fountain ellipse, flat nibs, `OutlineEngine` |
| c | `stamp/**` | spacing, dabs, scatter/jitter/tilt/spray, PRNG, procedural paper grain, `StampEngine` |
| d | `codec/**` | varint/zigzag `Reader`, stroke blob v1, `.pwrec` v1 |
| e | `raster/**`, later `crates/penwise-core/tests/raster_golden.rs` | tiny-skia fill/dabs/coverage, PNG, pixel comparator |
| f | `crates/penwise-core/tests/golden.rs`, `crates/penwise-fixtures` (new bin crate, created by f), wasm test wiring | geometry goldens native + wasm, fixture converter |
| contract | `lib.rs`, `float.rs`, `geometry.rs`, `brush/**`, `bounds.rs`, `hash.rs`, the quantization grid in `sample.rs`, `Cargo.toml`, `clippy.toml`, this doc | shared types, presets, formats |

## Module tree

```
src/lib.rs            module tree + re-exports (contract)
src/float.rs          min/max/clamp01/lerp/smoothstep/canon: deterministic float helpers (contract, implemented)
src/hash.rs           FNV-1a 64 (contract, implemented)
src/bounds.rs         Bounds (contract, implemented)
src/geometry.rs       Outline, Dab, StrokeStyle, StrokeGeometry, OutlineView, GeometryView, canonical_bytes (contract, implemented)
src/brush/            Color, BlendMode, BrushFamily/OutlineKind/StampKind + byte tags, BrushParams, Preset (contract, implemented)
src/sample.rs         Sample, GridSample, quantize (contract, implemented); RawSample, DigitizerRanges::map (a)
src/smoothing.rs      OneEuroParams, OneEuro, OneEuroScalar, SmoothingParams, FilteredSample, Smoother (a)
src/smoothing/speed.rs  SPEED_WINDOW_US, windowed pen speed from quantized positions (a; ink-rendering.md §4.2)
src/dynamics.rs       PressureCurve, VelocityResponse, TiltResponse, Dynamics, InkPoint (a)
src/builder.rs        StrokeBuilder (a)
src/outline/          mod.rs OutlineEngine; freehand.rs (MIT notice); nib.rs; sweep.rs (flat-nib hull sweep); walker.rs (b)
src/stamp/            mod.rs StampEngine; spacing.rs; dab.rs; prng.rs; texture.rs (c)
src/codec/            mod.rs; error.rs CodecError; varint.rs; stroke.rs; pwrec.rs (d)
src/raster/           mod.rs; outline.rs; stamps.rs; png.rs; sheet.rs (+ sheet/pixels.rs: shared brush-sheet renderer, used by examples/brush_sheet.rs and the sheet golden) — feature `raster` only (e)
```

`outline/flat.rs` is intentionally absent: flat opacity is a style property (one nonzero polygon never
double-covers), the round marker is perfect-freehand, and the highlighter and calligraphy are flat nibs (`nib.rs`, swept by `sweep.rs`).

## Data flow

```
RawSample ──DigitizerRanges::map──► Sample (page px, unquantized)        Android: Sample built from MotionEvent axes
                                       │
StrokeBuilder::push ─► Sample::quantize ─► Smoother::filter ─► Dynamics::apply ─► InkPoint
                                                                                    │
                     hold-back: newest point = pending tail; earlier points committed
                                                                                    │
                           ┌──────────── BrushFamily::Outline ────────────┐  ┌── BrushFamily::Stamp ──┐
                           OutlineEngine::{extend, update_tail, finish}      StampEngine::{extend, update_tail, finish}
                           │                                                 │
StrokeBuilder::geometry ─► GeometryView (stable + tentative)                 │
StrokeBuilder::finish   ─► StrokeGeometry::{Outline{style, outline} | Stamps{style, grain, dabs}}
                                       │
           ┌───────────────────────────┼──────────────────────────────┐
  raster::render_strokes (server)   canonical_bytes → fnv1a64 (goldens)   Android/web renderers
codec::stroke stores {params, seed, quantized samples}; geometry is always re-derived from it.
```

## Invariants

1. **Determinism (geometry bit-identical on x86_64, aarch64, wasm32).**
   - Transcendentals only via `libm` (`sinf`, `cosf`, `sincosf`, `atan2f`, `powf`, `expf`, `logf`, … and the f64
     variants). `clippy.toml` bans the std methods, plus `f32/f64::min/max` (sign of zero) and `mul_add`.
   - Use `float::{min, max, clamp01, lerp, smoothstep}`; `clamp`, `floor`, `round`, `sqrt`, `abs` are fine.
   - Every float that reaches output goes through `float::canon` (finite, −0 → +0); output is LE
     `to_bits()` bytes. Never serialize or hash `usize`; never let `HashMap` order reach output; sort with
     `total_cmp` + integer tie-break.
   - `f64` only for long accumulators (arc length, time), converted with `as f32` where used.
   - No `simd128`/`relaxed-simd`, no runtime CPU dispatch, no `algebraic_*`.
2. **Quantize before build.** `StrokeBuilder::push` quantizes each sample (`Sample::quantize`, idempotent) before
   anything else; the stroke codec stores exactly that grid (1/16 px, 1/4096 pressure, π/2048 rad angles). Live
   drawing, stored strokes and `.pwrec` replays therefore feed identical inputs.
3. **Chunk invariance.** For any split of a sample sequence into `push` chunks, `finish()` returns geometry with
   identical `canonical_bytes()` to one `push` of everything. Engines carry all cross-push state in fields
   (`extend(a ++ b)` ≡ `extend(a); extend(b)`); `update_tail` never changes committed state.
4. **Hold-back + pen-up.** The newest filtered point is the pending tail (tentative). `finish` commits it with its
   raw quantized position (strokes end where the pen left); the live view shows the tail at that same raw position,
   so the last live frame is exactly the finished stroke.
5. **Stable vs tentative** (ink-rendering.md §4.3): stable outline prefixes / stable dabs never change once
   exposed through `GeometryView`; tentative parts are replaced on every push.
6. **Seeded randomness only through `stamp::prng`** keyed by `(seed, dab_index, channel)`; no sequential streams,
   no wall clock. Temporal dab emission uses sample timestamps only.
7. **No panics on any input.** Non-finite input is sanitized at `quantize`; decoders return `CodecError`;
   `canonical_bytes` returns `NonFiniteError` (engine bug) instead of panicking.

## Decisions fixed by the contract

- **Coordinates:** page px (y down), `f32`, positions clamped to ±2²⁰ px by `quantize` (grid exact in `f32`).
  Sizes are **diameters** in px (340 dpi ⇒ 13.39 px/mm).
- **Sample angles:** `tilt` = Android `AXIS_TILT` (0 upright … π/2 flat); `azimuth` = Android
  `AXIS_ORIENTATION` (lean direction clockwise from screen-up, `(−π, π]`, 0 when upright).
  `Sample::lean_vector() = (sin az, −cos az)` converts to a page-space lean direction for stamp tilt shading.
- **Color:** straight (non-premultiplied) `sRGB` RGBA8; effective alpha `a/255 · opacity`; renderers premultiply.
- **Blend:** `Normal` (source-over) and `Multiply`; multiply strokes render in the under-ink band first.
- **Paper grain:** procedural tileable value noise (`stamp::texture::paper_height`), no asset bytes, no I/O;
  surfaces needing a bitmap call `bake_tile`.
- **Brush tags:** `BrushFamily::code` (`0x00–0x05` outline, `0x10–0x16` stamp), `BlendMode`, `CapStyle`,
  `SprayDistribution` codes are frozen.
- **Canonical geometry bytes v1:** see `StrokeGeometry::canonical_bytes` doc; goldens hash them with
  `hash::fnv1a64`.
- **Stroke blob v1** (`codec::stroke`, magic `PWSK`) and **`.pwrec` v1** (`codec::pwrec`, magic `PWRC`) layouts
  are in the module docs; both end in an FNV-1a 64 trailer.
- **Dependencies:** `libm 0.2.16`, `thiserror 2`, optional `tiny-skia 0.12` (`raster` feature:
  `std, simd, png-format`), wasm-only dev-dep `wasm-bindgen-test 0.3.79`. Build wasm/FFI with `-p`, never
  `--workspace`.

## Per-slice `todo!()` list

### a — input, smoothing, dynamics, builder
- `sample.rs`: `DigitizerRanges::map`
- `smoothing.rs`: `OneEuro::filter`, `OneEuroScalar::filter`, `Smoother::filter`
- `dynamics.rs`: `PressureCurve::eval`, `VelocityResponse::size_factor`, `VelocityResponse::opacity_factor`,
  `TiltResponse::side`, `TiltResponse::is_none`, `Dynamics::apply`
- `builder.rs`: `StrokeBuilder::new`, `StrokeBuilder::push`, `StrokeBuilder::geometry`, `StrokeBuilder::finish`
- Tests: `map` on `SM_X710` corners and tilt cases (upright → azimuth 0); One Euro rate independence and first
  sample passthrough; windowed pen speed within ±10% of true speed at 125/250/482 Hz; `c(p)` table values from
  ink-rendering.md §5; builder chunk invariance with random chunkings for one outline and one stamp preset (needs
  b and c landed; run after merge).

### b — outline brushes
- `outline/mod.rs`: `OutlineEngine::new`, `extend`, `update_tail`, `view`, `finish`
- `outline/freehand.rs`: `FreehandOptions::from_params`, `FreehandOptions::radius`, `rot_around`,
  `round_start_cap`, `round_end_cap`, `flat_cap`, `dot`
- `outline/nib.rs`: `DirectionSmoother::update`, `fountain_scale`, `nib_width`, `effective_angle`, `nib_corners`, `nib_rect`
- `outline/sweep.rs`: `push_segment`, `convex_hull` (exact swept convex hull of consecutive flat nib rectangles, ink-rendering.md §2.1)
- Tests: dot for one point, polygon closed and finite, `extend` chunk invariance, stable prefix never changes
  across `update_tail`, highlighter width constant under pressure.

### c — stamp brushes
- `stamp/mod.rs`: `StampEngine::new`, `extend`, `update_tail`, `stable`, `tentative`, `finish`
- `stamp/spacing.rs`: `walk_segment`
- `stamp/dab.rs`: `diameter`, `dab_alpha`, `make_dab`, `spray_burst`
- `stamp/prng.rs`: `mix64`, `rand_u64`, `rand_unit`, `rand_particle_unit` (test vectors in the module doc)
- `stamp/texture.rs`: `paper_height`, `grain_coverage`, `bake_tile`
- Tests: PRNG vectors, spacing count on a straight line, airbrush temporal dabs from timestamps only, chunk
  invariance, dab cap, tile periodicity of `paper_height`.

### d — codec
- `codec/varint.rs`: `zigzag32`, `unzigzag32`, `write_uv32`, `write_uv64`, `write_sv32`, `Reader::position`,
  `remaining`, `bytes`, `u8`, `u16_le`, `u64_le`, `f32_le`, `uv32`, `uv64`, `sv32`
- `codec/stroke.rs`: `encode_stroke`, `decode_stroke`, `encode_params`, `decode_params`, `encode_samples`,
  `decode_samples`
- `codec/pwrec.rs`: `encode_pwrec`, `decode_pwrec`, `RawFrame::to_raw_sample`, `Recording::digitizer_ranges`,
  `RecordedStroke::to_samples` (calls slice a's `DigitizerRanges::map`)
- Tests: round trips for every preset's params and random grid samples; each error variant from crafted bytes
  (truncation, bad magic/version, 6-byte varint, checksum flip, trailing byte).

### e — raster (feature `raster`)
- `raster/mod.rs`: `render_strokes`
- `raster/outline.rs`: `fill_outline`
- `raster/stamps.rs`: `CoverageBuffer::reset`, `dab_falloff`, `min_fade`, `render_stamps`
- `raster/png.rs`: `PixelDiff::passes`, `encode_png`, `decode_png`, `compare_pixels`, `diff_image`
- Later: `tests/raster_golden.rs` (brush sheet PNG goldens, server-only, `PixelTolerance::GOLDEN`).

### f — goldens, fixtures, wasm wiring
- `crates/penwise-fixtures` bin crate: `fixtures/raw/*/NN-*.getevent.gz` → `fixtures/NN-*.pwrec` via
  `codec::encode_pwrec` (gzip/text parsing stays out of core).
- `crates/penwise-core/tests/golden.rs`: decode each `.pwrec` (`include_bytes!`), build every `Preset::ALL` per
  contact stroke with `StrokeBuilder::build` (seed = stroke index), one summary line per (fixture, preset):
  stroke count, vertex/dab count, bbox, `fnv1a64(canonical_bytes)`; bless with `PENWISE_BLESS=1` (native only);
  `cfg_attr` pair for `wasm_bindgen_test`; `just test-wasm` recipe (`wasm-pack test --node crates/penwise-core`).

## Contract-implemented items (usable now)

`float::*`, `hash::{fnv1a64, Fnv1a64}`, `Bounds`, `Sample::{quantize, to_grid, lean_vector}`,
`GridSample::to_sample`, `DigitizerRanges::SM_X710`, all `brush` types and `Preset::{ALL, name, params}`,
`OutlineView::write_polygon`, `Outline::bounds`, `StrokeGeometry::{style, bounds, canonical_bytes}`,
`StrokeBuilder::{build, params, seed}`.
