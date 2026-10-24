/**
 * Notebook / page list and page switching.
 *
 * Owner: slice d.
 *
 * Lists come from the viewer's local catalog (`Viewer.notebooks()` / `Viewer.pages(id)`), refreshed whenever
 * a `Received` has `catalog_changed`. Follow mode (default): a `SwitchPage` from another device (the tablet)
 * moves the view. Clicking a page switches locally and turns follow off; the "Follow tablet" toggle turns it
 * back on. "Lead" mode additionally sends `encode_switch_page` so the tablet follows the browser.
 * Rendering uses DOM APIs only (`textContent`, never `innerHTML`: titles are user data).
 */

import type { NotebookEntry, PageEntry } from "../generated/penwise-wasm/penwise_wasm.js";

/** Who drives page switches. */
export type FollowMode = "follow" | "free" | "lead";

/** User actions from the list. */
export interface PageListEvents {
  /** User picked a page. */
  select(pageId: string): void;
  /** User changed the follow mode. */
  mode(mode: FollowMode): void;
}

/** One notebook with its pages, as the list renders it. */
export interface NotebookGroup {
  readonly notebook: NotebookEntry;
  readonly pages: readonly PageEntry[];
}

/** Sidebar of notebooks and pages. */
export class PageList {
  constructor(root: HTMLElement, events: PageListEvents) {
    throw new Error("unimplemented: PageList.constructor");
  }

  /** Re-renders the list, marking `currentPage` and the active `mode`. */
  render(groups: readonly NotebookGroup[], currentPage: string | null, mode: FollowMode): void {
    throw new Error("unimplemented: PageList.render");
  }
}

/** Page to show after a catalog change: `current` if still live, else the first page of its notebook, else the
 * first page overall, else `null`. */
export function choosePage(groups: readonly NotebookGroup[], current: string | null): string | null {
  throw new Error("unimplemented: choosePage");
}
