/**
 * The page area: routes pointer, wheel and resize events. Pen/mouse go to the pen controller; touch pans
 * (palms never draw); the wheel pans and ctrl/pinch-wheel zooms.
 */

import type { PenInput } from "../input/pen";
import type { PointerLike } from "../input/samples";
import { panBy, type Viewport, type ViewTransform, zoomAt } from "../view";

/** What the stage needs from the app. */
export interface StageDeps {
  readonly pen: PenInput;
  view(): ViewTransform;
  /** The user moved the view (pan/zoom). */
  setView(view: ViewTransform): void;
  /** The stage changed size. */
  resized(viewport: Viewport, dpr: number): void;
  /** The last pointer type seen anywhere on the window (`pen`/`mouse`/`touch`): frame input routing. */
  pointer(type: string): void;
}

/** Wires `stage`'s events. */
export function attachStage(stage: HTMLElement, deps: StageDeps): void {
  let panning: { id: number; x: number; y: number } | null = null;

  const rectOf = () => stage.getBoundingClientRect();

  const notePointer = (event: PointerEvent) => deps.pointer(event.pointerType);
  window.addEventListener("pointerdown", notePointer, true);
  window.addEventListener("pointermove", notePointer, true);

  stage.addEventListener("pointerdown", (event) => {
    // Frame chips are controls, not canvas: their clicks must not start a stroke.
    if (event.target instanceof Element && event.target.closest("#frame-chips") !== null) return;
    if (event.pointerType === "touch") {
      panning = { id: event.pointerId, x: event.clientX, y: event.clientY };
      stage.setPointerCapture(event.pointerId);
      return;
    }
    if (deps.pen.down(event as PointerLike)) {
      stage.setPointerCapture(event.pointerId);
      event.preventDefault();
    }
  });
  stage.addEventListener("pointermove", (event) => {
    if (panning && event.pointerId === panning.id) {
      deps.setView(panBy(deps.view(), event.clientX - panning.x, event.clientY - panning.y));
      panning = { id: panning.id, x: event.clientX, y: event.clientY };
      return;
    }
    deps.pen.move(event as PointerLike);
  });
  const end = (event: PointerEvent) => {
    if (panning && event.pointerId === panning.id) panning = null;
    else deps.pen.up(event as PointerLike);
  };
  stage.addEventListener("pointerup", end);
  stage.addEventListener("pointercancel", (event) => {
    if (panning && event.pointerId === panning.id) panning = null;
    else deps.pen.cancel(event);
  });
  stage.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const rect = rectOf();
      if (event.ctrlKey || event.metaKey) {
        const factor = Math.exp(-event.deltaY * 0.01);
        deps.setView(
          zoomAt(deps.view(), factor, event.clientX - rect.left, event.clientY - rect.top),
        );
      } else {
        deps.setView(panBy(deps.view(), -event.deltaX, -event.deltaY));
      }
    },
    { passive: false },
  );
  const observer = new ResizeObserver(() => {
    const rect = rectOf();
    deps.resized({ width: rect.width, height: rect.height }, window.devicePixelRatio || 1);
  });
  observer.observe(stage);
}
