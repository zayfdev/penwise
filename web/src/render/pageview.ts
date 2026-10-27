/**
 * The page on screen: two stacked canvases at `devicePixelRatio`.
 *
 * - `committed` caches paper + every committed item. It repaints only when the committed scene changed, the
 *   view moved/resized, or a picture finished loading.
 * - `live` holds the in-progress strokes (other devices' previews and this client's own pen). A live frame
 *   clears and repaints only this canvas, so a stroke in flight never re-rasterizes the page.
 *
 * Repaints are requested with `invalidate*` and coalesced into one animation frame; the draw lists are pulled
 * from the {@link PageSource} at paint time.
 */

import type { Viewport, ViewTransform } from "../view";
import type { PageEntry } from "../wasm";
import { type DrawData, isEmpty } from "./drawdata";
import type { PaperGrain } from "./grain";
import type { ImageCache } from "./images";
import { context2d, LayerPool } from "./layers";
import { viewMatrix } from "./matrix";
import { type PaintEnv, paintDrawList } from "./paint";
import { paintPaper } from "./paper";

/** What the page view draws. */
export interface PageSource {
  /** The shown page (`null`: nothing to draw). */
  page(): PageEntry | null;
  /** Committed items of the shown page. */
  committed(): DrawData;
  /** Live strokes of the shown page. */
  live(): DrawData;
}

/** Paint counters (the e2e asserts that live frames do not repaint the committed layer). */
export interface PaintStats {
  committed: number;
  live: number;
}

/** Schedules a callback for the next frame. */
export type FrameScheduler = (callback: () => void) => void;

/** Two-canvas page renderer. */
export class PageView {
  readonly stats: PaintStats = { committed: 0, live: 0 };
  private readonly committedCtx: CanvasRenderingContext2D;
  private readonly liveCtx: CanvasRenderingContext2D;
  private readonly layers = new LayerPool();
  private view: ViewTransform = { scale: 1, originX: 0, originY: 0 };
  private dpr = 1;
  private committedDirty = true;
  private liveDirty = true;
  private scheduled = false;

  constructor(
    readonly committed: HTMLCanvasElement,
    readonly live: HTMLCanvasElement,
    private readonly source: PageSource,
    private readonly grain: PaperGrain,
    private readonly images: ImageCache,
    private readonly schedule: FrameScheduler = (cb) => requestAnimationFrame(cb),
  ) {
    this.committedCtx = context2d(committed);
    this.liveCtx = context2d(live);
  }

  /** Current page → CSS view. */
  get transform(): ViewTransform {
    return this.view;
  }

  /** Resizes both backing stores to `viewport` CSS px at `dpr` and repaints. */
  resize(viewport: Viewport, dpr: number): void {
    this.dpr = dpr;
    for (const canvas of [this.committed, this.live]) {
      canvas.width = Math.max(1, Math.round(viewport.width * dpr));
      canvas.height = Math.max(1, Math.round(viewport.height * dpr));
    }
    this.invalidateCommitted();
    this.invalidateLive();
  }

  /** Moves the view and repaints. */
  setView(view: ViewTransform): void {
    this.view = view;
    this.invalidateCommitted();
    this.invalidateLive();
  }

  /** The committed scene (or the page itself) changed. */
  invalidateCommitted(): void {
    this.committedDirty = true;
    this.requestFrame();
  }

  /** Live strokes changed. */
  invalidateLive(): void {
    this.liveDirty = true;
    this.requestFrame();
  }

  /** Paints whatever is dirty now (the scheduled frame calls this). */
  paintNow(): void {
    this.scheduled = false;
    if (this.committedDirty) this.paintCommitted();
    if (this.liveDirty) this.paintLive();
  }

  private requestFrame(): void {
    if (this.scheduled) return;
    this.scheduled = true;
    this.schedule(() => this.paintNow());
  }

  private env(): PaintEnv {
    const { scale, originX, originY } = this.view;
    return {
      grain: this.grain,
      images: this.images,
      layers: this.layers,
      base: viewMatrix(scale, originX, originY, this.dpr),
    };
  }

  private paintCommitted(): void {
    this.committedDirty = false;
    this.stats.committed++;
    const page = this.source.page();
    const env = this.env();
    if (!page) {
      this.committedCtx.clearRect(0, 0, this.committed.width, this.committed.height);
      return;
    }
    paintPaper(this.committedCtx, page, env.base);
    paintDrawList(this.committedCtx, this.source.committed(), env);
  }

  private paintLive(): void {
    this.liveDirty = false;
    this.stats.live++;
    this.liveCtx.clearRect(0, 0, this.live.width, this.live.height);
    if (!this.source.page()) return;
    const data = this.source.live();
    if (!isEmpty(data)) paintDrawList(this.liveCtx, data, this.env());
  }
}
