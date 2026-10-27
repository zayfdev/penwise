/**
 * Draw-list painter (command semantics: `crates/penwise-wasm/src/draw_list.rs`). Never computes stroke
 * geometry: every outline, dab, mask and box comes from wasm.
 */

import { type DabEnv, paintDabs } from "./dabs";
import {
  type Command,
  commandAt,
  commandCount,
  cssColor,
  type DrawData,
  Kind,
  runRange,
  type Style,
  styleAt,
} from "./drawdata";
import type { PaperGrain } from "./grain";
import type { ImageCache } from "./images";
import { context2d, type LayerPool } from "./layers";
import { type Matrix, mul, setMatrix } from "./matrix";

/** Everything a paint pass needs besides the draw list. */
export interface PaintEnv {
  readonly grain: PaperGrain;
  readonly images: ImageCache;
  readonly layers: LayerPool;
  /** Page → device matrix of the target canvas. */
  readonly base: Matrix;
}

const TEXT_LINE_HEIGHT = 1.25;

/** Paints `data` onto `ctx` (whose canvas size is the device viewport). Leaves the transform untouched. */
export function paintDrawList(ctx: CanvasRenderingContext2D, data: DrawData, env: PaintEnv): void {
  ctx.save();
  setMatrix(ctx, env.base);
  paintRange(ctx, data, env, 0, commandCount(data));
  ctx.restore();
}

function paintRange(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  env: PaintEnv,
  start: number,
  end: number,
): void {
  for (let i = start; i < end; i++) {
    const cmd = commandAt(data, i);
    if (cmd.kind === Kind.GroupBegin) {
      const close = findGroupEnd(data, i + 1, end);
      paintGroup(ctx, data, env, cmd, i + 1, close);
      i = close;
    } else {
      paintCommand(ctx, data, env, cmd);
    }
  }
}

function findGroupEnd(data: DrawData, from: number, end: number): number {
  for (let i = from; i < end; i++) {
    if (commandAt(data, i).kind === Kind.GroupEnd) return i;
  }
  return end;
}

/** An erased item: content into an offscreen layer, masks cut out of it, then composited with its blend. */
function paintGroup(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  env: PaintEnv,
  group: Command,
  start: number,
  end: number,
): void {
  const layer = env.layers.acquire(ctx.canvas.width, ctx.canvas.height);
  const lctx = context2d(layer);
  setMatrix(lctx, env.base);
  paintRange(lctx, data, env, start, end);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = styleAt(data, group.style).multiply ? "multiply" : "source-over";
  ctx.drawImage(layer, 0, 0);
  ctx.restore();
  env.layers.release(layer);
}

function paintCommand(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  env: PaintEnv,
  cmd: Command,
): void {
  switch (cmd.kind) {
    case Kind.PathNonZero:
      fillPath(ctx, data, cmd, "nonzero");
      break;
    case Kind.PathEvenOdd:
      fillPath(ctx, data, cmd, "evenodd");
      break;
    case Kind.Dabs: {
      const dabEnv: DabEnv = { grain: env.grain, layers: env.layers, base: env.base };
      paintDabs(ctx, data, cmd.first, cmd.count, styleAt(data, cmd.style), dabEnv);
      setMatrix(ctx, env.base);
      break;
    }
    case Kind.Mask:
      strokeMask(ctx, data, cmd);
      break;
    case Kind.Text:
      paintText(ctx, data, env, cmd);
      break;
    case Kind.Image:
      paintImage(ctx, data, env, cmd);
      break;
    default:
      break;
  }
}

function applyStyle(ctx: CanvasRenderingContext2D, style: Style): void {
  ctx.fillStyle = cssColor(style);
  ctx.globalAlpha = Math.min(1, Math.max(0, style.a * style.opacity));
  ctx.globalCompositeOperation = style.multiply ? "multiply" : "source-over";
}

