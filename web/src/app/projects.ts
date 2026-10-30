/** The linked projects (`GET /projects`): the iframe URLs and the sidebar's list. */

import type { Project } from "../frames/model";

/** How often missing projects are refetched while a frame refers to one, ms. */
const RETRY_MS = 2_000;

/** The project list, refetched when a frame needs a project that is not in it. */
export class Projects {
  private projects: readonly Project[] = [];
  private fetchedAt = 0;

  constructor(
    private readonly fetchList: () => Promise<readonly Project[]>,
    /** A new list arrived. */
    private readonly changed: (projects: readonly Project[]) => void,
  ) {}

  get list(): readonly Project[] {
    return this.projects;
  }

  /** Fetches the list; a failure (not paired yet, no projects) keeps the last one. */
  async refresh(): Promise<void> {
    this.fetchedAt = Date.now();
    try {
      this.projects = await this.fetchList();
    } catch {
      return;
    }
    this.changed(this.projects);
  }

  /** Refetches when a frame names a project the list lacks (at most every {@link RETRY_MS}). */
  ensure(frames: readonly { readonly project: string }[]): void {
    const missing = frames.some((f) => !this.projects.some((project) => project.id === f.project));
    if (missing && Date.now() - this.fetchedAt > RETRY_MS) void this.refresh();
  }
}
