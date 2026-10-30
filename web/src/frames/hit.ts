/**
 * Where a pen-down asks for the element under it (design-harness v2 contract §5). Pure.
 *
 * Inside the live window the framed page's bridge answers (`pw:hit?`, viewport px); anywhere else on the frame
 * the server answers from its own headless page (`GET /frames/{id}/hit`, document px).
 */

import type { Rect } from "./bridge-messages";

/** `[x, y, w, h]` in board px, as `Viewer.frame_live_window` reports. */
export type WindowRect = readonly [number, number, number, number];

/** How a hit is asked for. */
export type HitRoute =
  | { readonly via: "bridge"; readonly x: number; readonly y: number }
  | { readonly via: "server"; readonly x: number; readonly y: number };

/**
 * The route for a pen-down at board point `board`, whose document point is `doc`. `window` is the live
 * window (`null` when the frame has no live iframe).
 */
export function routeHit(
  window: WindowRect | null,
  board: readonly [number, number],
  doc: readonly [number, number],
): HitRoute {
  if (window !== null) {
    const [wx, wy, ww, wh] = window;
    const inside = board[0] >= wx && board[0] < wx + ww && board[1] >= wy && board[1] < wy + wh;
    if (inside) return { via: "bridge", x: board[0] - wx, y: board[1] - wy };
  }
  return { via: "server", x: doc[0], y: doc[1] };
}

/** A document rect as the viewport rect the core's layout merge expects (it adds the scroll back). */
export function viewportRect(docRect: Rect, scroll: readonly [number, number]): Rect {
  return [docRect[0] - scroll[0], docRect[1] - scroll[1], docRect[2], docRect[3]];
}
