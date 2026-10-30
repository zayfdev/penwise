/**
 * The live side of the frames (design-harness v2): which frame holds the iframe, where its window sits, what the
 * rest of every box shows, and how scrolling and the camera follow each other.
 *
 * The rules are pure (`focus.ts`, `scroll-link.ts`, `still.ts`, `hit.ts`); this class joins them with the
 * layer, the core and the clock. Geometry itself — the live window, document points, box heights — is the core's.
 */

import { Settle, type SettleTimers } from "../camera";
import type { Viewport, ViewTransform } from "../view";
import { layoutJson } from "./bridge-messages";
import type { CoreApi } from "./core-api";
import type { LivePlan } from "./focus";
import type { WindowRect } from "./hit";
import type { FrameLayer } from "./layer";
import { LiveFocus } from "./live-focus";
import { type Frame, intersects, placeFrame } from "./model";
import { scrollTarget } from "./scroll-link";
import { type Still, stillLayoutJson } from "./still";
import { STILL_POLL_MS, type StillFetch, StillStore } from "./still-store";

/** The hidden previous frame keeps its iframe this long, ms. */
export const WARM_KEEP_MS = 60_000;
/** The live frame is mostly covered by its window, so its still is re-checked less often, ms. */
export const LIVE_POLL_MS = 10_000;

/** The part of the layer the coordinator drives. */
export type LiveLayer = Pick<
  FrameLayer,
  "liveId" | "layoutOf" | "setLive" | "placeWindow" | "renderStills" | "setStill" | "scrollTo"
>;

/** What the coordinator needs around it. */
export interface LiveDeps {
  readonly layer: LiveLayer;
  readonly core: Pick<CoreApi, "set_frame_layout" | "clear_frame_layout" | "frame_live_window">;
  /** The shown page's frames, boxes as the core computes them. */
  frames(): readonly Frame[];
  view(): ViewTransform;
  viewport(): Viewport;
  /** The server's still of a frame (`hex` is the id its routes take). */
  fetchStill: StillFetch;
  timers: SettleTimers;
  /** The core's layouts changed: read the frames (and their boxes) again. */
  reread(): void;
  /** The core's layout changed what is drawn. */
  layoutChanged(repaint: boolean): void;
  problem(message: string): void;
}

/** Focus, window, stills and scroll link of the shown page's frames. */
export class LiveCoordinator {
  private readonly focus = new LiveFocus();
  private readonly stills: StillStore;
  private readonly settled: Settle;
  private readonly scrollSynced = new Set<string>();
  private held = "";
  private warmTimer: number | null = null;
  private livePolledAt = 0;

  constructor(private readonly deps: LiveDeps) {
    this.stills = new StillStore(
      (id, etag) => deps.fetchStill(this.hexOf(id), etag),
      (id, still) => this.stillChanged(id, still),
    );
    this.settled = new Settle(() => this.settle(), deps.timers);
  }

  /** Starts re-checking the stills of the frames on screen. */
  startPolling(): void {
    window.setInterval(() => this.poll(Date.now()), STILL_POLL_MS);
  }

  /** The view or the frame set changed: evaluate again once it rests. */
  poke(): void {
    this.settled.poke();
  }

  /** The owner moved the camera by hand: an explicit focus no longer pins. */
  release(): void {
    this.focus.release();
  }

  /** Gives frames the layer built after their still arrived (a page shown again) their still. */
  restoreStills(frames: readonly Frame[]): void {
    for (const frame of frames) this.deps.layer.setStill(frame.id, this.stills.get(frame.id));
    this.deps.layer.renderStills();
  }

  /** An explicit focus: live at once. */
  focusFrame(id: string): void {
    this.apply(this.focus.focus(id));
  }

  /** A frame item is gone. */
  frameGone(id: string): void {
    this.scrollSynced.delete(id);
    this.stills.forget(id);
    this.apply(this.focus.forget(id));
  }

  /** The core's live window of the live frame (`null` when the frame has no iframe). */
  windowOf(id: string): WindowRect | null {
    if (this.deps.layer.liveId() !== id) return null;
    try {
      const rect = this.deps.core.frame_live_window(id);
      return rect.length === 4 ? [rect[0] ?? 0, rect[1] ?? 0, rect[2] ?? 0, rect[3] ?? 0] : null;
    } catch {
      return null;
    }
  }

  /** The live page reported its layout and the core has it: place the window, link the scroll. */
  layoutReported(frame: Frame): void {
    const { layer } = this.deps;
    layer.placeWindow(frame.id, this.windowOf(frame.id));
    layer.renderStills();
    if (!this.scrollSynced.has(frame.id)) {
      this.scrollSynced.add(frame.id);
      this.syncScroll();
    }
  }