function pathOf(data: DrawData, first: number, count: number, closed: boolean): Path2D {
  const path = new Path2D();
  for (let run = first; run < first + count; run++) {
    const [from, to] = runRange(data, run);
    if (to - from < 2) continue;
    path.moveTo(data.points[from] ?? 0, data.points[from + 1] ?? 0);
    for (let k = from + 2; k < to; k += 2) {
      path.lineTo(data.points[k] ?? 0, data.points[k + 1] ?? 0);
    }
    if (closed) path.closePath();
  }
  return path;
}

function fillPath(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  cmd: Command,
  rule: CanvasFillRule,
): void {
  ctx.save();
  applyStyle(ctx, styleAt(data, cmd.style));
  ctx.fill(pathOf(data, cmd.first, cmd.count, true), rule);
  ctx.restore();
}

function strokeMask(ctx: CanvasRenderingContext2D, data: DrawData, cmd: Command): void {
  ctx.save();
  ctx.globalCompositeOperation = "destination-out";
  ctx.globalAlpha = 1;
  ctx.strokeStyle = "#000";
  ctx.lineWidth = data.maskWidths[cmd.aux] ?? 1;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.stroke(pathOf(data, cmd.first, cmd.count, false));
  ctx.restore();
}

/** Local box frame → page: local `(0,0)` → tl, `(w,0)` → tr, `(0,h)` → bl. */
function boxFrame(data: DrawData, run: number, width: number, height: number): Matrix | null {
  const [o] = runRange(data, run);
  const p = (i: number) => data.points[o + i] ?? 0;
  if (!(width > 0 && height > 0)) return null;
  const [tlx, tly, trx, try_, , , blx, bly] = [p(0), p(1), p(2), p(3), p(4), p(5), p(6), p(7)];
  return {
    a: (trx - tlx) / width,
    b: (try_ - tly) / width,
    c: (blx - tlx) / height,
    d: (bly - tly) / height,
    e: tlx,
    f: tly,
  };
}

function paintText(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  env: PaintEnv,
  cmd: Command,
): void {
  const text = data.texts[cmd.aux] ?? "";
  const size = data.textSizes[cmd.aux] ?? 16;
  const width = data.textBoxes[cmd.aux * 2] ?? 0;
  const height = data.textBoxes[cmd.aux * 2 + 1] ?? 0;
  const frame = boxFrame(data, cmd.first, width, height);
  if (!frame || text === "") return;
  ctx.save();
  setMatrix(ctx, mul(env.base, frame));
  applyStyle(ctx, styleAt(data, cmd.style));
  ctx.font = `${size}px system-ui, sans-serif`;
  ctx.textBaseline = "top";
  let y = 0;
  for (const line of wrapLines(text, width, (s) => ctx.measureText(s).width)) {
    ctx.fillText(line, 0, y);
    y += size * TEXT_LINE_HEIGHT;
  }
  ctx.restore();
}

/** Greedy word wrap to `width` (a word longer than the box stays on its own line). */
export function wrapLines(text: string, width: number, measure: (s: string) => number): string[] {
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    let line = "";
    for (const word of paragraph.split(/\s+/).filter((w) => w !== "")) {
      const candidate = line === "" ? word : `${line} ${word}`;
      if (line !== "" && measure(candidate) > width) {
        lines.push(line);
        line = word;
      } else {
        line = candidate;
      }
    }
    lines.push(line);
  }
  return lines;
}

function paintImage(
  ctx: CanvasRenderingContext2D,
  data: DrawData,
  env: PaintEnv,
  cmd: Command,
): void {
  const hash = data.images[cmd.aux];
  const bitmap = hash === undefined ? null : env.images.get(hash);
  if (!bitmap) return;
  const frame = boxFrame(data, cmd.first, bitmap.width, bitmap.height);
  if (!frame) return;
  ctx.save();
  setMatrix(ctx, mul(env.base, frame));
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = "source-over";
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bitmap, 0, 0);
  ctx.restore();
}
