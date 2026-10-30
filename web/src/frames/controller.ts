/**
 * Frames and marks of the shown page (contracts K1–K3): iframes, bridge layouts, stroke bindings and mark chips.
 *
 * The core is the single source of truth: this controller feeds every bridge layout into `set_frame_layout`,
 * asks the core which element keys to watch, names the frame/key of a stroke started over a frame, and draws
 * the chips the core reports. It never computes where ink lands.
 */

import { type FitMode, type PageRect, toPage, type Viewport, type ViewTransform } from "../view";
import type { Viewer } from "../wasm";
import { Binder } from "./binding";
import {
  type BindingMode,
  type Hit,
  type Layout,
  layoutJson,
  type StrokeBinding,
} from "./bridge-messages";
import type { CoreApi } from "./core-api";
import { FrameEdits } from "./edits";
import type { WindowRect } from "./hit";
import { FrameLayer, type FrameLayerEnv } from "./layer";
import { LiveCoordinator } from "./live";
import { type Mark, MarkLayer, parseMarks } from "./marks";
import { type Frame, frameAt, type Project, parseFrames } from "./model";
import { framesAreStale } from "./refresh";
import type { StillFetch } from "./still-store";

/** What the controller needs from the app. */
export interface FrameDeps {
  readonly viewer: Viewer;
  /** The M2 part of the wasm viewer (`set_frame_layout`, `marks`, …). */
  readonly core: CoreApi;
  page(): string | null;
  view(): ViewTransform;
  /** Stage origin in client px. */
  origin(): { left: number; top: number };
  projects(): readonly Project[];
  /** A sync snapshot is half applied: the replica's items (and so its frames) are incomplete. */
  snapshotBusy(): boolean;
  /** Flushes the outbox after an edit (frame items, mark status). */
  flush(): void;
  /** The committed scene may have changed: repaint it. */
  repaint(): void;
  /** An edit landed: repaint, refresh history/status and the chips. */
  changed(): void;
  problem(message: string): void;
  /** Glides the camera to `rect` (`fitRect`). */
  fit(rect: PageRect, mode: FitMode): void;
  /** The live page scrolled by itself: pans the camera by `dy` page px so its window stays on screen. */
  follow(dy: number): void;
  /** The server's still of a frame (`GET /frames/{hex}/still`; the argument is the frame's `hex`). */
  fetchStill: StillFetch;
  /** The server's hit for a document point of a frame (`GET /frames/{hex}/hit`; the argument is `hex`). */
  frameHit(hex: string, x: number, y: number): Promise<Hit>;
  /** The owner focused `frame` by hand (double click): lead mode tells the tablet. */
  focused(frame: Frame): void;
}

/** Iframes, bridge data, bindings and mark chips of the shown page. */
export class FrameController {
  private readonly layer: FrameLayer;
  private readonly markLayer: MarkLayer;
  /** The page's frames as the core reports them (boxes included). */
  private frames: readonly Frame[] = [];
  private readonly live: LiveCoordinator;
  private readonly edits: FrameEdits;
  private readonly binder: Binder;
  private view: ViewTransform = { scale: 1, originX: 0, originY: 0 };
  private viewport: Viewport = { width: 1, height: 1 };
  /** The page `frames` was read from (a snapshot is only stale for that page). */
  private framesPage: string | null = null;
  private marks: readonly Mark[] = [];
  private mode: BindingMode = "bound";
  private showDone = false;

