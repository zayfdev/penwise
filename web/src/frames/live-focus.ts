/**
 * Who is live right now: the focus rule's pick (or an explicit focus) run through the live/warm pair.
 * The rule and the pair are in `focus.ts`; this joins them with frames and the camera.
 */

import type { Viewport, ViewTransform } from "../view";
import { type FocusBox, type LivePlan, LiveSet, pickFocus } from "./focus";
import type { Frame } from "./model";

/** The box of a frame for the focus rule: page units, `cssW` wide and `boxH` tall. */
export function focusBox(frame: Frame): FocusBox {
  return { id: frame.id, x: frame.x, y: frame.y, w: frame.cssW, h: frame.boxH };
}

/** The live/warm state of one page's frames. */
export class LiveFocus {
  private readonly live = new LiveSet();
  private pinned: string | null = null;

  /** The frame that is live now. */
  get current(): string | null {
    return this.live.current;
  }

  /** Re-runs the focus rule for the camera at rest. */
  evaluate(frames: readonly Frame[], view: ViewTransform, viewport: Viewport): LivePlan {
    const focused = pickFocus({
      boxes: frames.map(focusBox),
      view,
      viewport,
      current: this.live.current,
      pinned: this.pinned,
    });
    return this.live.advance(focused);
  }

  /**
   * An explicit focus (a `Focus` message, a double click): live at once, and it wins until the owner moves
   * the camera by hand.
   */
  focus(id: string): LivePlan {
    this.pinned = id;
    return this.live.advance(id);
  }

  /** The owner pans or zooms by hand: the rule decides again. */
  release(): void {
    this.pinned = null;
  }

  /** The hidden previous frame has been kept long enough. */
  dropWarm(id: string): LivePlan {
    return this.live.dropWarm(id);
  }

  /** The frame item is gone. */
  forget(id: string): LivePlan {
    if (this.pinned === id) this.pinned = null;
    return this.live.forget(id);
  }
}
