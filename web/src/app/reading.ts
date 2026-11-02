/** Which frame the window reads: the tablet's while following, else the top one. */

import type { Frame } from "../frames/model";
import { readableFrame } from "../frames/readable";
import type { FollowMode } from "./follow";

/** Remembers the tablet's last `Focus` and picks the frame to read. */
export class Reading {
  private tablet: { readonly page: string; readonly item: string } | null = null;

  /** The tablet focused `item` on `page`. */
  tabletFocused(page: string, item: string): void {
    this.tablet = { page, item };
  }

  /** The frame of `frames` (the shown page's) to read in `mode`; `null` on a page without frames. */
  frame(frames: readonly Frame[], page: string | null, mode: FollowMode): Frame | null {
    const item = mode !== "free" && this.tablet?.page === page ? this.tablet.item : null;
    return readableFrame(frames, item);
  }

  /** Like {@link frame}, but the live frame (`liveId`) comes first: what "Fit" fits. */
  current(
    frames: readonly Frame[],
    liveId: string | null,
    page: string | null,
    mode: FollowMode,
  ): Frame | null {
    return frames.find((f) => f.id === liveId) ?? this.frame(frames, page, mode);
  }
}
