/** Read-only state for the browser e2e (paint counters prove live frames skip the committed layer). */

import type { PaintStats } from "../render/pageview";
import type { ViewTransform } from "../view";

/** What `window.__penwise.state()` returns. */
export interface ProbeState {
  readonly page: string | null;
  readonly view: ViewTransform;
  readonly connection: string;
  readonly pending: number;
  readonly head: string;
}

/** Defines `window.__penwise` (non-writable). */
export function exposeProbe(stats: PaintStats, state: () => ProbeState): void {
  Object.defineProperty(window, "__penwise", { value: { stats, state } });
}
