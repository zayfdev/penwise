/** Which frame the PC window shows by default, and the view that makes it readable (width fitted, top first). */

import { fitRect, type Viewport, type ViewTransform } from "../view";
import type { Frame } from "./model";

/**
 * The frame to read: the tablet's (`tabletItem`) when it is among `frames`, else the top-left one (smallest
 * `y`, then `x`). `null` when the page has no frames.
 */
export function readableFrame(frames: readonly Frame[], tabletItem: string | null): Frame | null {
  const tablet = frames.find((frame) => frame.id === tabletItem);
  if (tablet !== undefined) return tablet;
  let top: Frame | null = null;
  for (const frame of frames) {
    if (top === null || frame.y < top.y || (frame.y === top.y && frame.x < top.x)) top = frame;
  }
  return top;
}

/** The frame's width fills the stage and its top is at the top of the stage; the wheel scrolls down it. */
export function readableView(frame: Frame, viewport: Viewport): ViewTransform {
  const rect = { x: frame.x, y: frame.y, w: frame.cssW, h: frame.boxH };
  return fitRect(rect, "width", viewport, { scale: 1, originX: rect.x, originY: rect.y });
}
