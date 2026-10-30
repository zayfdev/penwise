/**
 * Which frame is live (design-harness v2 contract §3). Pure: the layer owns the iframes, this decides who holds
 * one.
 *
 * Focus rule: the frame whose box covers the most of the stage, once its width on screen reaches 60 % of the
 * stage width. Hysteresis keeps a frame focused until it falls under 50 % or another frame covers a quarter
 * more of the stage. An explicit focus (a `Focus` message, a double click) is pinned and wins while its frame
 * stays at least half the stage wide, until the owner moves the camera by hand. At most one frame is live;
 * the one before it is kept hidden for a fast return (and dropped after a minute), older ones lose their
 * iframe.
 */

import type { Viewport, ViewTransform } from "../view";

/** A frame's box in page units. */
export interface FocusBox {
  readonly id: string;
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

/** A frame must be this wide (fraction of the stage) to become focused. */
export const FOCUS_ENTER_WIDTH = 0.6;
/** A focused frame stays focused down to this width. */
export const FOCUS_LEAVE_WIDTH = 0.5;
/** A challenger must cover this many times the focused frame's area to take over. */
export const FOCUS_TAKEOVER = 1.25;

/** What the rule reads. */
export interface FocusInput {
  readonly boxes: readonly FocusBox[];
  readonly view: ViewTransform;
  readonly viewport: Viewport;
  /** The frame focused now, if any. */
  readonly current: string | null;
  /** The frame an explicit focus chose, if any. */
  readonly pinned: string | null;
}

interface Measured {
  readonly id: string;
  /** Width on screen as a fraction of the stage width. */
  readonly widthRatio: number;
  /** Stage area the box covers, CSS px². */
  readonly covered: number;
}

function measure(box: FocusBox, view: ViewTransform, viewport: Viewport): Measured {
  const left = (box.x - view.originX) * view.scale;
  const top = (box.y - view.originY) * view.scale;
  const width = box.w * view.scale;
  const height = box.h * view.scale;
  const overlapW = Math.min(left + width, viewport.width) - Math.max(left, 0);
  const overlapH = Math.min(top + height, viewport.height) - Math.max(top, 0);
  return {
    id: box.id,
    widthRatio: width / viewport.width,
    covered: overlapW > 0 && overlapH > 0 ? overlapW * overlapH : 0,
  };
}

/** The frame that should be focused, or `null` (board view: nothing is wide enough). */
export function pickFocus(input: FocusInput): string | null {
  const { viewport, view, current, pinned } = input;
  if (viewport.width <= 0 || viewport.height <= 0) return current;
  const measured = input.boxes.map((box) => measure(box, view, viewport));
  const keepPinned = measured.find(
    (m) => m.id === pinned && m.covered > 0 && m.widthRatio >= FOCUS_LEAVE_WIDTH,
  );
  if (keepPinned) return keepPinned.id;
  let best: Measured | null = null;
  let incumbent: Measured | null = null;
  for (const m of measured) {
    if (m.covered <= 0) continue;
    const held = m.id === current;
    if (m.widthRatio < (held ? FOCUS_LEAVE_WIDTH : FOCUS_ENTER_WIDTH)) continue;
    if (held) incumbent = m;
    // Later frames paint on top, so on a tie the later one wins.
    if (best === null || m.covered >= best.covered) best = m;
  }
  if (incumbent !== null && best !== null && best.id !== incumbent.id) {
    return best.covered >= incumbent.covered * FOCUS_TAKEOVER ? best.id : incumbent.id;
  }
  return best?.id ?? null;
}

/** Who holds an iframe after a focus change. */
export interface LivePlan {
  /** The frame with a visible, live iframe. */
  readonly live: string | null;
  /** The previous live frame: its iframe is kept hidden for a fast return. */
  readonly warm: string | null;
  /** Frames that had an iframe and lose it. */
  readonly drop: readonly string[];
}

/** The live/warm pair: one live iframe, the previous one kept hidden. */
export class LiveSet {
  private live: string | null = null;
  private warm: string | null = null;

  /** The frame that is live now. */
  get current(): string | null {
    return this.live;
  }

  /** The focus is now `focused` (`null` = board view); returns who holds an iframe afterwards. */
  advance(focused: string | null): LivePlan {
    const before = [this.live, this.warm];
    if (focused !== this.live && (focused !== null || this.live !== null)) {
      if (focused === this.warm) {
        [this.live, this.warm] = [focused, this.live];
      } else {
        [this.live, this.warm] = [focused, this.live ?? this.warm];
      }
    }
    return this.plan(before);
  }

  /** The hidden previous frame has been kept long enough: it loses its iframe (no-op if `id` is not it). */
  dropWarm(id: string): LivePlan {
    const before = [this.live, this.warm];
    if (this.warm === id) this.warm = null;
    return this.plan(before);
  }

  /** The frame item is gone: nothing keeps an iframe for it. */
  forget(id: string): LivePlan {
    const before = [this.live, this.warm];
    if (this.live === id) this.live = null;
    if (this.warm === id) this.warm = null;
    return this.plan(before);
  }

  private plan(before: readonly (string | null)[]): LivePlan {
    const drop = before.filter(
      (id): id is string => id !== null && id !== this.live && id !== this.warm,
    );
    return { live: this.live, warm: this.warm, drop };
  }
}
