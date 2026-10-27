/**
 * Dab strokes (pencil, crayon, airbrush, …) on Canvas2D. Per the draw-list contract: dabs are drawn as white
 * elliptical radial falloffs into a coverage canvas (source-over = `S ← S + a(1−S)`), multiplied by the paper
 * grain, tinted, then composited with the stroke's blend. Grain is one level per stroke (mean of its dabs).
 */

import { cssColor, DAB_STRIDE, type DrawData, type Style } from "./drawdata";
import type { GrainCache } from "./grain";
import { context2d, type LayerPool } from "./layers";
import { type Matrix, mul, scaleOf, setMatrix } from "./matrix";

/** What dab painting needs from the renderer. */
export interface DabEnv {
  readonly grain: GrainCache;
  readonly layers: LayerPool;
  /** Page → device matrix of `target`. */
  readonly base: Matrix;
}

const FALLOFF_STOPS = 12;

/** `MyPaint` two-segment falloff at `rr = (dist/r)²` (core `raster::stamps::dab_falloff`). */
export function falloff(rr: number, hardness: number): number {
  if (Number.isNaN(rr) || rr > 1) return 0;
  if (Number.isNaN(hardness) || hardness >= 1) return 1;
  if (hardness <= 0) return 0;
  const r = Math.max(rr, 0);
  const opa = r <= hardness ? 1 + r * (1 - 1 / hardness) : (hardness / (1 - hardness)) * (1 - r);
  return Math.min(1, Math.max(0, opa));
}

/** Device-px bounds `[x0, y0, x1, y1]` of dabs `first..first+count`, clamped to the target; `null` if empty. */
export function dabBounds(
  data: DrawData,
  first: number,
  count: number,
  base: Matrix,
  width: number,
  height: number,
): [number, number, number, number] | null {
  const s = scaleOf(base);
  let x0 = Number.POSITIVE_INFINITY;
  let y0 = Number.POSITIVE_INFINITY;
  let x1 = Number.NEGATIVE_INFINITY;
  let y1 = Number.NEGATIVE_INFINITY;
  for (let i = first; i < first + count; i++) {
    const o = i * DAB_STRIDE;
    const x = data.dabs[o] ?? 0;
    const y = data.dabs[o + 1] ?? 0;
    const r = Math.max(data.dabs[o + 2] ?? 0, data.dabs[o + 3] ?? 0) * s + 1;
    const cx = base.a * x + base.c * y + base.e;
    const cy = base.b * x + base.d * y + base.f;
    x0 = Math.min(x0, cx - r);
    y0 = Math.min(y0, cy - r);
    x1 = Math.max(x1, cx + r);
    y1 = Math.max(y1, cy + r);
  }
  const bx0 = Math.max(0, Math.floor(x0));
  const by0 = Math.max(0, Math.floor(y0));
  const bx1 = Math.min(width, Math.ceil(x1));
  const by1 = Math.min(height, Math.ceil(y1));
  return bx1 > bx0 && by1 > by0 ? [bx0, by0, bx1, by1] : null;
}

/** Paints one `Dabs` command onto `target` (identity transform expected on exit). */
export function paintDabs(
  target: CanvasRenderingContext2D,
  data: DrawData,
  first: number,
  count: number,
  style: Style,
  env: DabEnv,
): void {
  const { width, height } = target.canvas;
  const bounds = dabBounds(data, first, count, env.base, width, height);
  if (!bounds) return;
  const [bx, by, ex, ey] = bounds;
  const layer = env.layers.acquire(ex - bx, ey - by);
  const sctx = context2d(layer);
  const local: Matrix = { ...env.base, e: env.base.e - bx, f: env.base.f - by };
  drawCoverage(sctx, data, first, count, local);
  applyGrain(sctx, data, first, count, style, local, env.grain);
  sctx.setTransform(1, 0, 0, 1, 0, 0);
  sctx.globalCompositeOperation = "source-in";
  sctx.globalAlpha = 1;
  sctx.fillStyle = cssColor(style);
  sctx.fillRect(0, 0, layer.width, layer.height);
  target.save();
  target.setTransform(1, 0, 0, 1, 0, 0);
  target.globalAlpha = style.a * style.opacity;
  target.globalCompositeOperation = style.multiply ? "multiply" : "source-over";
  target.drawImage(layer, bx, by);
  target.restore();
  env.layers.release(layer);
}

function drawCoverage(
  sctx: CanvasRenderingContext2D,
  data: DrawData,
  first: number,
  count: number,
  local: Matrix,
): void {
  const gradients = new Map<number, CanvasGradient>();
  sctx.globalCompositeOperation = "source-over";
  for (let i = first; i < first + count; i++) {
    const o = i * DAB_STRIDE;
    const [x, y, rx, ry, angle, opacity, hardness] = [
      data.dabs[o] ?? 0,
      data.dabs[o + 1] ?? 0,
      data.dabs[o + 2] ?? 0,
      data.dabs[o + 3] ?? 0,
      data.dabs[o + 4] ?? 0,
      data.dabs[o + 5] ?? 0,
      data.dabs[o + 6] ?? 1,
    ];
    if (!(rx > 0 && ry > 0) || opacity <= 0) continue;
    const key = Math.round(Math.min(1, Math.max(0, hardness)) * 64);
    let gradient = gradients.get(key);
    if (!gradient) {
      gradient = sctx.createRadialGradient(0, 0, 0, 0, 0, 1);
      for (let k = 0; k <= FALLOFF_STOPS; k++) {
        const d = k / FALLOFF_STOPS;
        gradient.addColorStop(d, `rgb(255 255 255 / ${falloff(d * d, key / 64)})`);
      }
      gradients.set(key, gradient);
    }
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    setMatrix(
      sctx,
      mul(local, { a: cos * rx, b: sin * rx, c: -sin * ry, d: cos * ry, e: x, f: y }),
    );
    sctx.globalAlpha = Math.min(1, opacity);
    sctx.fillStyle = gradient;
    sctx.fillRect(-1, -1, 2, 2);
  }
}

function applyGrain(
  sctx: CanvasRenderingContext2D,
  data: DrawData,
  first: number,
  count: number,
  style: Style,
  local: Matrix,
  grain: GrainCache,
): void {
  if (style.grainScale <= 0) return;
  let strength = 0;
  let level = 0;
  for (let i = first; i < first + count; i++) {
    strength += data.dabs[i * DAB_STRIDE + 7] ?? 0;
    level += data.dabs[i * DAB_STRIDE + 8] ?? 0;
  }
  strength /= count;
  level /= count;
  if (strength <= 0) return;
  const pattern = grain.pattern(sctx, style.grainScale, level, style.grainSoftness, strength);
  if (!pattern) return;
  pattern.setTransform(new DOMMatrix([local.a, local.b, local.c, local.d, local.e, local.f]));
  sctx.setTransform(1, 0, 0, 1, 0, 0);
  sctx.globalAlpha = 1;
  sctx.globalCompositeOperation = "destination-in";
  sctx.fillStyle = pattern;
  sctx.fillRect(0, 0, sctx.canvas.width, sctx.canvas.height);
}