  /** Asks the live page to scroll to what the camera shows (`pw:scrollTo`), when its window does not cover it. */
  syncScroll(): void {
    const { layer } = this.deps;
    const id = layer.liveId();
    const frame = this.deps.frames().find((f) => f.id === id);
    const layout = id === null ? null : layer.layoutOf(id);
    if (!frame || !layout || frame.extent !== "full") return;
    const docH = Math.max(layout.doc[1], frame.cssH);
    const target = scrollTarget(
      { x: frame.x, y: frame.y, cssH: frame.cssH, docH },
      this.deps.view(),
      this.deps.viewport(),
      layout.scroll[1],
    );
    if (target !== null) layer.scrollTo(frame.id, target);
  }

  /** Re-checks the stills on screen; the live frame, mostly under its window, only every {@link LIVE_POLL_MS}. */
  poll(now: number): void {
    if (typeof document !== "undefined" && document.hidden) return;
    const live = this.deps.layer.liveId();
    const checkLive = now - this.livePolledAt >= LIVE_POLL_MS;
    if (checkLive) this.livePolledAt = now;
    this.stills.poll(
      this.visible()
        .map((frame) => frame.id)
        .filter((id) => id !== live || checkLive),
    );
  }

  /**
   * After the camera rests: pick the live frame, fetch what is missing, and ask the live page to scroll so its
   * window covers what is on screen.
   */
  private settle(): void {
    const { deps } = this;
    this.apply(this.focus.evaluate(deps.frames(), deps.view(), deps.viewport()));
    this.stills.poll(
      this.visible()
        .filter((f) => this.stills.get(f.id) === null)
        .map((f) => f.id),
    );
    this.syncScroll();
    deps.layer.renderStills();
  }

  private apply(plan: LivePlan): void {
    const { layer } = this.deps;
    const key = `${plan.live ?? ""}|${plan.warm ?? ""}`;
    if (key === this.held) return;
    const previous = layer.liveId();
    this.held = key;
    layer.setLive(plan);
    for (const id of plan.drop) this.demote(id);
    if (previous !== null && previous !== plan.live) this.demote(previous);
    if (plan.live !== null && plan.live !== previous) this.promote(plan.live);
    this.keepWarm(plan.warm);
    layer.renderStills();
    this.deps.reread();
  }

  /** The hidden previous frame is dropped a minute after it was hidden. */
  private keepWarm(warm: string | null): void {
    const { timers } = this.deps;
    if (this.warmTimer !== null) timers.clear(this.warmTimer);
    this.warmTimer =
      warm === null
        ? null
        : timers.set(() => {
            this.warmTimer = null;
            this.apply(this.focus.dropWarm(warm));
          }, WARM_KEEP_MS);
  }

  /**
   * The frame lost its window. With a still, the still's layout replaces the live one; without one the live
   * layout stays as it is (its rects are document coordinates, so it is still right; contract §7).
   */
  private demote(id: string): void {
    this.scrollSynced.delete(id);
    const frame = this.deps.frames().find((f) => f.id === id);
    const still = this.stills.get(id);
    if (frame && still !== null) this.replaceLayout(frame, still);
  }

  /** The frame got its window: a kept iframe still has its layout, a new one reports its own. */
  private promote(id: string): void {
    const { layer } = this.deps;
    const frame = this.deps.frames().find((f) => f.id === id);
    const reported = layer.layoutOf(id);
    if (frame && reported !== null) {
      this.replaceLayout(frame, null, layoutJson(reported, frame.preset));
      this.scrollSynced.add(id);
    }
    layer.placeWindow(id, this.windowOf(id));
  }

  /**
   * Replaces — never merges into — the frame's layout in the core: the still's, a given `json`, or none. A layout
   * left over from the live page would keep its scroll, document size and rects.
   */
  private replaceLayout(frame: Frame, still: Still | null, json?: string): void {
    const { core } = this.deps;
    try {
      const had = core.clear_frame_layout(frame.id);
      const next = json ?? (still === null ? null : stillLayoutJson(still, frame.preset));
      const changed = next === null ? false : core.set_frame_layout(frame.id, next);
      this.deps.layoutChanged(had || changed);
    } catch (error) {
      this.deps.problem(`frame layout not applied: ${String(error)}`);
    }
  }

  private stillChanged(id: string, still: Still): void {
    const { layer } = this.deps;
    layer.setStill(id, still);
    const frame = this.deps.frames().find((f) => f.id === id);
    if (frame && layer.liveId() !== id) {
      this.replaceLayout(frame, still);
      this.deps.reread();
    }
    layer.renderStills();
  }

  private hexOf(id: string): string {
    return this.deps.frames().find((frame) => frame.id === id)?.hex ?? "";
  }

  /** Frames whose box is on the stage. */
  private visible(): Frame[] {
    const viewport = this.deps.viewport();
    const view = this.deps.view();
    return this.deps.frames().filter((frame) => intersects(placeFrame(frame, view), viewport));
  }
}
