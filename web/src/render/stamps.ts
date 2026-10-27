/**
 * Dab stroke coverage, the browser's copy of core `raster::stamps` (read the formula there, in the module docs).
 * Pure math: no canvas, so `stamps.test.ts` replays `crates/penwise-core/tests/goldens/stamp_coverage.txt` through
 * it and compares with core's own coverage. `dabs.ts` turns the coverage buffer into pixels.
 */

/** Page → device mapping of a similarity view: `device = (page − origin) · scale`. */
export interface View {
  readonly scale: number;
  readonly originX: number;
  readonly originY: number;
}

/** One dab as the draw list carries it (page px). */
export interface Dab {
  readonly x: number;
  readonly y: number;
  readonly radiusX: number;
  readonly radiusY: number;
  readonly angle: number;
  readonly opacity: number;
  readonly hardness: number;
  readonly grainStrength: number;
  readonly grainLevel: number;
}

/** The stroke's paper grain (`style.grainScale > 0`); per-dab level and strength travel with each dab. */
export interface GrainStyle {
  readonly scale: number;
  readonly softness: number;
}

/** Paper height at a page position (`PaperGrain.height` bound to the grain scale). */
export type HeightAt = (x: number, y: number) => number;

const MIN_FADE_HARDNESS = 0.1;
const MIN_HARDNESS = 1e-3;

const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

/** Core `dab_falloff`: MyPaint two-segment falloff at `rr = (dist/r)²`. */
export function falloff(rr: number, hardness: number): number {
  if (Number.isNaN(rr) || rr > 1) return 0;
  if (Number.isNaN(hardness) || hardness >= 1) return 1;
  if (hardness <= 0) return 0;
  const r = Math.max(rr, 0);
  const opa = r <= hardness ? 1 + r * (1 - 1 / hardness) : (hardness / (1 - hardness)) * (1 - r);
  return clamp01(opa);
}

/** Core `min_fade`: `[radius', hardness', faded]` for a pixel radius; keeps the optical radius, ≥ 1 px fade. */
export function minFade(radius: number, hardness: number): [number, number, boolean] {
  const needsFade = radius > 0 && Number.isFinite(radius) && radius * (1 - hardness) < 1;
  if (!needsFade) return [radius, hardness, false];
  const rOpt = radius - ((1 - hardness) * radius) / 2;
  const h = Math.max((rOpt - 0.5) / (rOpt + 0.5), MIN_FADE_HARDNESS);
  return [1 / (1 - h), h, true];
}

/** Core `smoothstep`: a step at `e0` when `e1 ≤ e0`. */
function smoothstep(e0: number, e1: number, x: number): number {
  if (e1 <= e0) return x >= e0 ? 1 : 0;
  const t = clamp01((x - e0) / (e1 - e0));
  return t * t * (3 - 2 * t);
}

/** Core `grain_coverage`: `lerp(1, smoothstep(1 − level − w, 1 − level + w, height), strength)`, `w = softness`. */
export function grainCoverage(
  height: number,
  level: number,
  softness: number,
  strength: number,
): number {
  const edge = 1 - level;
  return 1 + (smoothstep(edge - softness, edge + softness, height) - 1) * strength;
}

/** A dab in device px with the minimum fade applied (core `PixelDab`). */
export interface PixelDab {
  readonly cx: number;
  readonly cy: number;
  readonly invRx2: number;
  readonly invRy2: number;
  /** Circumscribing radius (bbox half-extent). */
  readonly extent: number;
  readonly sin: number;
  readonly cos: number;
  readonly hardness: number;
  readonly opacity: number;
  readonly grainStrength: number;
  readonly grainLevel: number;
}

/** Core `PixelDab::new`; `null` for invisible or malformed dabs. */
export function pixelDab(dab: Dab, view: View): PixelDab | null {
  const opacity = clamp01(dab.opacity);
  const fields = [dab.x, dab.y, dab.radiusX, dab.radiusY, dab.angle];
  if (!(dab.radiusX > 0 && dab.radiusY > 0 && opacity > 0) || !fields.every(Number.isFinite)) {
    return null;
  }
  const cx = (dab.x - view.originX) * view.scale;
  const cy = (dab.y - view.originY) * view.scale;
  const rx = dab.radiusX * view.scale;
  const ry = dab.radiusY * view.scale;
  const h = Math.max(clamp01(dab.hardness), MIN_HARDNESS);
  // Fade along the minor axis; the major axis keeps its optical radius but never ends up shorter than the faded
  // minor one (see core).
  const [rMinor, fadedH, faded] = minFade(Math.min(rx, ry), h);
  const keep = (r: number): number => (faded ? Math.max((r * (1 + h)) / (1 + fadedH), rMinor) : r);
  const [fx, fy] = rx < ry ? [rMinor, keep(ry)] : [keep(rx), rMinor];
  const extent = Math.max(fx, fy);
  if (!(Number.isFinite(cx) && Number.isFinite(cy) && Number.isFinite(extent))) return null;
  return {
    cx,
    cy,
    invRx2: 1 / (fx * fx),
    invRy2: 1 / (fy * fy),
    extent,
    sin: Math.sin(dab.angle),
    cos: Math.cos(dab.angle),
    hardness: fadedH,
    opacity,
    grainStrength: clamp01(dab.grainStrength),
    grainLevel: dab.grainLevel,
  };
}

