/** Notebook/page lists from the viewer's catalog and the page-selection rules. */

import type { NotebookEntry, PageEntry, Viewer } from "../wasm";

/** One notebook with its pages, as the sidebar renders it. */
export interface NotebookGroup {
  readonly notebook: NotebookEntry;
  readonly pages: readonly PageEntry[];
}

/** The viewer's live catalog. */
export function readCatalog(viewer: Pick<Viewer, "notebooks" | "pages">): NotebookGroup[] {
  return viewer.notebooks().map((notebook) => ({ notebook, pages: viewer.pages(notebook.id) }));
}

/** The page entry with `id`, if any group has it. */
export function findPage(groups: readonly NotebookGroup[], id: string | null): PageEntry | null {
  if (id === null) return null;
  for (const group of groups) {
    const page = group.pages.find((p) => p.id === id);
    if (page) return page;
  }
  return null;
}

/**
 * Page to show after a catalog change: `current` if still live, else `hint` (the server's current page), else
 * the first page of the first notebook that has one, else `null`.
 */
export function choosePage(
  groups: readonly NotebookGroup[],
  current: string | null,
  hint: string | null,
): string | null {
  if (findPage(groups, current)) return current;
  if (findPage(groups, hint)) return hint;
  for (const group of groups) {
    const first = group.pages[0];
    if (first) return first.id;
  }
  return null;
}
