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
  /** Page state recipe of each shown frame (`""` = initial state). */
  readonly states: readonly string[];
  /** Marks of the shown page as the core reports them (`rect` is page coords, `null` when hidden). */
  readonly marks: readonly Mark[];
}

/** Defines `window.__penwise` (non-writable). */
export function exposeProbe(stats: PaintStats, state: () => ProbeState): void {
  Object.defineProperty(window, "__penwise", { value: { stats, state } });
}