  constructor(
    framesRoot: HTMLElement,
    chipsRoot: HTMLElement,
    marksRoot: HTMLElement,
    env: FrameLayerEnv,
    private readonly deps: FrameDeps,
  ) {
    this.edits = new FrameEdits(deps);
    this.layer = new FrameLayer(framesRoot, chipsRoot, env, {
      preset: (frame, preset) => this.edits.setPreset(frame, preset),
      route: (frame, route) => this.edits.setRoute(frame, route),
      layout: (frame, layout) => this.layout(frame, layout),
      scrolled: (_frame, dy) => this.deps.follow(dy),
      choose: (frame) => this.edits.setChosen(frame),
      patched: (frame, ok, failed) => this.edits.patched(frame, ok, failed),
    });
    this.live = new LiveCoordinator({
      layer: this.layer,
      core: deps.core,
      frames: () => this.frames,
      view: () => this.view,
      viewport: () => this.viewport,
      fetchStill: deps.fetchStill,
      timers: {
        set: (callback, ms) => window.setTimeout(callback, ms),
        clear: (handle) => window.clearTimeout(handle),
      },
      reread: () => this.reread(),
      layoutChanged: (repaint) => {
        this.refreshMarks();
        if (repaint) this.deps.repaint();
      },
      problem: (message) => this.deps.problem(message),
    });
    this.live.startPolling();
    this.binder = new Binder({
      layer: this.layer,
      core: deps.core,
      live: this.live,
      frames: () => this.frames,
      view: () => this.view,
      origin: () => deps.origin(),
      mode: () => this.mode,
      frameHit: (hex, x, y) => deps.frameHit(hex, x, y),
      repaint: () => deps.repaint(),
      problem: (message) => deps.problem(message),
    });
    this.markLayer = new MarkLayer(
      marksRoot,
      {
        status: (id, status) => this.edits.setStatus(id, status),
        replay: (frame, state) => this.layer.replay(frame, state),
      },
      (frame) => this.layer.stateOf(frame),
    );
  }

  /** The last pointer type seen on the window (frames take mouse/touch input unless a pen inks). */
  setPointer(type: string): void {
    this.layer.setPointer(type);
  }

  /** "Draw with mouse": the mouse inks over frames instead of clicking through them. */
  setDrawMouse(on: boolean): void {
    this.layer.setDrawMouse(on);
  }

  /** Frames of the shown page, in paint order. */
  get current(): readonly Frame[] {
    return this.frames;
  }

  /** Marks of the shown page as last read from the core. */
  get currentMarks(): readonly Mark[] {
    return this.marks;
  }

  /** Frames that have reported a layout since their last load (the bridge is live). */
  get layouts(): number {
    return this.layer.layoutCount();
  }

  /** Iframes built so far: more than one per frame item means a live site was reloaded for no reason. */
  get frameBuilds(): number {
    return this.layer.stats.built;
  }

  /** The page state recipe of each shown frame (a mark whose state differs is dimmed). */
  get states(): readonly string[] {
    return this.frames.map((frame) => this.layer.stateOf(frame.id));
  }

  /**
   * Re-reads the page's frames and marks, then tells each bridge what to watch.
   *
   * While a snapshot is half applied the replica's items are incomplete (`begin_snapshot` empties them), so
   * the same page keeps its live frames and chips: re-reading now would tear the iframes down and reload the
   * framed site. The snapshot's end triggers another refresh with the complete state.
   */
  refresh(): readonly Frame[] {
    const page = this.deps.page();
    if (framesAreStale(page, this.framesPage, this.frames.length, this.deps.snapshotBusy())) {
      return this.frames;
    }
    this.framesPage = page;
    const previous = this.frames;
    this.frames = this.read(page);
    const ids = new Set(this.frames.map((frame) => frame.id));
    for (const old of previous) if (!ids.has(old.id)) this.live.frameGone(old.id);
    this.layer.render(this.frames, this.deps.projects());
    this.live.restoreStills(this.frames);
    this.refreshMarks();
    for (const frame of this.frames) this.layer.watch(frame.id, this.watchKeys(frame.id));
    this.live.poke();
    return this.frames;
  }

  private read(page: string | null): readonly Frame[] {
    return page === null ? [] : parseFrames(this.deps.viewer.frames(page));
  }

  /**
   * The core's layouts changed (a bridge layout, a still, a demotion): its frames' boxes may have changed with
   * them, so read them again and place what moved.
   */
  private reread(): void {
    const page = this.deps.page();
    if (page !== this.framesPage) return;
    this.frames = this.read(page);
    this.layer.render(this.frames, this.deps.projects());
    this.layer.renderStills();
  }

  /** Follows the canvas view; chips and mark chips move with it. */
  setView(view: ViewTransform, viewport: Viewport): void {
    this.view = view;
    this.viewport = viewport;
    this.layer.setView(view);
    this.markLayer.setView(view, viewport);
    this.live.poke();
  }

