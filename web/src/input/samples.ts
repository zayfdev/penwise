/**
 * Pointer events → the flat arrays `pointer_samples` / `Viewer.stroke_push` take. The pressure/tilt → core
 * sample conversion itself is wasm (`pointer_samples`, the same mapping as the tablet's digitizer frames);
 * this file only picks positions and real timestamps out of (coalesced) events.
 */

import { toPage, type ViewTransform } from "../view";

/** The slice of a `PointerEvent` sampling reads. */
export interface PointerLike {
  readonly pointerId: number;
  readonly pointerType: string;
  readonly button: number;
  readonly buttons: number;
  readonly clientX: number;
  readonly clientY: number;
  readonly pressure: number;
  readonly tiltX: number;
  readonly tiltY: number;
  readonly timeStamp: number;
  getCoalescedEvents?(): readonly PointerLike[];
}

/** Floats per event in {@link PackedSamples.events}: `x, y, pressure, tiltX°, tiltY°` (wasm `POINTER_STRIDE`). */
export const POINTER_STRIDE = 5;
/** Max samples per `StrokeDelta` (proto `MAX_DELTA_SAMPLES`). */
export const MAX_DELTA_SAMPLES = 256;

/** Events of one flush, ready for wasm. */
export interface PackedSamples {
  readonly events: Float32Array;
  /** Microseconds since the stroke's first event, non-decreasing. */
  readonly times: Int32Array;
}

/** The real hardware events behind `event`: its coalesced events, or itself. */
export function expand(event: PointerLike): readonly PointerLike[] {
  const coalesced = event.getCoalescedEvents?.() ?? [];
  return coalesced.length > 0 ? coalesced : [event];
}

/**
 * Packs events into wasm input. Positions go through `view` into page px (`rect` = stage origin in client px);
 * timestamps are the events' own (`t0` = first event's `timeStamp`, ms) so velocity-dependent brushes see real
 * timing; a timestamp earlier than its predecessor is clamped to it.
 */
export function packSamples(
  events: readonly PointerLike[],
  view: ViewTransform,
  rect: { readonly left: number; readonly top: number },
  t0: number,
  lastUs = 0,
): PackedSamples {
  const out = new Float32Array(events.length * POINTER_STRIDE);
  const times = new Int32Array(events.length);
  let previous = lastUs;
  events.forEach((e, i) => {
    const [x, y] = toPage(view, e.clientX - rect.left, e.clientY - rect.top);
    out.set([x, y, e.pressure, e.tiltX, e.tiltY], i * POINTER_STRIDE);
    previous = Math.max(previous, Math.round((e.timeStamp - t0) * 1000));
    times[i] = previous;
  });
  return { events: out, times };
}

/** `[start, end)` index ranges of at most `size` items covering `count`. */
export function chunkRanges(count: number, size: number = MAX_DELTA_SAMPLES): [number, number][] {
  const ranges: [number, number][] = [];
  for (let start = 0; start < count; start += size)
    ranges.push([start, Math.min(count, start + size)]);
  return ranges;
}

/** Whether `event` starts/continues ink: a pen or mouse with its primary (tip/left) button down. */
export function isInking(event: PointerLike): boolean {
  if (event.pointerType === "touch") return false;
  const eraser = (event.buttons & 32) !== 0;
  return !eraser && (event.buttons & 1) !== 0;
}
