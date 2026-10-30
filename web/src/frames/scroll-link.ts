/**
 * The scroll link of a live frame (design-harness v2 contract §5, web side). Pure.
 *
 * The live window follows the page's own scroll, so the content inside it sits at its true board position.
 * Two directions keep the window on what the owner looks at:
 * - the page scrolls (wheel over the live iframe): the camera pans by the same amount, so the window stays on
 *   screen while the page appears fixed to the board ({@link ScrollFollower});
 * - the camera moved (pan/zoom elsewhere): once it settles the page is asked to scroll so the window covers the
 *   visible part of the frame ({@link scrollTarget}, sent as `pw:scrollTo`).
 */

import type { Viewport, ViewTransform } from "../view";
import type { ScrollCause } from "./bridge-messages";
import type { DocRange } from "./still";

/** What {@link scrollTarget} reads about a frame, board px and CSS px. */
export interface ScrollFrame {
  /** Board position of the box's top-left. */
  readonly x: number;
  readonly y: number;
  /** The preset viewport height: the live window's height. */
  readonly cssH: number;
  /** The document height, CSS px (the largest scroll is `docH - cssH`). */
  readonly docH: number;
}

/**
 * The part of a `full` frame's document on screen, or `null` when none of it is. `boxY` is the board y of the
 * box's top (document y 0), `boxH` its height.
 */
export function visibleDocRange(
  boxY: number,
  boxH: number,
  view: ViewTransform,
  viewport: Viewport,
): DocRange | null {
  const from = Math.max(0, view.originY - boxY);
  const to = Math.min(boxH, view.originY - boxY + viewport.height / view.scale);
  return to > from ? { from, to } : null;
}

/**
 * Where the live iframe sits inside its box, CSS px: the core's live window (board px, `pos + scroll`) relative
 * to the box's top-left. The page's *reported* scroll decides, never the camera's wish, so the content inside the
 * iframe is always at its true board position. Without a window (no layout yet) it sits at the box's top.
 */
export function iframeOffset(
  boxX: number,
  boxY: number,
  window: readonly number[] | null,
): { readonly left: number; readonly top: number } {
  if (window === null) return { left: 0, top: 0 };
  return {
    left: Math.round(window[0] ?? 0) - Math.round(boxX),
    top: Math.round(window[1] ?? 0) - Math.round(boxY),
  };
}

/**
 * The scroll the page should be asked for so the live window covers the visible part of the frame, or `null`
 * when it already does (no jitter) or the frame is off screen. Clamped to `0..docH - cssH`.
 */
export function scrollTarget(
  frame: ScrollFrame,
  view: ViewTransform,
  viewport: Viewport,
  scrollY: number,
): number | null {
  const visible = visibleDocRange(frame.y, frame.docH, view, viewport);
  if (visible === null) return null;
  const { from: visibleFrom, to: visibleTo } = visible;
  if (visibleFrom >= scrollY && visibleTo <= scrollY + frame.cssH) return null;
  const spare = Math.max(0, frame.cssH - (visibleTo - visibleFrom)) / 2;
  const max = Math.max(0, frame.docH - frame.cssH);
  const target = Math.min(max, Math.max(0, Math.round(visibleFrom - spare)));
  return Math.abs(target - scrollY) < 1 ? null : target;
}

/** After the page overrode a requested scroll, no new request is made for this long, ms. */
export const PAGE_WINS_MS = 1_000;

/**
 * Tells the owner's scrolling from everything else the page does, from the bridge's `cause` labels.
 *
 * - `user`: the owner scrolled (wheel, touch, keys, and the momentum after): the camera follows, so the window
 *   stays on screen.
 * - `request`: the scroll this client asked for with `pw:scrollTo {id}`; matched by id. The camera already moved.
 * - `page`: reload restore, anchors, `scrollIntoView`, smooth-scroll libraries: only the window follows (the page
 *   still sits right on the board). If the page moves away from a requested target by itself, the page wins:
 *   nothing is requested for {@link PAGE_WINS_MS}.
 */
export class ScrollFollower {
  private last: number | null = null;
  private nextId = 1;
  private pending: { readonly id: number; readonly target: number } | null = null;
  private heldUntil = 0;

  /** True while a request would fight the page (it overrode the last one a moment ago). */
  held(now: number): boolean {
    return now < this.heldUntil;
  }

  /** A `pw:scrollTo` to `target` is being sent; returns the id it carries. */
  request(target: number): number {
    const id = this.nextId++;
    this.pending = { id, target };
    return id;
  }

  /** The page was reloaded: forget its scroll. */
  reset(): void {
    this.last = null;
    this.pending = null;
    this.heldUntil = 0;
  }

  /**
   * The page reported `scrollY` with `cause` (and, for a request, its `id`) at `now`; returns how far the camera
   * should follow (page px; zero unless the owner scrolled).
   */
  reported(scrollY: number, cause: ScrollCause, id: number | null, now: number): number {
    const before = this.last;
    this.last = scrollY;
    if (before === null) return 0;
    const pending = this.pending;
    if (cause === "user") {
      this.pending = null;
      return scrollY - before;
    }
    if (cause === "request") {
      if (pending !== null && id === pending.id && Math.abs(scrollY - pending.target) < 1) {
        this.pending = null;
      }
      return 0;
    }
    if (scrollY !== before) {
      // The page moved by itself: whatever was asked of it, the page wins for a moment.
      this.pending = null;
      this.heldUntil = now + PAGE_WINS_MS;
    }
    return 0;
  }
}
