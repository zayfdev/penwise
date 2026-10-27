import { beforeAll, describe, expect, it } from "vitest";
import { realWasm } from "../test/support";
import type { Wasm } from "../wasm";
import { type BrushChoice, type PenDeps, PenInput } from "./pen";
import type { PointerLike } from "./samples";

let wasm: Wasm;
beforeAll(() => {
  wasm = realWasm();
});

function pointer(partial: Partial<PointerLike> = {}): PointerLike {
  return {
    pointerId: 1,
    pointerType: "pen",
    button: 0,
    buttons: 1,
    clientX: 10,
    clientY: 20,
    pressure: 0.5,
    tiltX: 0,
    tiltY: 0,
    timeStamp: 0,
    ...partial,
  };
}

interface Calls {
  begin: unknown[][];
  pushes: { samples: number[]; times: number[] }[];
  ends: number;
  cancels: number;
  sent: Uint8Array[];
  flushes: number;
  committed: number;
  live: number;
  frames: (() => void)[];
  problems: string[];
}

function rig(page: string | null = "page"): { pen: PenInput; calls: Calls; deps: PenDeps } {
  const calls: Calls = {
    begin: [],
    pushes: [],
    ends: 0,
    cancels: 0,
    sent: [],
    flushes: 0,
    committed: 0,
    live: 0,
    frames: [],
    problems: [],
  };
  const brush: BrushChoice = { preset: 3, argb: 0xff112233, scale: 1.5 };
  const deps: PenDeps = {
    viewer: {
      stroke_begin: (...args: unknown[]) => {
        calls.begin.push(args);
        return "id";
      },
      stroke_push: (samples: Float32Array, times: Uint32Array) => {
        calls.pushes.push({ samples: [...samples], times: [...times] });
        return new Uint8Array([calls.pushes.length]);
      },
      stroke_end: () => {
        calls.ends++;
      },
      stroke_cancel: () => {
        calls.cancels++;
        return new Uint8Array([99]);
      },
    },
    wasm,
    send: (frame) => {
      calls.sent.push(frame);
      return true;
    },
    flush: () => {
      calls.flushes++;
    },
    page: () => page,
    view: () => ({ scale: 1, originX: 0, originY: 0 }),
    brush: () => brush,
    origin: () => ({ left: 0, top: 0 }),
    liveChanged: () => {
      calls.live++;
    },
    committedChanged: () => {
      calls.committed++;
    },
    now: () => 123,
    problem: (m) => calls.problems.push(m),
    schedule: (cb) => calls.frames.push(cb),
  };
  return { pen: new PenInput(deps), calls, deps };
}

describe("PenInput", () => {
  it("begins with the toolbar brush, streams a delta per frame and commits on pen-up", () => {
    const { pen, calls } = rig();
    expect(pen.down(pointer({ timeStamp: 100 }))).toBe(true);
    expect(calls.begin).toEqual([["page", 3, 0xff112233, 1.5, 123]]);
    expect(calls.pushes).toHaveLength(1);

    pen.move(pointer({ clientX: 30, timeStamp: 108 }));
    pen.move(pointer({ clientX: 40, timeStamp: 116 }));
    expect(calls.pushes).toHaveLength(1);
    expect(calls.frames).toHaveLength(1);
    calls.frames[0]?.();
    expect(calls.pushes[1]?.times).toEqual([8000, 16000]);

    pen.up(pointer({ clientX: 50, buttons: 0, timeStamp: 120 }));
    expect(calls.ends).toBe(1);
    expect(calls.flushes).toBe(1);
    expect(calls.committed).toBe(1);
    expect(pen.drawing).toBe(false);
    expect(calls.sent.map((f) => f[0])).toEqual([1, 2, 3]);
  });

  it("feeds coalesced events with their own timestamps and converts tilt in wasm", () => {
    const { pen, calls } = rig();
    pen.down(pointer({ timeStamp: 0 }));
    const a = pointer({ clientX: 11, timeStamp: 2, tiltX: 30 });
    const b = pointer({ clientX: 12, timeStamp: 4, tiltX: 30 });
    pen.move(pointer({ timeStamp: 4, getCoalescedEvents: () => [a, b] }));
    calls.frames[0]?.();
    const push = calls.pushes[1];
    expect(push?.times).toEqual([2000, 4000]);
    // [x, y, pressure, tilt, azimuth] per sample: 30° toward +x.
    expect(push?.samples[0]).toBe(11);
    expect(push?.samples[3]).toBeCloseTo((30 * Math.PI) / 180, 2);
    expect(push?.samples[4]).toBeCloseTo(Math.PI / 2, 2);
  });

  it("splits a long burst into deltas of at most 256 samples", () => {
    const { pen, calls } = rig();
    pen.down(pointer());
    const burst = Array.from({ length: 600 }, (_, i) => pointer({ clientX: i, timeStamp: i }));
    pen.move(pointer({ getCoalescedEvents: () => burst }));
    calls.frames[0]?.();
    expect(calls.pushes.slice(1).map((p) => p.times.length)).toEqual([256, 256, 88]);
  });

  it("pointercancel sends a cancel and commits nothing", () => {
    const { pen, calls } = rig();
    pen.down(pointer());
    pen.cancel(pointer());
    expect(calls.cancels).toBe(1);
    expect(calls.ends).toBe(0);
    expect(calls.sent.at(-1)).toEqual(new Uint8Array([99]));
    pen.up(pointer());
    expect(calls.ends).toBe(0);
  });

  it("ignores touch, hover, other pointers and a missing page", () => {
    const { pen, calls } = rig();
    expect(pen.down(pointer({ pointerType: "touch" }))).toBe(false);
    expect(pen.down(pointer({ buttons: 0 }))).toBe(false);
    expect(rig(null).pen.down(pointer())).toBe(false);
    pen.down(pointer({ pointerId: 1 }));
    pen.move(pointer({ pointerId: 2, timeStamp: 5 }));
    expect(calls.frames).toHaveLength(0);
    expect(pen.down(pointer({ pointerId: 3 }))).toBe(false);
  });

  it("abort (page switch) cancels the open stroke", () => {
    const { pen, calls } = rig();
    pen.down(pointer());
    pen.abort();
    expect(calls.cancels).toBe(1);
    expect(pen.drawing).toBe(false);
  });

  it("reports and drops a stroke the viewer refuses", () => {
    const { calls, deps } = rig();
    const pen = new PenInput({
      ...deps,
      viewer: {
        ...deps.viewer,
        stroke_begin: () => {
          throw new Error("unknown page");
        },
      },
    });
    expect(pen.down(pointer())).toBe(false);
    expect(calls.problems[0]).toContain("unknown page");
  });
});
