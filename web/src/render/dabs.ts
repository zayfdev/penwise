/**
 * Dab strokes (pencil, crayon, airbrush, …) on Canvas2D. The coverage `S` of a stroke is computed per pixel by
 * the port of core's stamp renderer in `stamps.ts` (per-dab falloff, per-dab grain, minimum fade, union rule),
 * then tinted with the stroke color and composited with the stroke's blend over what is already painted.
 */

import { DAB_STRIDE, type DrawData, type Style } from "./drawdata";
import type { PaperGrain } from "./grain";
import { context2d, type LayerPool } from "./layers";
import { type Matrix, scaleOf } from "./matrix";
import { accumulate, type Dab, type PixelDab, pixelDab, strokeRect, type View } from "./stamps";

/** What dab painting needs from the renderer. */
export interface DabEnv {
  readonly grain: PaperGrain;
  readonly layers: LayerPool;
  /** Page → device matrix of `target` (uniform scale plus translation). */
  readonly base: Matrix;
}

/** Dabs `first..first+count` of the draw list's dab array (`DAB_STRIDE` floats each). */
export function readDabs(data: DrawData, first: number, count: number): Dab[] {
  const out: Dab[] = [];
  for (let i = first; i < first + count; i++) {
    const o = i * DAB_STRIDE;
    const f = (k: number): number => data.dabs[o + k] ?? 0;
    out.push({
      x: f(0),
      y: f(1),
      radiusX: f(2),
      radiusY: f(3),
      angle: f(4),
      opacity: f(5),
      hardness: f(6),
      grainStrength: f(7),
      grainLevel: f(8),
    });
  }
  return out;
}

/** The viewport of a page → device matrix (core `Viewport`: `device = (page − origin) · scale`). */
export function viewOf(base: Matrix): View {
  const scale = scaleOf(base);
  return { scale, originX: -base.e / scale, originY: -base.f / scale };
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
  const view = viewOf(env.base);
  const dabs: PixelDab[] = [];
  for (const dab of readDabs(data, first, count)) {
    const pd = pixelDab(dab, view);
    if (pd) dabs.push(pd);
  }
  const rect = strokeRect(dabs, target.canvas.width, target.canvas.height);
  if (!rect) return;
  const [bx, by, ex, ey] = rect;
  const [w, h] = [ex - bx, ey - by];
  const coverage = new Float32Array(w * h);
  const { grainScale, grainSoftness } = style;
  const grain =
    grainScale > 0
      ? {
          style: { scale: grainScale, softness: grainSoftness },
          height: (x: number, y: number) => env.grain.height(x, y, grainScale),
          view,
        }
      : null;
  accumulate(coverage, rect, dabs, grain);

  const layer = env.layers.acquire(w, h);
  const sctx = context2d(layer);
  const image = sctx.createImageData(w, h);
  const [r, g, b] = rgbBytes(style);
  for (let i = 0; i < coverage.length; i++) {
    const o = i * 4;
    image.data[o] = r;
    image.data[o + 1] = g;
    image.data[o + 2] = b;
    image.data[o + 3] = Math.round((coverage[i] ?? 0) * 255);
  }
  sctx.putImageData(image, 0, 0);
  target.save();
  target.setTransform(1, 0, 0, 1, 0, 0);
  target.globalAlpha = style.a * style.opacity;
  target.globalCompositeOperation = style.multiply ? "multiply" : "source-over";
  target.drawImage(layer, bx, by);
  target.restore();
  env.layers.release(layer);
}

function rgbBytes(style: Style): [number, number, number] {
  const byte = (v: number): number => Math.round(Math.min(1, Math.max(0, v)) * 255);
  return [byte(style.r), byte(style.g), byte(style.b)];
}
