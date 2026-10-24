/**
 * Canvas2D renderer of wasm geometry (draw list layout: `crates/penwise-wasm/src/draw_list.rs`).
 *
 * Owner: slice d.
 *
 * Never computes stroke geometry (AGENTS.md "TypeScript"): outlines, dabs, masks and text boxes all come from
 * the wasm `DrawList`. Three stacked canvases: `paper` (background pattern), `ink` (committed items), `live`
 * (in-progress strokes, redrawn every animation frame while live ink changes). Each is sized to CSS size ×
 * `devicePixelRatio`; the page → canvas transform is `setTransform(s·dpr, 0, 0, s·dpr, −ox·s·dpr, −oy·s·dpr)`.
 *
 * Command rendering:
 * - `PathNonZero` / `PathEvenOdd`: one `Path2D` of the runs (`moveTo` + `lineTo` + `closePath` each),
 *   `fill(path, "nonzero" | "evenodd")`, `fillStyle` = style rgb, `globalAlpha` = a · opacity,
 *   `globalCompositeOperation` = "multiply" when blend = 1 (highlighter), else "source-over".
 * - `Dabs`: an offscreen coverage canvas over the dabs' bounds; each dab = elliptical radial gradient
 *   (white, alpha `opacity · falloff(hardness)`, rotated by `angle`, radii `radius_x`/`radius_y`) drawn
 *   source-over (= `S ← S + a(1 − S)`); if `grain_scale > 0` apply the grain pattern with "destination-in";
 *   tint with "source-in" + style color; composite onto the target at a · opacity with the style blend.
 * - `GroupBegin`/`GroupEnd`/`Mask`: offscreen group, masks stroked with "destination-out", round caps/joins.
 * - `Text`: canvas text in the corner frame, wrapped to the box width.
 */

import type { DrawList, PageEntry } from "./generated/penwise-wasm/penwise_wasm.js";

/** Floats per dab (draw list contract). */
export const DAB_STRIDE = 9;
/** Floats per style (draw list contract). */
export const STYLE_STRIDE = 8;
/** u32s per command (draw list contract). */
export const COMMAND_STRIDE = 5;
/** Mirrors proto `BACKGROUND_SPACING_PX`. */
export const BACKGROUND_SPACING_PX = 93.7;
/** Margin around content when fitting an infinite page, page px. */
export const FIT_MARGIN_PX = 64;

/** Page px → CSS px: `css = (page − origin) · scale`. */
export interface ViewTransform {
  readonly scale: number;
  readonly originX: number;
  readonly originY: number;
}

/** The three stacked canvases. */
export interface Canvases {
  readonly paper: HTMLCanvasElement;
  readonly ink: HTMLCanvasElement;
  readonly live: HTMLCanvasElement;
}

/** View that fits `page` (fixed: whole sheet; infinite: `contentBounds` + margin, or origin at 0,0 when
 * empty) into a `width × height` CSS px viewport, centred, uniform scale. */
export function fitView(
  page: PageEntry,
  contentBounds: Float32Array | undefined,
  width: number,
  height: number,
): ViewTransform {
  throw new Error("unimplemented: fitView");
}

/** Paper grain patterns per (scale, level, softness, strength), built from wasm `bake_grain_tile` +
 * `grain_coverage_lut`; cached because baking is expensive. */
export class GrainCache {
  /** Pattern for one stroke's grain, or `null` when the 2D context cannot create patterns. */
  pattern(
    context: CanvasRenderingContext2D,
    scale: number,
    level: number,
    softness: number,
    strength: number,
  ): CanvasPattern | null {
    throw new Error("unimplemented: GrainCache.pattern");
  }

  /** Drops every cached tile. */
  clear(): void {
    throw new Error("unimplemented: GrainCache.clear");
  }
}

/** Draws one page view onto the three canvases. */
export class PageRenderer {
  constructor(canvases: Canvases, grain: GrainCache) {
    throw new Error("unimplemented: PageRenderer.constructor");
  }

  /** Resizes the backing stores to `width × height` CSS px at `dpr`. */
  resize(width: number, height: number, dpr: number): void {
    throw new Error("unimplemented: PageRenderer.resize");
  }

  /** Sets the page → CSS transform for subsequent draws. */
  setView(view: ViewTransform): void {
    throw new Error("unimplemented: PageRenderer.setView");
  }

  /** Clears `paper` and draws white paper plus the page background (code 0 blank, 1 lined, 2 grid, 3 dot)
   * with `BACKGROUND_SPACING_PX`; fixed pages also get a sheet edge. */
  drawPaper(page: PageEntry): void {
    throw new Error("unimplemented: PageRenderer.drawPaper");
  }

  /** Clears `ink` and draws the committed draw list. */
  drawInk(list: DrawList): void {
    throw new Error("unimplemented: PageRenderer.drawInk");
  }

  /** Clears `live` and draws the live draw list. */
  drawLive(list: DrawList): void {
    throw new Error("unimplemented: PageRenderer.drawLive");
  }
}
