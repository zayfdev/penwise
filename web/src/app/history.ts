/** The viewer's own undo/redo as the toolbar and keys use it. */

import type { Viewer } from "../wasm";

/** Undoes/redoes on `page`; sends the emitted ops through `flush`; failures go to `problem`. */
export function stepHistory(
  viewer: Pick<Viewer, "undo" | "redo">,
  page: string,
  redo: boolean,
  flush: () => void,
  problem: (message: string) => void,
): void {
  try {
    const count = redo ? viewer.redo(page, Date.now()) : viewer.undo(page, Date.now());
    if (count > 0) flush();
  } catch (error) {
    problem(`${redo ? "redo" : "undo"} failed: ${String(error)}`);
  }
}

/** Whether undo and redo have a step on `page`. */
export function historyState(
  viewer: Pick<Viewer, "can_undo" | "can_redo">,
  page: string | null,
): [boolean, boolean] {
  return page === null ? [false, false] : [viewer.can_undo(page), viewer.can_redo(page)];
}
