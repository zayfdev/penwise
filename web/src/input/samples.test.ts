import { describe, expect, it } from "vitest";
import { chunkRanges, expand, isInking, type PointerLike, packSamples } from "./samples";

function pointer(partial: Partial<PointerLike> = {}): PointerLike {
  return {
    pointerId: 1,
    pointerType: "pen",
    button: 0,
    buttons: 1,
    clientX: 0,
    clientY: 0,
    pressure: 0.5,
    tiltX: 0,
    tiltY: 0,
    timeStamp: 0,
    ...partial,
  };
}

describe("packSamples", () => {
  const view = { scale: 2, originX: 10, originY: 20 };
  const rect = { left: 100, top: 50 };

  it("maps client px through the view and keeps pressure and tilt raw for wasm", () => {
    const { events } = packSamples(
      [pointer({ clientX: 140, clientY: 90, pressure: 0.25, tiltX: 12, tiltY: -3 })],
      view,
      rect,
      0,
    );
    expect([...events]).toEqual([30, 40, 0.25, 12, -3]);
  });

  it("uses each event's own timestamp in µs relative to the stroke start", () => {
    const { times } = packSamples(
      [pointer({ timeStamp: 1000 }), pointer({ timeStamp: 1004.5 }), pointer({ timeStamp: 1008 })],
      view,
      rect,
      1000,
    );
    expect([...times]).toEqual([0, 4500, 8000]);
  });

  it("never lets time run backwards, across flushes too", () => {
    const first = packSamples([pointer({ timeStamp: 10 })], view, rect, 0);
    const second = packSamples([pointer({ timeStamp: 9 })], view, rect, 0, first.times[0] ?? 0);
    expect([...first.times, ...second.times]).toEqual([10_000, 10_000]);
  });

  it("keeps µs times monotonic past the int32 limit (36 min) and saturates at u32", () => {
    const ms = (min: number) => min * 60_000;
    const { times } = packSamples(
      [
        pointer({ timeStamp: ms(30) }),
        pointer({ timeStamp: ms(40) }),
        pointer({ timeStamp: ms(90) }),
      ],
      view,
      rect,
      0,
    );
    expect([...times]).toEqual([1_800_000_000, 2_400_000_000, 4_294_967_295]);
  });
});

describe("expand", () => {
  it("returns the coalesced hardware events when the browser has them", () => {
    const a = pointer({ clientX: 1 });
    const b = pointer({ clientX: 2 });
    expect(expand(pointer({ getCoalescedEvents: () => [a, b] }))).toEqual([a, b]);
  });
  it("falls back to the event itself", () => {
    const e = pointer();
    expect(expand(e)).toEqual([e]);
    expect(expand(pointer({ getCoalescedEvents: () => [] }))).toHaveLength(1);
  });
});

describe("chunkRanges", () => {
  it("covers the count in chunks of at most the delta limit", () => {
    expect(chunkRanges(600)).toEqual([
      [0, 256],
      [256, 512],
      [512, 600],
    ]);
    expect(chunkRanges(0)).toEqual([]);
    expect(chunkRanges(3, 2)).toEqual([
      [0, 2],
      [2, 3],
    ]);
  });
});

describe("isInking", () => {
  it("accepts the tip or left button of a pen or mouse", () => {
    expect(isInking(pointer())).toBe(true);
    expect(isInking(pointer({ pointerType: "mouse", buttons: 1 }))).toBe(true);
  });
  it("rejects touch, hover, the eraser end and other buttons", () => {
    expect(isInking(pointer({ pointerType: "touch" }))).toBe(false);
    expect(isInking(pointer({ buttons: 0 }))).toBe(false);
    expect(isInking(pointer({ buttons: 1 | 32 }))).toBe(false);
    expect(isInking(pointer({ buttons: 2 }))).toBe(false);
  });
});
