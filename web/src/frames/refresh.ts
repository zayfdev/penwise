/**
 * Which frames survive a refresh.
 *
 * Two transients must never tear a live iframe down (they reload the framed site and lose its scroll/state):
 * - a snapshot is half applied: `begin_snapshot(Scope::Page|Catalog|All)` empties items in the replica until
 *   the last chunk lands, so `Viewer.frames(page)` is briefly empty (or partial);
 * - `GET /projects` has not answered yet (or is refetching): a frame whose project is momentarily unknown
 *   still has a working iframe.
 *
 * Pure so the rules are unit-tested without a DOM.
 */

import type { Frame, Project } from "./model";

/**
 * True when the replica's frame list for `page` cannot be trusted yet: a snapshot is in progress and the
 * last complete list belongs to that same page. A page switch, or a first read, is never stale.
 */
export function framesAreStale(
  page: string | null,
  previousPage: string | null,
  previousCount: number,
  snapshotBusy: boolean,
): boolean {
  return snapshotBusy && page === previousPage && previousCount > 0;
}

/**
 * Frames the layer may render: those with a known project, plus frames already on screen whose project is
 * momentarily unknown (keep the live iframe, do not reload it). A brand-new frame without a project waits.
 */
export function renderableFrames(
  frames: readonly Frame[],
  projects: readonly Project[],
  onScreen: Iterable<string>,
): readonly Frame[] {
  const known = new Set(projects.map((project) => project.id));
  const shown = new Set(onScreen);
  return frames.filter((frame) => known.has(frame.project) || shown.has(frame.id));
}
