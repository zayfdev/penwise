/**
 * The binding of a stroke started over a frame (contract K1, V2 §5): which frame, and — in bound mode — the
 * element under the pen-down. Inside the live window the framed page's bridge answers; anywhere else on the
 * frame the server does, from its own copy of the page. Either way the answer arrives while the stroke is
 * drawn; a stroke committed before it falls back to an area binding.
 */

import { toPage, type ViewTransform } from "../view";
import {
  type BindingMode,
  type Hit,
  layoutJson,
  type Rect,
  type StrokeBinding,
} from "./bridge-messages";
import type { CoreApi } from "./core-api";
import { routeHit, viewportRect } from "./hit";
import type { FrameLayer } from "./layer";
import type { LiveCoordinator } from "./live";
import { type Frame, frameAt } from "./model";

/** What the binder needs around it. */
export interface BinderDeps {
  readonly layer: FrameLayer;
  readonly core: CoreApi;
  readonly live: LiveCoordinator;
  frames(): readonly Frame[];
  view(): ViewTransform;
  /** Stage origin in client px. */
  origin(): { left: number; top: number };
  mode(): BindingMode;
  /** `GET /frames/{hex}/hit` for a document point. */
  frameHit(hex: string, x: number, y: number): Promise<Hit>;
  repaint(): void;
  problem(message: string): void;
}

/** Turns a pen-down into a frame binding. */
export class Binder {
  constructor(private readonly deps: BinderDeps) {}

  /** The binding of a stroke starting at a client point, `null` off frames. */
  bindingAt(clientX: number, clientY: number): StrokeBinding | null {
    const origin = this.deps.origin();
    const [x, y] = toPage(this.deps.view(), clientX - origin.left, clientY - origin.top);
    const frame = frameAt(this.deps.frames(), x, y);
    if (frame === null) return null;
    const mode = this.deps.mode();
    let key: string | null = null;
    if (mode === "bound") {
      this.ask(frame, [x, y], (answer) => {
        key = answer;
      });
    }
    return { frame: frame.id, mode, key: () => key };
  }

  private ask(
    frame: Frame,
    board: readonly [number, number],
    resolved: (key: string | null) => void,
  ): void {
    const { layer, core } = this.deps;
    let doc: readonly [number, number] = [board[0] - frame.x, board[1] - frame.y];
    try {
      const point = core.frame_doc_point(frame.id, board[0], board[1]);
      if (point.length === 2) doc = [point[0] ?? doc[0], point[1] ?? doc[1]];
    } catch {
      /* the frame's own origin stands in */
    }
    const route = routeHit(this.deps.live.windowOf(frame.id), board, doc);
    if (route.via === "bridge") {
      layer.hit(frame.id, route.x, route.y, (hit) => this.answered(frame, hit, false, resolved));
      return;
    }
    this.deps.frameHit(frame.hex, route.x, route.y).then(
      (hit) => this.answered(frame, hit, true, resolved),
      () => resolved(null),
    );
  }

  /** `inDoc`: the rect is in document coordinates (the server's), else viewport coordinates (the bridge's). */
  private answered(
    frame: Frame,
    hit: Hit,
    inDoc: boolean,
    resolved: (key: string | null) => void,
  ): void {
    resolved(hit.key);
    // The core converts the samples to element-local coordinates at commit: it needs the rect first.
    if (hit.key === null || hit.rect === null) return;
    const scroll = layerScroll(this.deps.layer, frame.id);
    this.mergeRect(frame, hit.key, inDoc ? viewportRect(hit.rect, scroll) : hit.rect);
  }

  /** Puts the hit element's rect into the core's layout, so an immediate bound commit places the stroke. */
  private mergeRect(frame: Frame, key: string, rect: Rect): void {
    const { layer, core } = this.deps;
    // A frame without a live window has no bridge layout of its own: only the rect goes to the core.
    const layout = layer.liveId() === frame.id ? layer.mergeRects(frame.id, { [key]: rect }) : null;
    try {
      const json =
        layout === null
          ? JSON.stringify({ preset: frame.preset, rects: { [key]: rect } })
          : layoutJson(layout, frame.preset);
      if (core.set_frame_layout(frame.id, json)) this.deps.repaint();
    } catch (error) {
      this.deps.problem(`element rect not applied: ${String(error)}`);
    }
  }
}

/** The scroll the core holds for the frame: the live page's, zero for a still. */
function layerScroll(layer: FrameLayer, frameId: string): readonly [number, number] {
  return layer.liveId() === frameId ? (layer.layoutOf(frameId)?.scroll ?? [0, 0]) : [0, 0];
}
