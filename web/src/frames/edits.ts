/** The owner's edits of frames and marks: add, preset, route, variant pick, mark status. Each is one core edit. */

import { toPage, type ViewTransform } from "../view";
import type { Viewer } from "../wasm";
import type { PatchFailure } from "./bridge-messages";
import type { CoreApi } from "./core-api";
import type { MarkStatus } from "./marks";
import { type Frame, normalizeRoute, presetSize } from "./model";

/** Inset of a frame added from the sidebar, CSS px from the visible top-left. */
const FRAME_INSET_PX = 32;

/** What the edits need from the app. */
export interface EditDeps {
  readonly viewer: Viewer;
  readonly core: CoreApi;
  page(): string | null;
  view(): ViewTransform;
  /** Flushes the outbox after an edit. */
  flush(): void;
  /** An edit landed: repaint, refresh history/status and the chips. */
  changed(): void;
  problem(message: string): void;
}

/** Frame and mark edits of the shown page. */
export class FrameEdits {
  constructor(private readonly deps: EditDeps) {}

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
    this.run("frame not added", () => this.deps.viewer.add_frame(page, json, Date.now()));
  }

  setPreset(frame: Frame, preset: string): void {
    const size = presetSize(preset) ?? { cssW: frame.cssW, cssH: frame.cssH };
    this.run("preset not changed", () =>
      this.deps.viewer.set_frame_preset(frame.id, preset, size.cssW, size.cssH, Date.now()),
    );
  }

  setRoute(frame: Frame, route: string): void {
    this.run("route not changed", () =>
      this.deps.viewer.set_frame_route(frame.id, normalizeRoute(route), Date.now()),
    );
  }

  /** The owner picked a variant: it becomes the chosen frame of its group (the core un-picks the others). */
  setChosen(frame: Frame): void {
    this.run("variant not chosen", () =>
      this.deps.core.set_frame_chosen(frame.id, true, Date.now()),
    );
  }

  setStatus(id: string, status: MarkStatus): void {
    this.run("mark not updated", () => this.deps.core.set_mark_status(id, status, Date.now()));
  }

  /** A patch op the bridge could not apply (bad selector, refused attribute): say which, once. */
  patched(frame: Frame, ok: boolean, failed: readonly PatchFailure[]): void {
    if (ok || failed.length === 0) return;
    const first = failed[0];
    const reason = first && first.reason !== "" ? `: ${first.reason}` : "";
    const ops = `${failed.length} failed op${failed.length === 1 ? "" : "s"}`;
    this.deps.problem(`ghost on ${frame.label || frame.route} has ${ops}${reason}`);
  }

  /** Runs one core edit; on success sends it and repaints. */
  private run(failure: string, edit: () => unknown): void {
    try {
      edit();
    } catch (error) {
      this.deps.problem(`${failure}: ${String(error)}`);
      return;
    }
    this.deps.flush();
    this.deps.changed();
  }
}
