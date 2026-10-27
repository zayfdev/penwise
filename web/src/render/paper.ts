/** Paper under the ink: sheet, and the ruled / grid / dot backgrounds (same colors and spacing as the server PNG). */

import type { PageEntry } from "../wasm";
import { apply, type Matrix, setMatrix } from "./matrix";

/** Mirrors proto `BACKGROUND_SPACING_PX`. */
export const BACKGROUND_SPACING_PX = 93.7;

const RULED_COLOR = "rgb(201 214 232)";
const GRID_COLOR = "rgb(220 227 236)";
const DOT_COLOR = "rgb(198 206 218)";
/** Backgrounds with `proto Background::code`. */
const LINED = 1;
const GRID = 2;
const DOT = 3;

/** Page-space rectangle `[x0, y0, x1, y1]`. */
export type Rect = readonly [number, number, number, number];

/** Multiples of `spacing` strictly inside `(lo, hi)`, starting at the first positive multiple. */
export function rulings(lo: number, hi: number, spacing: number): number[] {
  const out: number[] = [];
  const first = Math.max(1, Math.floor(lo / spacing) + 1);
  for (let k = first; k * spacing < hi; k++) out.push(k * spacing);
  return out;
}

/** Page rect covered by the device viewport of `base`. */
export function visibleRect(base: Matrix, width: number, height: number): Rect {
  const inv = (x: number, y: number): [number, number] => [
    (x - base.e) / base.a,
    (y - base.f) / base.d,
  ];
  const [x0, y0] = inv(0, 0);
  const [x1, y1] = inv(width, height);
  return [x0, y0, x1, y1];
}

/** Clears `ctx` and draws the sheet and background of `page` under `base` (page → device). */
export function paintPaper(ctx: CanvasRenderingContext2D, page: PageEntry, base: Matrix): void {
  const { width, height } = ctx.canvas;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const area: Rect = page.fixed
    ? [0, 0, page.width, page.height]
    : visibleRect(base, width, height);
  setMatrix(ctx, base);
  ctx.fillStyle = "#fff";
  ctx.fillRect(area[0], area[1], area[2] - area[0], area[3] - area[1]);
  drawBackground(ctx, page.background, area, 1 / base.a);
  if (page.fixed) drawSheetEdge(ctx, page, base);
  ctx.restore();
}

function drawBackground(
  ctx: CanvasRenderingContext2D,
  background: number,
  area: Rect,
  deviceUnit: number,
): void {
  if (background !== LINED && background !== GRID && background !== DOT) return;
  const [x0, y0, x1, y1] = area;
  if (BACKGROUND_SPACING_PX / deviceUnit < 4) return;
  const ys = rulings(y0, y1, BACKGROUND_SPACING_PX);
  const xs = rulings(x0, x1, BACKGROUND_SPACING_PX);
  if (background === LINED) {
    ctx.fillStyle = RULED_COLOR;
    for (const y of ys) ctx.fillRect(x0, y, x1 - x0, deviceUnit);
  } else if (background === GRID) {
    ctx.fillStyle = GRID_COLOR;
    for (const y of ys) ctx.fillRect(x0, y, x1 - x0, deviceUnit);
    for (const x of xs) ctx.fillRect(x, y0, deviceUnit, y1 - y0);
  } else {
    ctx.fillStyle = DOT_COLOR;
    const d = 3 * deviceUnit;
    for (const y of ys) for (const x of xs) ctx.fillRect(x - d / 2, y - d / 2, d, d);
  }
}

function drawSheetEdge(ctx: CanvasRenderingContext2D, page: PageEntry, base: Matrix): void {
  const [x0, y0] = apply(base, 0, 0);
  const [x1, y1] = apply(base, page.width, page.height);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.strokeStyle = "rgb(0 0 0 / 0.18)";
  ctx.lineWidth = 1;
  ctx.strokeRect(x0 + 0.5, y0 + 0.5, x1 - x0 - 1, y1 - y0 - 1);
  ctx.restore();
}
