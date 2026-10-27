/**
 * Page ↔ screen mapping. `css = (page − origin) · scale`; the backing store is `css · dpr`.
 */

import type { PageEntry } from "./wasm";

/** Margin around content when fitting an infinite page, page px. */
export const FIT_MARGIN_PX = 64;
/** Zoom limits relative to a 1:1 page px = CSS px view. */
export const MIN_SCALE = 0.05;
export const MAX_SCALE = 8;

/** Page px → CSS px. */
export interface ViewTransform {
  readonly scale: number;
  readonly originX: number;
  readonly originY: number;
}

/** Size of the stage in CSS px. */
export interface Viewport {
  readonly width: number;
  readonly height: number;
}

/** Page point under a CSS-px point. */
export function toPage(view: ViewTransform, cssX: number, cssY: number): [number, number] {
  return [view.originX + cssX / view.scale, view.originY + cssY / view.scale];
}

/**
 * View that fits the page into `viewport`, centred, uniform scale. Fixed pages fit the whole sheet; infinite
 * pages fit `bounds` (`[minX, minY, maxX, maxY]`) plus a margin, or sit at the origin at 1:1 when empty.
 */
export function fitView(
  page: Pick<PageEntry, "fixed" | "width" | "height">,
  bounds: ArrayLike<number> | null | undefined,
  viewport: Viewport,
): ViewTransform {
  let minX = 0;
  let minY = 0;
  let width = page.width;
  let height = page.height;
  if (!page.fixed) {
    if (bounds && bounds.length === 4) {
      minX = (bounds[0] ?? 0) - FIT_MARGIN_PX;
      minY = (bounds[1] ?? 0) - FIT_MARGIN_PX;
      width = (bounds[2] ?? 0) - (bounds[0] ?? 0) + 2 * FIT_MARGIN_PX;
      height = (bounds[3] ?? 0) - (bounds[1] ?? 0) + 2 * FIT_MARGIN_PX;
    } else {
      return { scale: 1, originX: -viewport.width / 2, originY: -viewport.height / 2 };
    }
  }
  if (width <= 0 || height <= 0 || viewport.width <= 0 || viewport.height <= 0) {
    return { scale: 1, originX: 0, originY: 0 };
  }
  const scale = clampScale(Math.min(viewport.width / width, viewport.height / height));
  return {
    scale,
    originX: minX + width / 2 - viewport.width / 2 / scale,
    originY: minY + height / 2 - viewport.height / 2 / scale,
  };
}

/** Zooms by `factor` keeping the page point under (`cssX`, `cssY`) fixed. */
export function zoomAt(
  view: ViewTransform,
  factor: number,
  cssX: number,
  cssY: number,
): ViewTransform {
  const scale = clampScale(view.scale * factor);
  const [px, py] = toPage(view, cssX, cssY);
  return { scale, originX: px - cssX / scale, originY: py - cssY / scale };
}

/** Pans by a CSS-px delta (content follows the pointer). */
export function panBy(view: ViewTransform, dxCss: number, dyCss: number): ViewTransform {
  return {
    scale: view.scale,
    originX: view.originX - dxCss / view.scale,
    originY: view.originY - dyCss / view.scale,
  };
}

function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}
