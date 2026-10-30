/**
 * The stills the client has fetched, one per frame. A still is re-checked with `If-None-Match`, so an
 * unchanged document costs a 304. Network access is injected (`Api.still`), so tests use a fake.
 */

import type { Still } from "./still";

/** How often visible frames are re-checked, ms. */
export const STILL_POLL_MS = 3_000;

/** The server's answer to `GET /frames/{id}/still`. */
export type StillReply =
  | { readonly kind: "none" }
  | { readonly kind: "unchanged" }
  /** `503` + `Retry-After`: a capture is under way or failed; do not ask this frame again for `afterMs`. */
  | { readonly kind: "retry"; readonly afterMs: number }
  | { readonly kind: "fresh"; readonly still: Still; readonly etag: string | null };

/** Fetches one frame's still; `etag` is sent as `If-None-Match`. Rejects on a network failure. */
export type StillFetch = (frameId: string, etag: string | null) => Promise<StillReply>;

interface Entry {
  still: Still | null;
  etag: string | null;
  busy: boolean;
  /** Not before this time (ms) is the frame asked again. */
  retryAt: number;
}

/** Stills by frame id. */
export class StillStore {
  private readonly entries = new Map<string, Entry>();

  constructor(
    private readonly fetchStill: StillFetch,
    /** A frame's still arrived or changed. */
    private readonly changed: (frameId: string, still: Still) => void,
    private readonly now: () => number = () => Date.now(),
  ) {}

  /** The latest still of `frameId`, or `null` before the server made one. */
  get(frameId: string): Still | null {
    return this.entries.get(frameId)?.still ?? null;
  }

  /** Re-checks each of `frameIds`; a check still in flight is not repeated. */
  poll(frameIds: Iterable<string>): void {
    for (const id of frameIds) void this.check(id);
  }

  /** Forgets a deleted frame. */
  forget(frameId: string): void {
    this.entries.delete(frameId);
  }

  /** Checks one frame; resolves once the answer is applied (a failure is swallowed: the next poll retries). */
  async check(frameId: string): Promise<void> {
    let entry = this.entries.get(frameId);
    if (entry === undefined) {
      entry = { still: null, etag: null, busy: false, retryAt: 0 };
      this.entries.set(frameId, entry);
    }
    if (entry.busy || this.now() < entry.retryAt) return;
    entry.busy = true;
    try {
      const reply = await this.fetchStill(frameId, entry.etag);
      if (this.entries.get(frameId) !== entry) return;
      // The old still stays on screen through a backoff.
      if (reply.kind === "retry") entry.retryAt = this.now() + reply.afterMs;
      if (reply.kind !== "fresh") return;
      entry.still = reply.still;
      entry.etag = reply.etag;
      this.changed(frameId, reply.still);
    } catch {
      /* offline or the server is restarting: keep what is shown */
    } finally {
      entry.busy = false;
    }
  }
}
