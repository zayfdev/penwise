/**
 * Browser drawing: pointer events → live deltas while the pen moves, one committed stroke on pen-up.
 *
 * All edits are built in Rust: `Viewer.stroke_*` returns the `StrokeDelta`/`StrokeCancel` frames and queues
 * the `Add` op; this class only forwards samples (coalesced events with their real timestamps), sends the
 * returned frames and asks the connection to flush the outbox. `pointercancel` abandons the stroke.
 */

import type { ViewTransform } from "../view";
import type { Viewer, Wasm } from "../wasm";
import {
  chunkRanges,
  expand,
  isInking,
  MAX_DELTA_SAMPLES,
  type PointerLike,
  packSamples,
} from "./samples";

/** Floats per sample `pointer_samples` returns. */
const SAMPLE_STRIDE = 5;

/** Brush settings of the toolbar. */
export interface BrushChoice {
  /** `Preset::ALL` index. */
  readonly preset: number;
  /** `0xAARRGGBB`. */
  readonly argb: number;
  /** Size multiplier. */
  readonly scale: number;
}

/** Everything the pen needs from the app. */
export interface PenDeps {
  readonly viewer: Pick<Viewer, "stroke_begin" | "stroke_push" | "stroke_end" | "stroke_cancel">;
  readonly wasm: Pick<Wasm, "pointer_samples">;
  /** Sends a client frame (false: offline; previews are droppable, the committed op is queued regardless). */
  send(frame: Uint8Array): boolean;
  /** Flushes the outbox after a commit. */
  flush(): void;
  /** Page being shown, `null` when there is none. */
  page(): string | null;
  view(): ViewTransform;
  brush(): BrushChoice;
  /** Stage origin in client px. */
  origin(): { left: number; top: number };
  /** Live layer changed. */
  liveChanged(): void;
  /** A stroke was committed or cancelled (committed layer must repaint). */
  committedChanged(): void;
  now(): number;
  problem(message: string): void;
  /** Runs `callback` before the next paint. */
  schedule(callback: () => void): void;
}

interface Active {
  readonly pointerId: number;
  readonly t0: number;
  readonly page: string;
  pending: PointerLike[];
  lastUs: number;
  flushScheduled: boolean;
}

/** Pen/mouse drawing controller. Touch is ignored (the stage pans with it). */
export class PenInput {
  private active: Active | null = null;

  constructor(private readonly deps: PenDeps) {}

  /** True while a stroke is open. */
  get drawing(): boolean {
    return this.active !== null;
  }

  /** `pointerdown`: begins a stroke. Returns true when the event started ink (caller captures the pointer). */
  down(event: PointerLike): boolean {
    const page = this.deps.page();
    if (this.active || page === null || !isInking(event)) return false;
    const brush = this.deps.brush();
    try {
      this.deps.viewer.stroke_begin(page, brush.preset, brush.argb, brush.scale, this.deps.now());
    } catch (error) {
      this.deps.problem(`cannot draw: ${String(error)}`);
      return false;
    }
    this.active = {
      pointerId: event.pointerId,
      t0: event.timeStamp,
      page,
      pending: [...expand(event)],
      lastUs: 0,
      flushScheduled: false,
    };
    this.flush();
    return true;
  }

  /** `pointermove` of the capturing pointer: queues its coalesced events for the next frame. */
  move(event: PointerLike): void {
    const active = this.active;
    if (!active || event.pointerId !== active.pointerId) return;
    active.pending.push(...expand(event));
    if (!active.flushScheduled) {
      active.flushScheduled = true;
      this.deps.schedule(() => this.flush());
    }
  }

  /** `pointerup`: sends the tail and commits. */
  up(event: PointerLike): void {
    const active = this.active;
    if (!active || event.pointerId !== active.pointerId) return;
    active.pending.push(...expand(event));
    this.flush();
    this.active = null;
    try {
      this.deps.viewer.stroke_end(this.deps.now());
    } catch (error) {
      this.deps.problem(`stroke not saved: ${String(error)}`);
    }
    this.deps.flush();
    this.deps.committedChanged();
    this.deps.liveChanged();
  }

  /** `pointercancel` (palm rejection, system gesture): drops the stroke and tells the others. */
  cancel(event?: Pick<PointerLike, "pointerId">): void {
    const active = this.active;
    if (!active || (event && event.pointerId !== active.pointerId)) return;
    this.active = null;
    try {
      this.deps.send(this.deps.viewer.stroke_cancel());
    } catch (error) {
      this.deps.problem(`cancel failed: ${String(error)}`);
    }
    this.deps.liveChanged();
  }

  /** The shown page changed or the connection was reset: abandon any open stroke. */
  abort(): void {
    this.cancel();
  }

  private flush(): void {
    const active = this.active;
    if (!active) return;
    active.flushScheduled = false;
    if (active.pending.length === 0) return;
    const events = active.pending;
    active.pending = [];
    const packed = packSamples(
      events,
      this.deps.view(),
      this.deps.origin(),
      active.t0,
      active.lastUs,
    );
    active.lastUs = packed.times[packed.times.length - 1] ?? active.lastUs;
    const samples = this.deps.wasm.pointer_samples(packed.events);
    try {
      for (const [from, to] of chunkRanges(packed.times.length, MAX_DELTA_SAMPLES)) {
        const frame = this.deps.viewer.stroke_push(
          samples.subarray(from * SAMPLE_STRIDE, to * SAMPLE_STRIDE),
          packed.times.subarray(from, to),
        );
        this.deps.send(frame);
      }
    } catch (error) {
      this.deps.problem(`stroke rejected: ${String(error)}`);
      this.cancel();
      return;
    }
    this.deps.liveChanged();
  }
}
