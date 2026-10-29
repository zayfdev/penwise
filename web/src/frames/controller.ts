/**
 * Frames and marks of the shown page (contracts K1–K3): iframes, bridge layouts, stroke bindings and mark chips.
 *
 * The core is the single source of truth: this controller feeds every bridge layout into `set_frame_layout`,
 * asks the core which element keys to watch, names the frame/key of a stroke started over a frame, and draws
 * the chips the core reports. It never computes where ink lands.
 */

import { toPage, type Viewport, type ViewTransform } from "../view";
import type { Viewer } from "../wasm";
import {
  type BindingMode,
  type Layout,
  layoutJson,
  type Rect,
  type StrokeBinding,
} from "./bridge-messages";
import type { CoreApi } from "./core-api";
import { FrameLayer, type FrameLayerEnv } from "./layer";
import { type Mark, MarkLayer, type MarkStatus, parseMarks } from "./marks";
import {
  type Frame,
  frameAt,
  normalizeRoute,
  type Project,
  parseFrames,
  presetSize,
} from "./model";

/** Inset of a frame added from the sidebar, CSS px from the visible top-left. */
const FRAME_INSET_PX = 32;

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
  /** Flushes the outbox after an edit (frame items, mark status). */
  flush(): void;
  /** The committed scene may have changed: repaint it. */
  repaint(): void;
  /** An edit landed: repaint, refresh history/status and the chips. */
  changed(): void;
  problem(message: string): void;
}

/** Iframes, bridge data, bindings and mark chips of the shown page. */
export class FrameController {
  private readonly layer: FrameLayer;
  private readonly markLayer: MarkLayer;
  private frames: readonly Frame[] = [];
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
    this.layer = new FrameLayer(framesRoot, chipsRoot, env, {
      preset: (frame, preset) => this.setPreset(frame, preset),
      route: (frame, route) => this.setRoute(frame, route),
      layout: (frame, layout) => this.layout(frame, layout),
    });
    this.markLayer = new MarkLayer(
      marksRoot,
      {
        status: (id, status) => this.setStatus(id, status),
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

  /** The page state recipe of each shown frame (a mark whose state differs is dimmed). */
  get states(): readonly string[] {
    return this.frames.map((frame) => this.layer.stateOf(frame.id));
  }

  /** Re-reads the page's frames and marks, then tells each bridge what to watch. */
  refresh(): readonly Frame[] {
    const page = this.deps.page();
    this.frames = page === null ? [] : parseFrames(this.deps.viewer.frames(page));
    this.layer.render(this.frames, this.deps.projects());
    this.refreshMarks();
    for (const frame of this.frames) this.layer.watch(frame.id, this.watchKeys(frame.id));
    return this.frames;
  }

  /** Follows the canvas view; chips and mark chips move with it. */
  setView(view: ViewTransform, viewport: Viewport): void {
    this.layer.setView(view);
    this.markLayer.setView(view, viewport);
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

  /**
   * The binding of a stroke starting at a client point: the frame under it and, in bound mode, the element
   * under the point. The bridge's answer arrives while the stroke is drawn; without one the core pins it to
   * the area (contract K1).
   */
  bindingAt(clientX: number, clientY: number): StrokeBinding | null {
    const origin = this.deps.origin();
    const [x, y] = toPage(this.deps.view(), clientX - origin.left, clientY - origin.top);
    const frame = frameAt(this.frames, x, y);
    if (frame === null) return null;
    const mode = this.mode;
    let key: string | null = null;
    if (mode === "bound") {
      this.layer.hit(frame.id, x - frame.x, y - frame.y, (hit) => {
        key = hit.key;
        // The core converts the samples to element-local coordinates at commit: it needs the rect first.
        if (hit.key !== null && hit.rect !== null) this.mergeHitRect(frame, hit.key, hit.rect);
      });
    }
    return { frame: frame.id, mode, key: () => key };
  }

  /** Puts the hit element's rect into the core's layout, so an immediate bound commit places the stroke. */
  private mergeHitRect(frame: Frame, key: string, rect: Rect): void {
    const layout = this.layer.mergeRects(frame.id, { [key]: rect });
    if (layout === null) return;
    try {
      if (this.deps.core.set_frame_layout(frame.id, layoutJson(layout, frame.preset))) {
        this.deps.repaint();
      }
    } catch (error) {
      this.deps.problem(`element rect not applied: ${String(error)}`);
    }
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
    const page = this.deps.page();
    if (page === null) {
      this.deps.problem("Open a page before adding a frame.");
      return;
    }
    const size = presetSize(preset) ?? presetSize("desktop");
    if (size === null) return;
    const [x, y] = toPage(this.deps.view(), FRAME_INSET_PX, FRAME_INSET_PX);
    const json = JSON.stringify({
      project,
      route: normalizeRoute(route),
      preset,
      cssW: size.cssW,
      cssH: size.cssH,
      x: Math.round(x),
      y: Math.round(y),
    });
    try {
      this.deps.viewer.add_frame(page, json, Date.now());
    } catch (error) {
      this.deps.problem(`frame not added: ${String(error)}`);
      return;
    }
    this.edit();
  }

  private setPreset(frame: Frame, preset: string): void {
    const size = presetSize(preset) ?? { cssW: frame.cssW, cssH: frame.cssH };
    try {
      this.deps.viewer.set_frame_preset(frame.id, preset, size.cssW, size.cssH, Date.now());
    } catch (error) {
      this.deps.problem(`preset not changed: ${String(error)}`);
      return;
    }
    this.edit();
  }

  private setRoute(frame: Frame, route: string): void {
    try {
      this.deps.viewer.set_frame_route(frame.id, normalizeRoute(route), Date.now());
    } catch (error) {
      this.deps.problem(`route not changed: ${String(error)}`);
      return;
    }
    this.edit();
  }

  private setStatus(id: string, status: MarkStatus): void {
    try {
      this.deps.core.set_mark_status(id, status, Date.now());
    } catch (error) {
      this.deps.problem(`mark not updated: ${String(error)}`);
      return;
    }
    this.edit();
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

  /** A frame or mark edit: send it, then repaint the frames and chips. */
  private edit(): void {
    this.deps.flush();
    this.deps.changed();
  }
}
