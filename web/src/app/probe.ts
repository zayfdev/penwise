/** Read-only state for the browser e2e (paint counters prove live frames skip the committed layer). */

import type { Mark } from "../frames/marks";
import type { PaintStats } from "../render/pageview";
import type { ViewTransform } from "../view";

/** What `window.__penwise.state()` returns. */
export interface ProbeState {
  readonly page: string | null;
  readonly view: ViewTransform;
  readonly connection: string;
  readonly pending: number;
  readonly head: string;
  /** Frames whose bridge has reported a layout (0 before the injected script is live). */
  readonly layouts: number;
  /** Iframes built so far: more than the number of frames means a live site was reloaded for no reason. */
  readonly frameBuilds: number;
  /** Page state recipe of each shown frame (`""` = initial state). */
  readonly states: readonly string[];
  /** Marks of the shown page as the core reports them (`rect` is page coords, `null` when hidden). */
  readonly marks: readonly Mark[];
  /** The frame with the live iframe, its window in board px, and how many iframes exist (live + warm). */
  readonly live: {
    readonly frame: string | null;
    readonly window: readonly [number, number, number, number] | null;
    readonly iframes: number;
  };
  /** The still strip images in each frame's box. */
  readonly stills: Readonly<Record<string, readonly string[]>>;
}

/** Defines `window.__penwise` (non-writable). */
export function exposeProbe(stats: PaintStats, state: () => ProbeState): void {
  Object.defineProperty(window, "__penwise", { value: { stats, state } });
}
