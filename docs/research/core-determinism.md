# Core determinism: floats, PRNG, raster, golden tests, fixtures

Scope: what `penwise-core` must do so stroke geometry is bit-identical on x86_64 (server, dev), aarch64 (tablet,
via uniffi) and `wasm32-unknown-unknown` (browser), and how we prove it. Facts checked 2026-10-24 against Rust
1.93.0 (pinned in `rust-toolchain.toml`), crates.io, and upstream sources.

## 1. Bit-identical floats: x86_64 ↔ aarch64 ↔ wasm32

**What Rust guarantees** ([RFC 3514 float semantics](https://github.com/rust-lang/rfcs/blob/master/text/3514-float-semantics.md),
[`f32` docs, 1.93](https://doc.rust-lang.org/1.93.0/std/primitive.f32.html)):

- `+ - * / %`, `sqrt`, `mul_add`, `div_euclid`/`rem_euclid`, int↔float `as` casts (saturating), `abs`, `copysign`,
  `floor`/`ceil`/`trunc`/`round`/`round_ties_even`/`fract`, comparisons, `total_cmp`, `to_bits`/`from_bits`: IEEE 754
  correctly rounded, so same bits on every target except i586/x87 targets (not ours). The docs for `sqrt` and
  `mul_add` say "guaranteed to be the rounded infinite-precision result… guaranteed not to change".
- **No FMA contraction.** rustc never fuses `a*b + c` into an FMA. RFC 3514 lists contraction as a transformation strict
  IEEE semantics rules out, and Rust has no fast-math flag. `-C target-cpu=native` does not change float results; it
  only enables instructions LLVM can use without changing semantics. Opt-level changes don't change results either.
  `mul_add` is exact everywhere, but wasm has no scalar FMA, so it lowers to a software `fmaf` (slow). Don't use it.
- **Unspecified precision (banned in core):** `sin cos tan asin acos atan atan2 sin_cos exp exp2 exp_m1 ln log log2
  log10 ln_1p powf powi cbrt hypot sinh cosh tanh asinh acosh atanh gamma erf` on both `f32` and `f64`. The docs say
  "The precision of this function is non-deterministic. This means it varies by platform, Rust version, and can even
  differ within the same execution". On Linux, std calls glibc (e.g. `hypot` "corresponds to the hypotf from libc"),
  Android uses bionic, and wasm uses the Rust port in compiler-builtins. LLVM can also constant-fold these calls with
  the *host* libm at compile time. `powi` is an LLVM intrinsic with no defined rounding.
- **`algebraic_*` methods** let the compiler reassociate. Banned.
- **`min`/`max`:** "If the inputs compare equal (such as +0.0 and -0.0), either input may be returned
  non-deterministically" ([core f32::min](https://doc.rust-lang.org/1.93.0/core/primitive.f32.html#method.min)).
  x86 `minss` and wasm `f32.min` disagree on ±0, and a different zero sign can flip the result of a later `1/x` or
  `atan2`. `clamp` is specified by comparisons and is fine.
- **NaN bits are not portable.** On wasm the NaN payload and sign are nondeterministic unless every input NaN is
  canonical ([RFC 3514 §wasm](https://github.com/rust-lang/rfcs/blob/master/text/3514-float-semantics.md),
  [wasm spec: NaN propagation](https://webassembly.github.io/spec/core/exec/numerics.html#nan-propagation)). x86
  produces the negative quiet "default NaN", and aarch64 produces the positive one. A NaN that reaches output bits
  therefore breaks hashes. Values must stay finite.
- **SIMD / auto-vectorization:** without fast-math, LLVM does not reorder float reductions, so auto-vectorized code
  gives the same results as scalar code. Our wasm target does **not** enable `simd128` or `relaxed-simd`. Local
  `rustc --print cfg --target wasm32-unknown-unknown` shows only `bulk-memory, multivalue, mutable-globals,
  nontrapping-fptoint, reference-types, sign-ext`. Never enable `+relaxed-simd`: its `relaxed_madd`/`relaxed_min`
  results depend on the host. Avoid crates that do runtime CPU dispatch to approximate instructions such as
  `rcpps`/`rsqrtps` (see §3).
- **Subnormals:** IEEE on all three targets. Rust never sets FTZ/DAZ. [UNVERIFIED] A C library linked into the server
  process could set MXCSR. Not a risk for core tests.
- **Non-float traps:** `usize` is 32-bit on wasm32 and 64-bit natively. Never hash or serialize `usize`, and never let
  `usize` overflow behaviour or `as` truncation matter; use `u32`/`u64` explicitly. Don't let `HashMap` order reach
  output (AGENTS.md). Float `Display`/`Debug` and `parse` are pure Rust (shortest round-trip) and identical on every
  target.

**`libm` crate:** [`libm` 0.2.16](https://crates.io/crates/libm), **MIT**, MSRV 1.63, lives in
[rust-lang/compiler-builtins](https://github.com/rust-lang/compiler-builtins) (a port of musl). It is pure Rust, so the
same source runs on every target. Its default `arch` feature swaps in hardware instructions only for correctly
rounded operations (`sqrt`, `fma`, `rint`, `floor`/`ceil`/`trunc` on x86 SSE2/aarch64/wasm, per
`src/math/arch/mod.rs`), so results don't change. API: free functions with a C-style `f` suffix for `f32`.

```rust
use libm::{atan2f, cosf, sinf, powf, hypotf, expf, logf, sincosf};
let a = atan2f(dy, dx); let (s, c) = sincosf(a); let w = powf(p, gamma);
```

**f32 vs f64:** use **f32** for geometry. All emitted vertices go to GPU/Canvas buffers as f32, and a
`Float32Array` handoff is half the bytes of f64. The ULP at 16 384 px is ≈0.002 px, far below a device pixel. Keep
coordinates **stroke-local** (relative to the stroke's first sample) so magnitudes stay small on the infinite canvas.
Use `f64` only for long accumulators (cumulative arc length, time over thousands of samples), converted with `as f32`
at the end. Both are deterministic. The choice is about precision and size, not correctness.

**Rules for `penwise-core`** (enforce with `clippy.toml` `disallowed-methods`, listing the full banned set for both
`f32` and `f64`):

1. Transcendentals only through `libm::*`. All std transcendental methods and `powi`, `algebraic_*`, `mul_add` are disallowed.
2. No `f32::min`/`max` (sign-of-zero ambiguity). Use a core helper `fn min(a,b) = if b < a { b } else { a }`, or `clamp`.
3. Every float reaching output (geometry, codec, hash) passes `canon(x)`, which asserts `is_finite()` and maps `-0.0` to
   `+0.0` (`x + 0.0`). The boundary fails loudly on NaN/inf.
4. Hash/serialize via `to_bits().to_le_bytes()`. Never use `usize`, native-endian bytes, or `HashMap` order.
5. Sorts use `total_cmp` plus an integer tie-break (index), so equal keys never depend on the algorithm.
6. Keep `-C target-feature`/`target-cpu` unset for core. No `simd128`/`relaxed-simd`. No crates with runtime CPU dispatch in core.
7. Gaussian-ish jitter uses libm (`logf`, `sincosf`) or an Irwin–Hall sum of uniforms. Never std.
8. Don't enable clippy nursery `suboptimal_flops`/`imprecise_flops`; they suggest `mul_add`/`hypot`/`ln_1p`.

**Recommendation:** f32 geometry with stroke-local coordinates. `libm = { version = "0.2.16", default-features = true }`
is core's only math dependency. Enforce the eight rules above with `disallowed-methods`, and catch violations with
golden tests run on both targets (§4).

## 2. Seeded PRNG

| Option | Deps | Notes |
|---|---|---|
| Hand-rolled **SplitMix64** | 0 | 64-bit state, one add + 3 xor-shift-multiplies. [Reference C](https://prng.di.unimi.it/splitmix64.c). Passes BigCrush. Its output n is a pure function of `(seed, n)`, so it works as a counter-based (stateless) generator |
| Hand-rolled **PCG32** (XSH-RR 64/32) | 0 | [pcg-c-basic](https://www.pcg-random.org/download.html). Sequential stream |
| [`rand_pcg` 0.10.2](https://crates.io/crates/rand_pcg) (`Pcg32` = `Lcg64Xsh32`) | `rand_core` 0.10 | MIT OR Apache-2.0, MSRV 1.85. Upstream docs say "deterministic and portable", but `rand_core` has broken its API across 0.6 → 0.9 → 0.10, and `seed_from_u64` expansion is defined by `rand_core`, not us |

Reference constants (test vectors verified by computation):

```rust
// SplitMix64 (Vigna): state += GAMMA; mix(state)
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
// seed 0 → 0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f

// PCG32: MULT = 6364136223846793005 (0x5851F42D4C957F2D), default inc 1442695040888963407 (0x14057B7EF767814F)
// step: old = state; state = old*MULT + inc; xs = (((old >> 18) ^ old) >> 27) as u32; xs.rotate_right((old >> 59) as u32)
// seed: state=0; inc=(seq<<1)|1; step; state+=initstate; step
// initstate 42, seq 54 → 0xa15c02b7, 0x7b47f409, 0xba1d3330 (matches pcg32-demo)
```

**Use the generator counter-based, keyed by dab:** `rand(seed, dab_index, channel) = mix64(seed.wrapping_add((dab_index
* CH + channel + 1).wrapping_mul(GAMMA)))`, with fixed `CH = 8` channels per dab (x-jitter, y-jitter, size, angle,
opacity, grain offset, …). With a sequential stream, any change in how many draws one dab makes would shift every
later dab. Keying by dab keeps old strokes stable when a brush adds a channel. Dabs can also be computed out of order,
for example when re-rendering only a dirty tile.

Floats: `((x >> 40) as f32) * (1.0 / 16_777_216.0)` gives an exact uniform in `[0,1)`.

**Recommendation:** hand-rolled SplitMix64 `mix64`, used counter-based, about 15 lines in `core::stamp::rng`. The
algorithm is frozen as part of the stroke format, since `seed` is stored per stroke forever. Pin it with the test
vectors above, and bump the brush-engine version if it ever changes. Don't depend on `rand_pcg`/`rand_core`.

## 3. `tiny-skia` (server raster, PNG goldens)

[`tiny-skia` 0.12.0](https://crates.io/crates/tiny-skia) (2026-02-02, **BSD-3-Clause**, now under
[linebender](https://github.com/linebender/tiny-skia)). 0.12 added sweep and 2-point conical gradients, color spaces,
wasm relaxed-SIMD, and `Pixmap::take_demultiplied`. Breaking change: `RadialGradient::new` now takes a start radius.
Features: `default = ["std", "simd", "png-format"]`. `png-format` pulls in `png` 0.18 (MIT/Apache-2.0).

API (all on `Pixmap`/`PixmapMut`):

```rust
let mut pm = Pixmap::new(w, h).unwrap();
let mut paint = Paint::default();                 // fields: shader, blend_mode, anti_alias, colorspace, force_hq_pipeline
paint.set_color_rgba8(20, 20, 30, 255); paint.anti_alias = true;
paint.blend_mode = BlendMode::Multiply;           // highlighter; also SourceOver, Screen, Darken…
let path = { let mut b = PathBuilder::new(); b.move_to(x0, y0); /* line_to … */ b.close(); b.finish().unwrap() };
pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
pm.stroke_path(&path, &paint, &Stroke { width: 2.0, line_cap: LineCap::Round, ..Default::default() }, ts, None);
// textured dab: pre-tinted dab texture drawn per dab
pm.draw_pixmap(0, 0, dab.as_ref(), &PixmapPaint { opacity, blend_mode, quality: FilterQuality::Bilinear }, xf, None);
// or as a shader: Pattern::new(tex.as_ref(), SpreadMode::Repeat, FilterQuality::Bilinear, opacity, xf)
// grain as mask: Mask::from_pixmap(grain.as_ref(), MaskType::Alpha) → fill_path(.., Some(&mask))
let png: Vec<u8> = pm.encode_png()?;              // premultiplied → demultiplies internally
```

For stamp brushes, tint the dab texture once per stroke (color × texture alpha), then call `draw_pixmap` with a
per-dab `Transform::from_row(sx, ky, kx, sy, tx, ty)`. Build the rotation with `libm::sincosf` ourselves, because
tiny-skia's `Transform::from_rotate` uses std `sin`/`cos`. Paper tooth (crayon) is a `Mask` from the grain texture in
page space.

**Is its output deterministic across machines?** Not guaranteed, but it can be made deterministic in practice:

- SIMD is selected at **compile time** by `cfg(target_feature)` (SSE2 by default on x86_64, AVX with
  `-Ctarget-cpu=haswell`, NEON on aarch64), with no runtime dispatch. A given build therefore runs the same code on
  every machine.
- `f32x4::recip_fast`/`recip_sqrt` use `_mm_rcp_ps`/`_mm_rsqrt_ps` on x86. Their precision is only bounded
  (≤1.5·2⁻¹²), so [UNVERIFIED] results can differ between Intel and AMD. Source check: they are only used in
  `pipeline/highp.rs` for **ColorDodge/ColorBurn**. Don't use those blend modes.
- With feature `std`, `tiny-skia-path` uses **std** `acos`/`cos`/`powf` in cubic root finding
  (`path_geometry.rs`, `path64/cubic64.rs`). `highp` color-space stages use `powf`. On Linux these go through glibc,
  so results can change with the glibc version. Avoid them: feed tiny-skia only **already flattened polygons**
  (lines, no cubics/quads), keep `colorspace = ColorSpace::Linear` (the default), and build transforms with libm.
- PNG **bytes** depend on the `png` crate and its deflate backend versions. Compare **decoded pixels**, never file hashes.

PNG golden check (server-only, AGENTS.md): decode both images and compare RGBA. Pass if the maximum channel delta is
≤ 2 **and** fewer than 0.1 % of pixels differ. On failure, write `actual.png` and `diff.png` to
`target/penwise-golden/`.

**Feature-gating out of wasm:** make it an optional dependency of core behind feature `raster`, enabled only by
`penwise-server`:

```toml
# crates/penwise-core/Cargo.toml
[features]
raster = ["dep:tiny-skia"]
[dependencies]
tiny-skia = { version = "0.12", optional = true, default-features = false, features = ["std", "simd", "png-format"] }
```

`#[cfg(feature = "raster")] pub mod raster;`. Build wasm and FFI with `-p penwise-core` / `-p penwise-wasm` (no
`--workspace`), so feature unification from the server can't turn it on. A plain `cargo build --workspace --target
wasm32` would unify features and must not be used. An optional dependency adds nothing when the feature is off.
`no-std-float` (libm math) is not an option, because `png-format` requires `std`.

**Recommendation:** `tiny-skia` 0.12 behind core feature `raster`, server-only. Feed it flattened polygons, use only
the blend modes SourceOver/Multiply/Screen/Darken/Lighten, and build transforms with libm. PNG goldens use the
decoded-pixel tolerance above; exact identity is enforced on geometry, not pixels.

## 4. Same golden tests on native and wasm

Current versions: [`wasm-bindgen-test` 0.3.79 / `wasm-bindgen` 0.2.129](https://crates.io/crates/wasm-bindgen-test)
(2026-09-25). `wasm-pack` 0.15.0 and Node v22.22.0 are installed locally. Node is sufficient: wasm-bindgen-test
"by default… target[s] Node.js" ([guide](https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/usage.html)),
and Node 22 supports every feature our target enables. No browser is needed.

Setup:

```toml
# crates/penwise-core/Cargo.toml — wasm-only dev-dep keeps native/Android graphs clean
[target.'cfg(target_arch = "wasm32")'.dev-dependencies]
wasm-bindgen-test = "0.3.79"
```

```rust
// crates/penwise-core/tests/golden.rs  (integration test; fns at file root, per wasm-bindgen-test rules)
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/golden/geometry.txt"));
const SENTENCE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/01-sentence.pwrec"));

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn outline_goldens() { /* decode fixture → build every preset → compare summary line; assert msg includes actual */ }
```

An alternative is `#[wasm_bindgen_test(unsupported = test)]`, documented in the guide and present in the 0.3.79
macro. It needs the dependency on every target. Prefer the `cfg_attr` pair.

Run:

- Native: `cargo test -p penwise-core`
- Wasm, option A: `wasm-pack test --node crates/penwise-core`. It fetches and caches the `wasm-bindgen-cli` matching
  `Cargo.lock`, so no version syncing is needed.
- Wasm, option B: `cargo install wasm-bindgen-cli --version 0.2.129 --locked`, add `[target.wasm32-unknown-unknown]
  runner = "wasm-bindgen-test-runner"` to `.cargo/config.toml`, then `cargo test -p penwise-core --target
  wasm32-unknown-unknown`. The CLI version must exactly equal the `wasm-bindgen` version in `Cargo.lock`.
- Add `just test-wasm` to `check`. Run it in `--release` too, because debug wasm is slow on fixtures with ~9k samples.

Gotchas:

- There is no filesystem and no process env in wasm32-unknown-unknown: `std::fs` returns `Unsupported` and
  `env::var` returns `Err`. Fixtures and goldens must be compiled in with `include_bytes!`/`include_str!`. Compile-time
  `env!("CARGO_MANIFEST_DIR")` works. Cargo tracks included files, so re-blessing triggers a rebuild.
- `println!` is a no-op on wasm32-unknown-unknown. Put the actual hash and summary in the `assert!` message; panic
  messages are captured.
- Tests must be at the root of the test file or inside a `pub mod`.
- Never run wasm tests with `--workspace`, since server crates (tokio) won't build for wasm32. Doctests run under the
  runner on Rust ≥1.89; use `--tests` to skip them.
- Existing `clippy.toml` `allow-*-in-tests` covers `unwrap`/`panic` in these tests.
- Optional third target: the tablet computes geometry too, so `cargo ndk -t arm64-v8a test --no-run`, push the test
  binary with `adb push`, and run it once per toolchain bump. aarch64 follows the same rules as x86_64.

**Recommendation:** one integration test file with the `cfg_attr` pair and goldens compiled in via `include_*!`. Run
it natively with `cargo test` and on wasm with `wasm-pack test --node` (option A; Node 22 is enough). Add it to `just
check`.

## 5. Golden data format and blessing

Full vectors for 13 fixtures × 13 presets would be megabytes of churn per tweak. Store **one summary line per (fixture,
preset)** in `fixtures/golden/geometry.txt`:

```
# penwise geometry goldens v1 — regenerate: PENWISE_BLESS=1 cargo test -p penwise-core --test golden
01-sentence ballpoint strokes=57 verts=18234 bbox=[12.5,40.25,800,210.75] fnv=8c3fa1d2e47b0915
```

- The hash covers the canonical bytes: for each stroke, `u32` LE vertex count, then each `canon(f32).to_bits()` LE in
  output order. Stamps hash `(x,y,size,angle,opacity)` per dab the same way.
- Hash function: hand-rolled **FNV-1a 64** (offset `0xcbf29ce484222325`, prime `0x100000001b3`; `""` →
  `0xcbf29ce484222325`, `"a"` → `0xaf63dc4c8601ec8c`). It is 8 lines, zero-dependency and identical on every target.
  Inputs aren't adversarial, so collision resistance doesn't matter.
  [`blake3` 1.8.7](https://crates.io/crates/blake3) (CC0/Apache-2.0) would add a C build step and SIMD dispatch for
  no benefit here.
- Count and bbox (Rust `{:?}` float formatting, deterministic) make a diff readable at a glance: "verts +12" says more
  than a changed hash.
- On mismatch, **native only**: write the full actual geometry to `target/penwise-golden/<fixture>-<preset>.bin` and
  `.svg` for inspection. Wasm only reports the actual summary line in the panic message, which is enough to tell
  native and wasm apart.

Blessing: `PENWISE_BLESS=1 cargo test -p penwise-core --test golden`. When the variable is set, the native test
rewrites the whole golden file, sorted and with stale entries dropped, and then passes. Without it, missing, extra or
changed lines fail with a hint to bless. Wasm can't bless: it has no env or fs, and the test only verifies. The
workflow is bless natively, then run `just test-wasm` to prove the new goldens are cross-target. Per AGENTS.md, a PR
that changes goldens includes before/after brush-sheet PNGs. Keep `just bless` as a recipe alias.

**Recommendation:** text summary lines with FNV-1a-64 over canonical LE bytes, plus count and bbox. Full dumps only
as native failure artifacts under `target/`. Bless with `PENWISE_BLESS=1`, native only.

## 6. Fixture format `.pwrec`

Source: `fixtures/raw/<session>/*.getevent.gz` (see `docs/research/pen-recordings.md`). Convert these once into
`fixtures/<NN-section>.pwrec`. The converter needs gzip and text parsing, so it lives outside core (dev bin/xtask);
core only holds the codec. There are about 51k contact frames in the session. At about 7 B/sample this comes to
roughly 350 KB total, which is fine to commit uncompressed.

Layout v1. All multi-byte fixed fields are LE. `uv` = LEB128 unsigned varint, `sv` = zigzag + LEB128.

```
magic        4  b"PWRC"
version      u16  = 1
channels     u16  bitmask: 0 x, 1 y, 2 pressure, 3 tilt_x, 4 tilt_y, 5 distance(hover), 6 t_us  (v1 writes 0x5F)
source       u8   0 = getevent raw device units, 1 = Android MotionEvent (app debug recorder, M3)
reserved     u8   = 0
device       uv len + UTF-8   e.g. "SM-X710/sec_e-pen"
per channel present (bit order): sv min, sv max         # e.g. x 0..14752, y 0..23603, p 0..4095, tilt ±63
px mapping   uv x_num, uv x_den, uv y_num, uv y_den     # px = (raw - min) * num / den; source 0: 1600/14753, 2560/23604
rate_hint    uv   nominal Hz (482)                      # informational only
strokes      uv   count
  per stroke:
    kind     u8   0 = contact (BTN_TOUCH down run), 1 = hover run (reserved: hover runs are out of v1)
    t0_us    uv   start time, µs since recording start
    n        uv   sample count (≥1)
    sample 0: sv per channel (absolute; t channel = 0)
    sample i: sv per channel delta vs sample i-1, row-major (all channels of a sample together → streamable writer)
trailer      u64  FNV-1a 64 of every preceding byte
```

- Hover is out of v1: `RunKind::Hover` (code 1) and the distance channel (bit 5) are reserved in the format so a later
  recorder can add them, but `penwise-fixtures` only emits contact runs with mask `0x5F`, so every committed fixture
  holds contact runs only.
- The ranges, mapping and channel mask in the header make the file self-describing. The 482 Hz rate, the
  portrait-native axes and the pressure ceiling are data, not code constants. The MotionEvent recorder (M3) uses
  `source = 1` with its own quantization (for example x/y in 1/16 px and pressure ×4095) declared in the same fields.
- Decoding is strict: wrong magic, an unknown version, varint overflow (>5 bytes for 32-bit), trailing garbage or a
  checksum mismatch each return a typed error, never a panic. Values are `i32` internally; deltas are wrapping `i32`
  on both sides, so every `i32` value round-trips. Only `version` decides the
  layout, so a v2 can add channels behind the mask.
- Plain deltas are good enough: t deltas of about 2074 µs fit 2 bytes, and x/y/p deltas usually fit 1–2 bytes.
  Delta-of-delta saves about 1 B/sample but costs clarity, so skip it.
- Postcard alternative: [`postcard` 1.1.3](https://crates.io/crates/postcard) (MIT/Apache-2.0, stable wire format,
  zigzag varints built in) plus `serde`. It would bring serde and proc-macros into the pure core for all three targets
  and still needs manual delta coding. Its layout is implied by struct definitions rather than written down.
- Share the varint/zigzag/FNV primitives with the stroke codec in core (PLAN: samples "quantized + delta-encoded"),
  giving one implementation with round-trip tests. Golden tests decode `.pwrec` → `Sample { x, y, pressure, tilt_x,
  tilt_y, t }` via the header mapping, which is computed in f64 and converted to f32, so it is deterministic.

**Recommendation:** use the hand-written, zero-dependency `.pwrec` v1 codec above in `penwise-core::codec`. It shares
primitives with the stroke codec, carries a header-declared device range and px mapping, uses zigzag-varint deltas,
and ends in an FNV-1a trailer. Don't use postcard/serde.