/** Pixel `(col, row)`'s contribution of one dab: `a = falloff · opacity · grain`, 0 outside the dab. */
export function dabAlpha(
  pd: PixelDab,
  col: number,
  row: number,
  grain: { style: GrainStyle; height: HeightAt; view: View } | null,
): number {
  const px = col + 0.5;
  const py = row + 0.5;
  const fx = px - pd.cx;
  const fy = py - pd.cy;
  const xr = fx * pd.cos + fy * pd.sin;
  const yr = fy * pd.cos - fx * pd.sin;
  const f = falloff(xr * xr * pd.invRx2 + yr * yr * pd.invRy2, pd.hardness);
  if (f <= 0) return 0;
  let a = f * pd.opacity;
  if (grain && pd.grainStrength > 0) {
    const qx = px / grain.view.scale + grain.view.originX;
    const qy = py / grain.view.scale + grain.view.originY;
    a *= grainCoverage(grain.height(qx, qy), pd.grainLevel, grain.style.softness, pd.grainStrength);
  }
  return clamp01(a);
}

/** Pixel `(col, row)` of the stroke's coverage `S` (union of the dabs, in order). */
export function coverageAt(
  dabs: readonly PixelDab[],
  col: number,
  row: number,
  grain: { style: GrainStyle; height: HeightAt; view: View } | null,
): number {
  let s = 0;
  for (const pd of dabs) s += dabAlpha(pd, col, row, grain) * (1 - s);
  return s;
}

/** Device-pixel rect `[x0, y0, x1, y1)` covering every dab, clamped to `[0, width] × [0, height]`; `null` if empty. */
export function strokeRect(
  dabs: readonly PixelDab[],
  width: number,
  height: number,
): [number, number, number, number] | null {
  let x0 = Number.POSITIVE_INFINITY;
  let y0 = Number.POSITIVE_INFINITY;
  let x1 = Number.NEGATIVE_INFINITY;
  let y1 = Number.NEGATIVE_INFINITY;
  for (const pd of dabs) {
    x0 = Math.min(x0, pd.cx - pd.extent);
    y0 = Math.min(y0, pd.cy - pd.extent);
    x1 = Math.max(x1, pd.cx + pd.extent);
    y1 = Math.max(y1, pd.cy + pd.extent);
  }
  const bx0 = Math.max(0, Math.floor(x0));
  const by0 = Math.max(0, Math.floor(y0));
  const bx1 = Math.min(width, Math.ceil(x1));
  const by1 = Math.min(height, Math.ceil(y1));
  return bx1 > bx0 && by1 > by0 ? [bx0, by0, bx1, by1] : null;
}

/**
 * Accumulates `dabs` into `buffer` (`w × h` coverage values covering device pixels from `(x0, y0)`), the union rule
 * dab by dab exactly like core's `render_stamps`.
 */
export function accumulate(
  buffer: Float32Array,
  rect: readonly [number, number, number, number],
  dabs: readonly PixelDab[],
  grain: { style: GrainStyle; height: HeightAt; view: View } | null,
): void {
  const [x0, y0, x1, y1] = rect;
  const w = x1 - x0;
  for (const pd of dabs) {
    const lx = Math.max(x0, Math.floor(pd.cx - pd.extent));
    const hx = Math.min(x1, Math.ceil(pd.cx + pd.extent));
    const ly = Math.max(y0, Math.floor(pd.cy - pd.extent));
    const hy = Math.min(y1, Math.ceil(pd.cy + pd.extent));
    for (let row = ly; row < hy; row++) {
      for (let col = lx; col < hx; col++) {
        const a = dabAlpha(pd, col, row, grain);
        if (a <= 0) continue;
        const i = (row - y0) * w + (col - x0);
        const s = buffer[i] ?? 0;
        buffer[i] = s + a * (1 - s);
      }
    }
  }
}