  /** The owner panned or zoomed by hand: an explicit focus no longer pins the live frame. */
  userMoved(): void {
    this.live.release();
  }

  /**
   * A double click at a client point: the frame under it is focused (fit to the stage width, live). Only where
   * mouse input reaches frames; `false` when it hit no frame.
   */
  doubleClick(clientX: number, clientY: number): boolean {
    if (!this.layer.interactive) return false;
    const origin = this.deps.origin();
    const [x, y] = toPage(this.view, clientX - origin.left, clientY - origin.top);
    const frame = frameAt(this.frames, x, y);
    if (frame === null) return false;
    this.focusFrame(frame);
    this.deps.focused(frame);
    return true;
  }

  /** A `Focus` message: fit frame `item` to the stage and make it live. `false` when it is not a frame here. */
  focusItem(item: string): boolean {
    const frame = this.frames.find((f) => f.id === item);
    if (frame === undefined) return false;
    this.focusFrame(frame);
    return true;
  }

  private focusFrame(frame: Frame): void {
    this.live.focusFrame(frame.id);
    this.deps.fit({ x: frame.x, y: frame.y, w: frame.cssW, h: frame.boxH }, "width");
  }

  /** The frame with the live iframe, and its window in board px (probe and e2e). */
  get liveState(): { frame: string | null; window: WindowRect | null; iframes: number } {
    const frame = this.layer.liveId();
    return {
      frame,
      window: frame === null ? null : this.live.windowOf(frame),
      iframes: this.layer.iframeCount(),
    };
  }

  /** The still images in each frame's box now (probe and e2e). */
  get stillUrls(): Record<string, readonly string[]> {
    return Object.fromEntries(
      this.frames.map((frame) => [frame.id, this.layer.stillUrls(frame.id)]),
    );
  }

  /** "Bind to element" (default) or "Pin to area" for strokes started over a frame. */
  setBindingMode(mode: BindingMode): void {
    this.mode = mode;
  }

  /** "Show done": done marks are hidden while this is off. */
  setShowDone(on: boolean): void {
    this.showDone = on;
    if (this.deps.core.set_show_done(on)) this.deps.repaint();
    this.refreshMarks();
  }

  /** The binding of a stroke starting at a client point: the frame under it and, in bound mode, its element. */
  bindingAt(clientX: number, clientY: number): StrokeBinding | null {
    return this.binder.bindingAt(clientX, clientY);
  }

  /** Commits the open stroke: with a binding for frame strokes, plain for canvas strokes. */
  commit(now: number, binding: string | null): void {
    if (binding === null) {
      this.deps.viewer.stroke_end(now);
      return;
    }
    this.deps.core.stroke_end_bound(now, binding);
  }

  /** Adds a frame on the shown page at the visible top-left (`add_frame`). */
  addFrame(project: string, route: string, preset: string): void {
    this.edits.addFrame(project, route, preset);
  }

  /** A bridge layout: merge it into the core and repaint only what the core says changed. */
  private layout(frame: Frame, layout: Layout): void {
    let changed = false;
    try {
      changed = this.deps.core.set_frame_layout(frame.id, layoutJson(layout, frame.preset));
    } catch (error) {
      this.deps.problem(`frame layout not applied: ${String(error)}`);
      return;
    }
    this.refreshMarks();
    if (changed) this.deps.repaint();
    this.reread();
    this.live.layoutReported(frame);
  }

  /** Reads the core's marks and redraws the chips (also after a layout or a status change). */
  private refreshMarks(): void {
    const page = this.deps.page();
    try {
      this.marks = page === null ? [] : parseMarks(this.deps.core.marks(page));
    } catch (error) {
      this.deps.problem(`marks not read: ${String(error)}`);
      this.marks = [];
    }
    this.markLayer.render(this.marks, this.showDone);
  }

  /** Element keys the core needs rects for (its bound strokes on this frame). */
  private watchKeys(frame: string): string[] {
    try {
      const keys: unknown = JSON.parse(this.deps.core.frame_keys(frame));
      return Array.isArray(keys)
        ? keys.filter((key): key is string => typeof key === "string")
        : [];
    } catch {
      return [];
    }
  }
}
