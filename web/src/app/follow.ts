/**
 * What a received frame means for the screen, as data: which layers to repaint and whether the shown page
 * should change. Pure so it is unit-tested without a DOM.
 */

import { MessageKind, type Received } from "../wasm";

/** Who drives page switches. `follow`: the tablet moves the browser; `free`: nobody; `lead`: the browser also moves the tablet. */
export type FollowMode = "follow" | "free" | "lead";

/** The slice of {@link Received} the plan reads. */
export type ReceivedView = Pick<
  Received,
  "kind" | "page" | "changed_pages" | "live_pages" | "catalog_changed" | "scope"
>;

/** Reaction to one frame. */
export interface Plan {
  readonly repaintCommitted: boolean;
  readonly repaintLive: boolean;
  readonly refreshCatalog: boolean;
  /** Page the server asked us to show (`Welcome.current_page` first time, `SwitchPage` in follow/lead mode). */
  readonly switchTo: string | null;
  /** The catalog snapshot finished: pick/watch a page now. */
  readonly catalogReady: boolean;
}

/** Tracks which snapshot is running so `SnapshotEnd` can be attributed. */
export class SnapshotTracker {
  private scope: string | null = null;

  /** A snapshot has begun and not ended: the replica is half replaced, so nothing should be painted from it. */
  get inProgress(): boolean {
    return this.scope !== null;
  }

  /** Feed every frame; returns the scope of a snapshot that just ended (`null` otherwise). */
  observe(received: Pick<Received, "kind" | "scope">): string | null {
    if (received.kind === MessageKind.SnapshotBegin) this.scope = received.scope ?? "unknown";
    if (received.kind !== MessageKind.SnapshotEnd) return null;
    const ended = this.scope;
    this.scope = null;
    return ended;
  }
}

/**
 * Plan for `received` while `shown` is on screen.
 * `endedScope` and `snapshotBusy` come from {@link SnapshotTracker}: while a snapshot runs, its half-applied
 * state is not painted; its end repaints the shown page (and refreshes the lists for catalog snapshots).
 */
export function planReceived(
  received: ReceivedView,
  shown: string | null,
  mode: FollowMode,
  endedScope: string | null,
  snapshotBusy: boolean,
): Plan {
  const touches = (pages: readonly string[]) => shown !== null && pages.includes(shown);
  let switchTo: string | null = null;
  if (received.kind === MessageKind.SwitchPage && mode !== "free" && received.page) {
    switchTo = received.page;
  }
  if (received.kind === MessageKind.Welcome && shown === null && received.page) {
    switchTo = received.page;
  }
  const catalogEnded = endedScope === "catalog" || endedScope === "all";
  return {
    repaintCommitted:
      (touches(received.changed_pages) && !snapshotBusy) ||
      catalogEnded ||
      (endedScope !== null && endedScope === shown),
    repaintLive: touches(received.live_pages),
    refreshCatalog: (received.catalog_changed && !snapshotBusy) || catalogEnded,
    switchTo,
    catalogReady: catalogEnded,
  };
}
