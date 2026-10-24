# Ink rendering: brush algorithms for `penwise-core`

Scope: the math `penwise-core` implements for the two brush families in PLAN.md ("Brushes & tools"). Units: canvas
unit = 1 screen px at 100 % zoom on the Tab S9 (340 dpi → **1 mm = 13.39 px**). Pressure `p = raw/4095 ∈ [0,1]`.
Time `t` in seconds from kernel timestamps. All transcendental math via `libm` (`sinf/cosf/atan2f/powf/expf`, libm
0.2.16, MIT); `+ − × ÷ sqrt` are IEEE-exact and need no wrapper. Geometry must be bit-identical native vs wasm;
raster output (dab masks, blending) only needs perceptual equality (PLAN "Screen checks").

## 0. Shared input pipeline (both families)

```
raw sample (x,y,p,tilt_x,tilt_y,t)
  → normalize (p/4095, tilt → altitude/azimuth)
  → One Euro filter on x,y (time-based, §4)        ← replaces perfect-freehand streamline
  → pressure curve c(p) (§5)  → optional velocity factor (§4)
  → family: Outline (§1–2)  |  Stamp (§3)
```

Tilt conversion (W3C Pointer Events 3, [tiltX/tiltY ↔ altitude/azimuth](https://www.w3.org/TR/pointerevents3/#converting-between-tiltx-tilty-and-altitudeangle-azimuthangle)).
The `sec_e-pen` reports `TILT_X/TILT_Y` in ±63, assumed degrees [UNVERIFIED: Samsung driver units; check against
Android `AXIS_TILT` on device]:

```
tx = tan(tilt_x°), ty = tan(tilt_y°)
tilt θ   = atan(sqrt(tx² + ty²))        // 0 = upright, π/2 = flat  (Android AXIS_TILT convention)
azimuth α = atan2(ty, tx)               // direction the barrel leans, in canvas axes (y down)
```

Android delivers `AXIS_TILT` (0…π/2) and `AXIS_ORIENTATION` (−π…π, 0 = pointing to top of screen)
([MotionEvent](https://developer.android.com/reference/android/view/MotionEvent)); convert to the same (θ, α) in
`sample.rs`. Recorded data: writing θ ≈ 30–42°, small writing up to ~45°, deliberate shading 50–60°
(07-tilt-shade: tilt_x to −55, tilt_y to 40 → θ ≈ 59°).

## 1. Outline brushes — perfect-freehand

Source: [steveruizok/perfect-freehand](https://github.com/steveruizok/perfect-freehand) v1.2.3 (2026-02-01),
`packages/perfect-freehand/src/{getStrokePoints,getStrokeOutlinePoints,getStrokeRadius,simulatePressure,constants,vec}.ts`.
Description below is read from that source.

### 1.1 Constants
`RATE_OF_PRESSURE_CHANGE=0.275`, `FIXED_PI=π+0.0001`, `START_CAP_SEGMENTS=13`, `END_CAP_SEGMENTS=29`,
`CORNER_CAP_SEGMENTS=13`, `END_NOISE_THRESHOLD=3` (px), `MIN_STREAMLINE_T=0.15`, `STREAMLINE_T_RANGE=0.85`,
`MIN_RADIUS=0.01`, `DEFAULT_FIRST_PRESSURE=0.25`, `DEFAULT_PRESSURE=0.5`, `UNIT_OFFSET=(1,1)`.
Vector helpers: `per(A) = (A.y, −A.x)`, `lrp(A,B,t) = A + (B−A)t`, `uni(A) = A/|A|`, `rotAround(A,C,r)` = rotate
A about C by r.
Options (defaults): `size=16, thinning=0.5, smoothing=0.5, streamline=0.5, easing=t→t, simulatePressure=true,
start={taper:0, cap:true, easing:t→t(2−t)}, end={taper:0, cap:true, easing:t→(t−1)³+1}, last=false`.

### 1.2 `getStrokePoints` (centerline)
1. `T = 0.15 + (1 − streamline)·0.85`.
2. If exactly 2 input points: replace by 5 points `lrp(p0,p1,i/4), i=0..4` (avoids "dash" with tapers).
   If 1 point: append `p0 + (1,1)`.
3. `out[0] = {point:p0, pressure: p0.p ≥0 ? p0.p : 0.25, vector:(1,1), distance:0, runningLength:0}`.
4. For `i = 1..n−1`:
   - `q = (last && i == n−1) ? pts[i] : lrp(prev.point, pts[i], T)` — streamline = per-sample EMA toward input.
   - skip if `q == prev.point` (exact equality).
   - `d = |q − prev.point|`, `L += d`.
   - **Min-length gate:** while `i < n−1` and not yet reached, skip the point if `L < size` (first point is kept;
     everything in the first `size` px is dropped — source has a `TODO: backfill`).
   - push `{point:q, pressure:pts[i].p (≥0 else 0.5), vector: uni(prev.point − q), distance:d, runningLength:L}`.
5. `out[0].vector = out[1].vector` (or (0,0)). Note `vector` points **backwards** (current → previous).

### 1.3 `getStrokeOutlinePoints` (polygon)
- `totalLength = last.runningLength`.
- Taper distance per end: `false/0 → 0`, `true → max(size, totalLength)`, number → that px value.
- `minDist² = (size·smoothing)²`.
- Radius: `r(p) = size · easing(0.5 − thinning·(0.5 − p))`. With easing = identity this is **linear in p**:
  diameter `D(p) = size·(1 − thinning + 2·thinning·p)`. Mapping from penwise `size_min/size_max` (diameters):
  `size = (Dmin+Dmax)/2`, `thinning = (Dmax−Dmin)/(Dmax+Dmin)`. `thinning=0 → r = size/2` constant.
  Negative thinning makes pressure thin the line.
- Initial `prevPressure` = fold over first ≤10 points: `acc ← (acc + p_k)/2` starting at `p_0`
  (with simulatePressure, `p_k = simulate(acc, d_k, size)`) — prevents fat starts.
- `simulatePressure(prev, d, size)`: `sp = min(1, d/size)`, `rp = min(1, 1−sp)`,
  `return min(1, prev + (rp − prev)·sp·0.275)` — faster ⇒ thinner. **Penwise: `simulatePressure=false`** (real pressure).
- Loop `i = 0..n−1` (`isLast = i == n−1`):
  1. **End noise:** if `!isLast && totalLength − L_i < 3` → skip.
  2. `p = thinning ? (simulate ? simulate(prevPressure, d_i, size) : p_i) : —`; `r = thinning ? r(p) : size/2`;
     record `firstRadius` once.
  3. **Taper:** `ts = L_i < taperStart ? easeS(L_i/taperStart) : 1`;
     `te = totalLength − L_i < taperEnd ? easeE((totalLength−L_i)/taperEnd) : 1`; `r = max(0.01, r·min(ts,te))`.
  4. `next = isLast ? v_i : v_{i+1}`; `nextDpr = isLast ? 1 : v_i·next`; `prevDpr = v_i·prevVector`.
  5. **Sharp corner** if `(prevDpr < 0 && !prevWasCorner) || nextDpr < 0`: `o = per(prevVector)·r`; for
     `t = 0, 1/13, …, ≤1`: push `rotAround(P−o, P, FIXED_PI·t)` to left and `rotAround(P+o, P, −FIXED_PI·t)` to
     right (a half-disc fan on each side); set prevLeft/Right to the last ones; `prevWasCorner = nextDpr < 0`;
     `continue` (note: prevPressure/prevVector **not** updated here).
  6. `prevWasCorner = false`.
  7. **Last point:** `o = per(v_i)·r`; push `P−o` left, `P+o` right; continue.
  8. **Regular point:** `o = per(lrp(next, v_i, nextDpr))·r` (direction blends toward the current vector when the
     turn is gentle); `L' = P−o`, `R' = P+o`; push each side only if `i ≤ 1 || |side' − prevSide|² > minDist²`
     (this is what `smoothing` does: decimates outline points). Then `prevPressure = p`, `prevVector = v_i`.
- **Caps** (`first = points[0].point`, `last = points[n−1].point`):
  - `n == 1`: if no tapers or `last` → return a dot: 13 points rotating `first + per-ish offset·r` by `2·FIXED_PI·t`.
  - start: tapered → nothing; `cap` → round: rotate `rightPts[0]` about `first` by `FIXED_PI·t`, `t = 1/13…1`;
    else flat: `c = left0 − right0`, points `first − 0.5c, first − 0.51c, first + 0.51c, first + 0.5c`.
  - end: `dir = per(−v_{n−1})`; tapered → push `last` only; `cap` → round: `s = last + dir·r`, push
    `rotAround(s, last, 3·FIXED_PI·t)` for `t = 1/29 … <1` (1.5 turns so sharp end turns are covered); else flat:
    `last ± dir·r`, `last ± dir·0.99r`.
- Output (closed, **fill nonzero**): `left ++ endCap ++ reverse(right) ++ startCap`.

### 1.4 Penwise deviations (decide in M1, document in code)
| Upstream behaviour | Problem at 482 Hz / for handwriting | Penwise change |
|---|---|---|
| streamline = per-sample EMA, `T` fixed | rate-dependent: at Δt=2.07 ms, `T=0.575` ⇒ τ = −Δt/ln(1−T) ≈ 2.4 ms (no smoothing); same setting at 60 Hz ≈ 19 ms | `streamline = 0` (T=1) and smooth with One Euro (§4); or `T = 1 − exp(−Δt/τ)` |
| min-length gate drops first `size` px | eats i-dots, hooks of small letters (ballpoint ≈4 px OK; marker 16 px bad) | gate at `min(size, 1.0 px)` |
| initial pressure = first 10 points | 10 samples = 21 ms here vs 167 ms at 60 Hz | average over first `max(10 samples, 2·size px of arc)` |
| `taper:true` uses `totalLength` | whole stroke geometry changes every sample → nothing stable while streaming | numeric tapers only |
| `FIXED_PI = π+0.0001` | browser SVG hack | keep (harmless, keeps parity with upstream fixtures) |
| JS `Math.hypot/sin/cos` | determinism | `sqrt(dx²+dy²)`, `libm::sinf/cosf` |

Rendering: fill the polygon directly (tiny-skia `fill_path`, `FillRule::Winding`). tldraw's smoother look comes from
the README's `getSvgPathFromStroke` (quadratic Béziers `Q p_i, mid(p_i,p_{i+1})`); optional, applied at path
construction, not stored. Flat opacity is free: one nonzero polygon never double-covers itself.

### 1.5 License / attribution
perfect-freehand is **MIT, © 2021 Stephen Ruiz Ltd** ([LICENSE](https://github.com/steveruizok/perfect-freehand/blob/main/LICENSE)).
A Rust port is a derivative work: keep the copyright + permission notice in the source tree (e.g. header in
`outline/freehand.rs` + `THIRD_PARTY_NOTICES` entry) and in distributed binaries' notices (APK "licenses" screen,
server `--licenses`). Existing Rust ports for reference only (do not depend; we need the deviations):
[`perfect_freehand` 0.1.1](https://crates.io/crates/perfect_freehand) (MIT), [`freedraw` 1.0.4](https://crates.io/crates/freedraw) (MIT).

## 2. Direction-dependent outlines: fountain, calligraphy, marker, highlighter

All reuse §1's centerline, taper and cap logic; only the **offset vector** at each point changes.
Direction `d_i = −v_i` (forward unit vector), smoothed over ≈ one radius of arc length (EMA on the unit vector by
arc length: `w = 1 − exp(−Δs/r)`, renormalize) to stop nib-width flicker on jitter.

### 2.1 Calligraphy (flat nib, fixed angle)
Nib = segment of width `W` at fixed angle φ (axis `u = (cos φ, sin φ)`) with thickness `t` (`u⊥ = per(u)`).
The swept width perpendicular to motion is the nib's projection on the normal `n = per(d)`:

```
width(θ) = W·|sin(θ − φ)| + t·|cos(θ − φ)|       θ = atan2(d.y, d.x)
```

Correct geometry offsets **along the nib axis**, not the normal (this is what makes real calligraphy edges):
```
s = −sign(d · u)·sign(d · u⊥)  (each sign +1 if 0)
left_i  = P_i + (W/2)·u + s·(t/2)·u⊥
right_i = P_i − (W/2)·u − s·(t/2)·u⊥
```
The sign picks the outer corner pair of the nib rectangle for the current direction, so the ribbon width is exactly
`width(θ)` above (`s = sign(d·u⊥)` alone takes the inner corners whenever `d·u > 0`, giving `W·|sin| − t·|cos|`).
When the stroke direction crosses φ the ribbon twists to width `t`; nonzero fill renders the twist correctly.
Pressure modulates `W` only: `W = W_max·(w0 + (1−w0)·c(p))`. Caps: flat (the nib end is the cap). Corner fans of
§1.3 step 5 are **disabled** for nib brushes (the ribbon already covers the turn). Exact alternative if twist
artifacts show at sharp reversals: emit, per segment, the convex hull of the nib at `P_i` and `P_{i+1}` (6 points)
as subpaths of one nonzero path. Inkscape's calligraphy tool exposes the same model (width, thinning, angle,
**fixation** 1 = fixed angle / 0 = nib ⊥ stroke, caps) — [manual](https://inkscape-manuals.readthedocs.io/en/latest/calligraphy-tool.html);
add `fixation f`: effective `φ_eff = lerp_angle(θ + π/2, φ, f)`.

### 2.2 Fountain pen (pressure + mild direction)
Model the tipping as an ellipse of semi-axes `R` (along `u`, angle φ) and `a·R` (`a` = aspect, 0.5–0.7); keep
perfect-freehand's symmetric normal offsets but scale the radius by the ellipse's support along `n`:
```
k(n) = sqrt((u·n)² + a²·(u⊥·n)²)          // ∈ [a, 1]
r_i  = r_pf(c(p_i)) · k(n_i) / ((1 + a)/2)   // normalized so average width ≈ pressure width
```
Down-strokes (perpendicular to φ) are wide, cross-strokes thinner. φ default = 45° (right-handed grip; tilt
azimuth can drive φ later: `φ = α + π/2` — feel-check in M4). Round caps, short end taper.

### 2.3 Marker
Round broad: perfect-freehand with `thinning ≈ 0.1`, round caps, no taper, opacity 0.9 applied **once to the whole
polygon** (flat, no self-darkening). Chisel variant = §2.1 with `W/t ≈ 3`, φ = 30°, fixation 1, flat caps.

### 2.4 Highlighter
Chisel nib (§2.1) `W ≈ 26 px, t ≈ 8 px`, φ = 90° (full width on horizontal strokes across text), thinning 0
(pressure ignored), flat caps, color e.g. `#FFE14D`, stroke alpha 0.35.
- **Blend: multiply**, `C = lerp(C_dst, C_dst·C_src, α)` (premultiplied in tiny-skia via
  [`BlendMode::Multiply`](https://docs.rs/tiny-skia/latest/tiny_skia/enum.BlendMode.html); Android
  `BlendMode.MULTIPLY` (API 29+); Canvas2D `globalCompositeOperation='multiply'`).
- **Under ink:** within a layer render items in two bands: highlighter band first, then everything else (stable sort
  by `(band, z)`). Multiply alone already keeps black ink black; the band also keeps colored ink saturated and makes
  overlapping highlighter strokes darken like a real felt tip (each stroke multiplies once, since each is one polygon).

## 3. Stamp (dab) engine

References: libmypaint ([ISC](https://github.com/mypaint/libmypaint/blob/master/COPYING); `mypaint-brush.c`,
`mypaint-tiled-surface.c`, [brushsettings.json](https://github.com/mypaint/libmypaint/blob/master/brushsettings.json)),
Krita manual ([brush tips/spacing](https://docs.krita.org/en/reference_manual/brushes/brush_settings/brush_tips.html),
[opacity & flow](https://docs.krita.org/en/reference_manual/brushes/brush_settings/opacity_and_flow.html),
[texture](https://docs.krita.org/en/reference_manual/brushes/brush_settings/texture.html),
[sensors](https://docs.krita.org/en/reference_manual/brushes/brush_settings/tablet_sensors.html),
[spray engine](https://docs.krita.org/en/reference_manual/brushes/brush_engines/spray_brush_engine.html)),
[Procreate Brush Studio](https://procreate.com/handbook/procreate/brushes/brush-studio-settings/). Krita is GPL-3:
use its docs for behaviour only, never copy code. libmypaint is ISC: code may be ported with notice.

### 3.1 Dab output (core, geometry-level, must be bit-identical)
`Dab { x, y, radius, aspect (≥1), angle, alpha, hardness, grain_strength }` in stroke order. Rasterization of dabs is
in the renderer (tiny-skia on server/wasm, Android Canvas/GL on tablet).

### 3.2 Placement along arc length
Path = filtered samples joined by **straight segments** (482 Hz ⇒ 1.4–4 px per segment at handwriting speed
50–150 mm/s; curves are already dense). Inputs `(p, θ, α)` lerp linearly along each segment.
Dab phase accumulator (MyPaint `count_dabs_to` form, simplified to one spatial + one temporal term):
```
dφ = Δs / max(spacing · D(s), 0.5 px)  +  Δt · dabs_per_second
```
Walk each segment: while `φ_rem ≤ remaining phase of segment`, solve the position where φ hits the next integer
(using `D` at the current dab — MyPaint does the same; error is negligible since D varies slowly), emit a dab,
continue. Carry the fractional φ across segments and across `StrokeBuilder::push` calls (needed for §4.3).
First dab is emitted at `s = 0`. `spacing` = fraction of **diameter** (Krita/Procreate convention; MyPaint uses
dabs per radius: `dabs_per_actual_radius = 1/(2·spacing)`).

**Stationary dabs (airbrush):** the temporal term emits dabs while the pen is still, but only from **recorded sample
timestamps** (no wall-clock timer: it would break determinism and replay). Clamp a single gap to Δt ≤ 50 ms so a
lost frame can't dump a blob. Only airbrush sets `dabs_per_second > 0` (default 60/s; MyPaint default 0).

### 3.3 Dynamics per dab
```
P = c(p)                                    // pressure curve, §5
D = Dmin + (Dmax − Dmin)·P^g_size           // size response
flow_dab = flow · lerp(1, P, k_flow)        // pressure → flow
alpha_dab = 1 − (1 − flow_dab)^(spacing)    // spacing-normalized: "flow" = alpha accumulated per one diameter of travel
```
The last line is MyPaint's `opaque_linearize` idea (`β_dab = β^(1/dabs_per_pixel)`, `dabs_per_pixel ≈ 1/spacing`)
made exact; it lets users change spacing without changing darkness. Uses `libm::powf`.

### 3.4 Dab mask (hardness falloff)
MyPaint two-segment profile on `rr = (dist/r)²` (from `calculate_opa`, `mypaint-tiled-surface.c`), 0 < h ≤ 1:
```
rr ≤ h : opa = 1 + rr·(1 − 1/h)
rr > h : opa = (h/(1−h))·(1 − rr)          ; opa = 0 for rr > 1
```
Continuous at `rr = h` (both = h); `h = 1` → hard disc. Small dabs: MyPaint enforces a minimum fade of ~1 px by
re-solving `(r, h)` keeping optical radius `r − (1−h)r/2`:
`h' = (r_opt − f/2)/(r_opt + f/2)`, `r' = f/(1 − h')`, `f = 1 px` — apply when `r(1−h) < 1`.
Elliptical dabs: rotate pixel offset by −angle, `rr = (x'² + (aspect·y')²)/r²` (MyPaint `calculate_rr`).
Pixel coverage = `opa · alpha_dab · grain(x,y)`.

Penwise deviations (`raster/stamps.rs`):
- **Re-solved hardness floor** `MIN_FADE_HARDNESS = 0.1`: sub-pixel dabs (`r_opt < 0.61 px`) would get `h' ≤ 0.1`
  (`h' ≤ 0` vanishes entirely), so `h' = max(h', 0.1)` keeps them visible. `min_fade` returns an explicit "faded"
  flag because `h'` can equal `h` bit for bit at the floor while `r` changed. Input hardness is also floored at
  `1e-3` (the profile is 0 everywhere at `h = 0`).
- **Elliptical dabs** apply the min fade to the **minor** axis only. The major axis is rescaled to keep its own
  optical radius at the new hardness, `r_major' = r_major·(1 + h)/(1 + h')`, and is never shorter than the faded minor
  axis (the 0.1 floor breaks optical-radius parity, so a circular sub-pixel dab would otherwise become an ellipse).

### 3.5 Opacity vs flow (stroke-level cap)
Krita "Wash" / Procreate default: flow = per-dab transparency, opacity = cap for the whole stroke. Implement with a
per-stroke coverage buffer `S` (single channel, stroke bbox):
```
per dab pixel:  S ← S + a·(1 − S)              // a = mask·alpha_dab·grain ; union build-up
composite once: dst ← blend(dst, color, opacity · S)
```
Within one stroke repeated passes build toward `opacity` but never exceed it; separate strokes stack. Krita's exact
op is `KoCompositeOpAlphaDarken` (fullFlowAlpha = `opacity > dstA ? lerp(dstA, opacity, mskA) : dstA`, then
`lerp(zeroFlowAlpha, fullFlowAlpha, flow)`); the union form above is simpler and equivalent in look. "Build-up"
mode (airbrush) = `opacity = 1` cap so flow accumulates freely. Live drawing renders `S` incrementally into a
scratch layer and composites each frame; precise eraser masks `S` (PLAN).

### 3.6 Scatter, jitter, spray
Counter-based PRNG so batch and streaming give identical numbers regardless of push chunking:
`u_k = splitmix64(seed ⊕ (dab_index·0x9E3779B97F4A7C15) ⊕ k)`, `f = (u >> 40) as f32 · 2⁻²⁴` (k = stream id:
0 offset-r, 1 offset-angle, 2 size, 3 opacity, …). ~10 lines in core; avoids a dependency (else
[`rand_pcg`](https://crates.io/crates/rand_pcg) 0.10.2, MIT/Apache-2.0).
- Position scatter (uniform disk): `ρ = scatter·D·sqrt(f0)`, `ψ = 2π·f1` → `(x,y) += ρ(cos ψ, sin ψ)`.
- Size jitter `D·(1 − j_s·f2)`, opacity jitter `alpha·(1 − j_o·f3)`, angle jitter `j_a·(2f4 − 1)·π`.
- **Spray** (Krita spray engine model): each "dab" is a spray area of diameter `D`; emit `N = round(N_max·P)`
  particles, each a hard tiny dab (radius `r_p·(0.5 + f)`), placed uniform-disk (`sqrt`) or Gaussian
  (Box–Muller, `libm::logf/cosf`) for a denser center; particle `i` uses dab_index `(dab, i)`.

### 3.7 Grain (pencil, crayon)
Two placements (Procreate: **Texturized** = static paper behind the stroke vs **Moving** = rolls with the dab):
use **canvas-space** (texturized) for pencil and crayon so overlapping strokes reveal the same paper tooth.
Paper height `h(x,y) = T((x,y)/scale) ∈ [0,1]` from a tileable grayscale texture (`assets/brushes/paper-*.png`,
self-generated, e.g. seeded value/Perlin noise, so no third-party license). Tooth fill model (Krita "Subtract"
texturing + pressure; [texture docs](https://docs.krita.org/en/reference_manual/brushes/brush_settings/texture.html)):
```
level  = lerp(0.15, 1.0, P)                          // how deep the lead reaches
grain  = smoothstep(1 − level − w, 1 − level + w, h)  // peaks catch first, valleys fill with pressure
coverage multiplier = lerp(1, grain, grain_strength)
```
`w` = edge softness: pencil 0.15, crayon 0.05 (hard waxy breakup). Crayon also uses a larger `scale` (coarser tooth).
Grain applies to `a` before the union in §3.5, so pressing harder or repeated passes fill valleys.

### 3.8 Tilt side shading (pencil, shading)
`s = smoothstep(θ0, θ1, θ)` with θ0 = 45°, θ1 = 65° (keeps writing tilt ≤ ~42° on the point; shading at 50–60°
lands mid-to-full). Per dab:
```
aspect  = 1 + (A_max − 1)·s          // A_max pencil 4
radius  = r·(1 + (K_max − 1)·s)      // K_max pencil 3 (minor axis grows too)
angle   = α + π/2                    // major axis ⟂ lean direction (lead side contact)
hardness= lerp(h, h_side, s); alpha ×= lerp(1, 0.5, s); grain_strength = lerp(g, 1, s)
center += 0.5·radius·(cos α, sin α)·s   // contact patch shifts toward the lean
```
Elliptical dabs need spacing measured in the ellipse frame (MyPaint `count_dabs_to` divides by the rotated,
aspect-scaled distance) — use `spacing · minor diameter` along the travel direction to avoid gaps.
Verify on `fixtures/raw/20261024-094722/07-tilt-shade` vs `01-sentence`/`02-small` that writing never triggers `s > 0.1`.

## 4. Smoothing, velocity, streaming

### 4.1 One Euro filter ([Casiez, Roussel, Vogel, CHI 2012](https://gery.casiez.net/1euro/))
Per axis (x, y), with real `Δt` from timestamps (guard `Δt ≤ 0` → reuse last Δt; clamp to ≥ 0.5 ms):
```
α(fc) = 1 / (1 + τ/Δt),  τ = 1/(2π·fc)
dx    = (x − x̂_prev)/Δt;   dx̂ = dx̂_prev + α(d_cutoff)·(dx − dx̂_prev)
fc    = min_cutoff + β·|dx̂|          // use speed |(dx̂, dŷ)| for both axes so x and y share fc
x̂     = x̂_prev + α(fc)·(x − x̂_prev)
```
No transcendental functions → bit-identical everywhere. Starting values in px and px/s (tune on fixtures; tuning
procedure from the paper: β = 0, lower `min_cutoff` until slow-stroke jitter is gone, then raise β until fast
strokes have no visible lag): `min_cutoff = 4 Hz` (τ ≈ 40 ms at rest), `β = 0.01 /px` (Hz per px/s; 1000 px/s → fc ≈ 14 Hz,
τ ≈ 11 ms), `d_cutoff = 1 Hz`. Digitizer resolution is ~9 counts/px, so jitter is small; err on light smoothing.
Pressure: separate One Euro with `min_cutoff = 8 Hz, β = 0` (EMA) to kill 1-LSB flicker on wide brushes.
Last sample on pen-up: emit the raw point (perfect-freehand's `last` behaviour) so strokes end where the pen left.
Crates exist ([`one-euro`](https://crates.io/crates/one-euro) 0.9.0 MIT, `one-euro-rs` 0.2.0, 2019);
the filter is ~25 lines — implement in core `smoothing` to control float ops for determinism.

### 4.2 Velocity response
Speed `v` px/s = arc length of the quantized (unfiltered) sample positions over the last 10 ms of timestamps,
divided by the elapsed time (window spans at least the last segment; timestamps made monotone; elapsed ≥ 0.5 ms).
Not the One Euro derivative `|(dx̂, dŷ)|`: `(x − x̂_prev)/Δt` compares raw with lagged filtered positions and
overestimates true speed 3–4× in a rate-dependent way (that derivative only drives the filter's own cutoff).
Thinning for ink-like brushes (fast strokes thinner):
```
P_v = P · (1 − k_v · smoothstep(v0, v1, v))     v0 = 300 px/s, v1 = 3000 px/s
```
Apply before the size map. Fountain `k_v = 0.25`, ballpoint 0.1, others 0. Stamp brushes may use v for opacity
(pencil `k_v = 0.15` on alpha: fast strokes lighter).

### 4.3 Streaming (`StrokeBuilder`): stable vs tentative
Contract: `push(samples)` returns `{stable_delta, tentative}`; `finish()` returns the final geometry, which must
**equal** batch processing of all samples (core test: random chunkings of every fixture → identical bytes).

Outline family — causal pieces are stable; anything that depends on the future is tentative:
| Part | Depends on | Status |
|---|---|---|
| One Euro / centerline point i | samples ≤ i | stable at once |
| point i's offset (`nextVector`, corner test) | point i+1 | stable when i+1 exists |
| initial-pressure average | first ~10 samples / 2·size px | prefix tentative until window filled |
| min-length gate | first `size` px | tentative until passed |
| end taper, end-noise skip, `last` raw point | `totalLength` | last `taperEnd + 3 px` of arc tentative |
| end cap | stroke end | always tentative until `finish()` |
So: stable = left/right point prefixes up to the last point with `L_i ≤ L_total − taperEnd − 3 px` and `i ≤ n−2`;
tentative = remaining left/right tail + end cap + start cap, recomputed per push (bounded work). Renderer draws
`stable_prefix ∪ tentative_tail` as one path; on the tablet the stable part can be cached as a bitmap only for
stamp brushes (outline polygons are cheap to refill). `StrokeDelta` (PLAN sync) streams raw samples, not geometry —
the browser runs the same builder.

Stamp family: dabs are emitted **only when final** — a dab at arc position s is fixed once the filtered path past s
exists and inputs at s are known. With One Euro the filtered point i is final when sample i arrives, so dabs on
segments up to the newest filtered point are stable; the only tentative bit is the pen-up raw-point replacement
of the last segment (re-emit dabs of the last segment on `finish()`). Carry `φ`, `dab_index` in builder state.

## 5. Presets

Pressure curve `c(p)` for all presets unless noted: `x = clamp((p − 0.02)/(0.90 − 0.02), 0, 1)` (floor ~80 kills
the near-zero contact samples; saturates at ~3700 since hard strokes hit 4095), then `c = x^γ`, default γ = 0.75
(maps small writing p≈0.32 → 0.446, normal p≈0.49 → 0.625, p95 0.80 → 0.914: keeps resolution in the 500–2500 band).
Store curves as monotone cubic (Fritsch–Carlson) control points for user editing; the default is sampled from
the formula above. Start-of-stroke floor: outline radius never < `Dmin/2` (thinning formula), stamp size `≥ Dmin`.
Sizes are diameters in px at 340 dpi (13.39 px/mm). "pf" = perfect-freehand params (§1.3 mapping
`size=(Dmin+Dmax)/2, thinning=(Dmax−Dmin)/(Dmax+Dmin)`; `streamline=0`, `smoothing=0.5`, `simulatePressure=false`).

| Preset | Family | D min–max px (mm) | Opacity | Flow | Spacing | Hardness | Scatter | Curve / response | Tilt / velocity | Other |
|---|---|---|---|---|---|---|---|---|---|---|
| ballpoint | outline pf | 2.4–5.0 (0.18–0.37) | 1.0 | – | – | – | – | γ 0.75 | k_v 0.1 | round caps, taper start 0 / end 2 px |
| fineliner | outline pf | 3.8–4.6 (≈0.3) | 1.0 | – | – | – | – | γ 1.0 (near-constant) | none | round caps, no taper |
| fountain | outline pf + ellipse nib (§2.2) | 2.5–9.0 | 1.0 | – | – | – | – | γ 0.8 | k_v 0.25; ellipse a 0.55, φ 45° | round caps, end taper 6 px |
| calligraphy | outline nib (§2.1) | W 10–22, t 1.5 | 1.0 | – | – | – | – | W = W_max(0.55 + 0.45c) | φ 40°, fixation 1 | flat caps, corner fans off |
| marker | outline pf (round) / nib (chisel) | 14–18 round; chisel W 20 t 7 | 0.9 (flat) | – | – | – | – | γ 1.0, thinning ≈ 0.1 | none | round caps / chisel φ 30° |
| highlighter | outline nib | W 26, t 8 | 0.35, multiply | – | – | – | – | pressure off | none | φ 90°, flat caps, under-ink band |
| pencil | stamp | 3–6 (0.22–0.45) | 0.9 | 0.35 | 0.10 | 0.6 | 0 | size g 0.5, k_flow 0.85 (pressure mostly → darkness) | tilt §3.8 (A 4, K 3, h_side 0.3); k_v 0.15 on alpha | grain canvas, strength 0.7, w 0.15, scale 1 |
| crayon | stamp | 8–14 | 1.0 | 0.6 | 0.08 | 0.8 | 0.05 | k_flow 0.5, size g 0.7 | tilt K 1.5 | grain coarse scale 3, strength 0.9, w 0.05 |
| airbrush | stamp | 30–90 | 1.0 (build-up) | 0.05 | 0.05 | 0.05 | 0 | pressure → flow (k_flow 1), size g 0.3 | none | dabs_per_second 60 |
| spray | stamp (particles) | area 40–80; particle 1–2.5 | 1.0 | 0.6/particle | 0.25 | 1.0 | uniform disk | N_max 12, N = round(12·c) | none | Gaussian option |
| soft | stamp | 12–48 | 1.0 | 0.15 | 0.08 | 0.25 | 0 | size g 1.0, k_flow 0.6 | none | wash mode |
| solid | stamp | 6–24 | 1.0 | 1.0 | 0.06 | 0.95 | 0 | size g 1.0 | none | 1 px min fade |
| shading | stamp | 40–160 | 0.3 cap | 0.06 | 0.10 | 0.1 | 0 | k_flow 0.8, size g 0.5 | tilt K 2 (no aspect), A 1.5 | grain strength 0.3 |

Tuning notes from `docs/research/pen-recordings.md`:
- **Dots** (09-dots: 45 strokes, ~30 frames each, p50 2315): outline `n == 1` dot path and stamp "first dab at s=0"
  must give a visible round dot; with the min-length gate at ≤ 1 px dots keep their real shape.
- **Fast** (03-fast, p50 3005) and **speed** (06-speed): check k_v doesn't thin fast handwriting below `Dmin`.
- **Scrub** (11-scrub, p ≈ 3600 sustained, one 4733-frame stroke): stress test for §3.5 cap (must stay ≤ opacity) and
  for builder cost (thousands of dabs; stamp builder must be O(new dabs) per push).
- **Pressure ramp/steady** (04, 05): plot `c(p)` → D to confirm visibly distinct widths between p 0.15 and 0.6 and
  saturation only above ~0.9.
- Start pressure ≈ 0 on many strokes: §1.3 initial-pressure averaging + Dmin floor handle it; no extra taper.

## Pitfalls checklist
- Never use `f32::hypot/sin/powf` from std in core; `libm` or basic ops only. Avoid FMA-dependent code (`mul_add`).
- perfect-freehand's `vector` points backwards; `per` is clockwise in y-down screen space — left/right naming is
  relative to that; keep upstream orientation so ports of its tests apply.
- Equality skip uses exact float compare — keep it (deterministic), but quantized input makes duplicates common.
- Spacing must be clamped (≥ 0.5 px) or tiny dabs explode dab counts; cap dabs per stroke (e.g. 200 k) as a safety.
- Highlighter must be one polygon per stroke; splitting it (eraser) must keep each piece a single nonzero fill.
- Don't time-emit dabs from the UI thread clock; only from sample timestamps.
