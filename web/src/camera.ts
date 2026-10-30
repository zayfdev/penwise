/**
 * Camera moves: the 220 ms ease-out-cubic glide of `fitRect` (contract §3) and the settle timer that tells
 * the frame layer the view has stopped moving. Time and scheduling are injected, so tests drive both by hand.
 */

import type { ViewTransform } from "./view";

/** Length of a fit animation, ms. */
export const FIT_DURATION_MS = 220;
/** The view counts as settled this long after its last change, ms. */
export const SETTLE_MS = 150;

/** Ease-out cubic on `t` in `[0, 1]`. */
export function easeOutCubic(t: number): number {
  const u = 1 - Math.min(1, Math.max(0, t));
  return 1 - u * u * u;
}

/**
 * The view `t` of the way from `from` to `to`: the scale moves geometrically (a zoom feels even), the origin
 * linearly (both ends are fit results, so that is smooth enough).
 */
export function lerpView(from: ViewTransform, to: ViewTransform, t: number): ViewTransform {
  if (t <= 0) return from;
  if (t >= 1) return to;
  return {
    scale: from.scale * (to.scale / from.scale) ** t,
    originX: from.originX + (to.originX - from.originX) * t,
    originY: from.originY + (to.originY - from.originY) * t,
  };
}

/** A running glide. */
interface Glide {
  readonly from: ViewTransform;
  readonly to: ViewTransform;
  readonly startedAt: number;
}

/** What the animation needs from the page. */
export interface CameraEnv {
  /** Runs `callback` before the next paint. */
  schedule(callback: () => void): void;
  now(): number;
  /** Applies one step of the glide. */
  apply(view: ViewTransform): void;
}

/** One glide at a time; starting another or calling {@link cancel} drops the running one. */
export class CameraAnimation {
  private glide: Glide | null = null;
  private generation = 0;

  constructor(
    private readonly env: CameraEnv,
    private readonly duration: number = FIT_DURATION_MS,
  ) {}

  /** True while a glide is running. */
  get running(): boolean {
    return this.glide !== null;
  }

  /** Glides from `from` to `to`; a zero distance applies `to` at once. */
  start(from: ViewTransform, to: ViewTransform): void {
    this.generation += 1;
    this.glide = { from, to, startedAt: this.env.now() };
    this.step(this.generation);
  }

  /**
   * The page under the camera moved by `dy` page px while a glide runs: both ends of the glide move with it, so
   * the glide still lands on what it was heading for. False when no glide runs.
   */
  offset(dy: number): boolean {
    const glide = this.glide;
    if (glide === null) return false;
    const shift = (view: ViewTransform): ViewTransform => ({ ...view, originY: view.originY + dy });
    this.glide = { ...glide, from: shift(glide.from), to: shift(glide.to) };
    return true;
  }

  /** Any input cancels the glide where it is. */
  cancel(): void {
    this.glide = null;
    this.generation += 1;
  }

  private step(generation: number): void {
    const glide = this.glide;
    if (glide === null || generation !== this.generation) return;
    const t = (this.env.now() - glide.startedAt) / this.duration;
    const done = t >= 1;
    if (done) this.glide = null;
    this.env.apply(lerpView(glide.from, glide.to, easeOutCubic(t)));
    if (!done) this.env.schedule(() => this.step(generation));
  }
}

/** Timers the settle debounce uses. */
export interface SettleTimers {
  set(callback: () => void, ms: number): number;
  clear(handle: number): void;
}

/** Fires `onSettled` once the view has not changed for {@link SETTLE_MS}. */
export class Settle {
  private handle: number | null = null;

  constructor(
    private readonly onSettled: () => void,
    private readonly timers: SettleTimers,
    private readonly ms: number = SETTLE_MS,
  ) {}

  /** The view (or the frame set) changed: restart the quiet period. */
  poke(): void {
    if (this.handle !== null) this.timers.clear(this.handle);
    this.handle = this.timers.set(() => {
      this.handle = null;
      this.onSettled();
    }, this.ms);
  }
}
