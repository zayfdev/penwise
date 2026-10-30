/** The viewer's camera moves: glides to a rect, follows the live page's own scrolling. */

import { CameraAnimation } from "../camera";
import { type FitMode, fitRect, type PageRect, type Viewport, type ViewTransform } from "../view";

/** What the camera needs from the app. */
export interface CameraDeps {
  view(): ViewTransform;
  viewport(): Viewport;
  /** Shows `view` (both canvases, the frames and the mark chips follow). */
  apply(view: ViewTransform): void;
  /** The camera moved by itself: the automatic refit must leave the view alone. */
  moved(): void;
}

/** Glides and follows; any input calls {@link cancel}. */
export class Camera {
  private readonly glide: CameraAnimation;

  constructor(private readonly deps: CameraDeps) {
    this.glide = new CameraAnimation({
      schedule: (callback) => requestAnimationFrame(callback),
      now: () => performance.now(),
      apply: (view) => deps.apply(view),
    });
  }

  /** Stops a running glide where it is. */
  cancel(): void {
    this.glide.cancel();
  }

  /** Glides to `rect` (`fitRect`, 220 ms ease-out-cubic). */
  fit(rect: PageRect, mode: FitMode): void {
    this.glideTo(fitRect(rect, mode, this.deps.viewport(), this.deps.view()));
  }

  /** Glides to `target`. */
  glideTo(target: ViewTransform): void {
    this.deps.moved();
    this.glide.start(this.deps.view(), target);
  }

  /**
   * The owner scrolled the live page by `dy` page px: the camera pans with it, so its window stays on screen.
   * A glide that is running (a `Focus` fit) keeps going, offset by `dy`, instead of being cancelled.
   */
  follow(dy: number): void {
    if (this.glide.offset(dy)) return;
    const view = this.deps.view();
    this.deps.apply({ ...view, originY: view.originY + dy });
  }
}
