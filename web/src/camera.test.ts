import { describe, expect, it } from "vitest";
import {
  CameraAnimation,
  easeOutCubic,
  FIT_DURATION_MS,
  lerpView,
  SETTLE_MS,
  Settle,
} from "./camera";
import type { ViewTransform } from "./view";

const from: ViewTransform = { scale: 0.5, originX: 0, originY: 0 };
const to: ViewTransform = { scale: 2, originX: 300, originY: 800 };

describe("easing", () => {
  it("is ease-out: fast first, slow last, monotonic", () => {
    expect(easeOutCubic(0)).toBe(0);
    expect(easeOutCubic(1)).toBe(1);
    expect(easeOutCubic(0.5)).toBeGreaterThan(0.5);
    let last = 0;
    for (let t = 0.1; t <= 1; t += 0.1) {
      const value = easeOutCubic(t);
      expect(value).toBeGreaterThan(last);
      last = value;
    }
    expect(easeOutCubic(-1)).toBe(0);
    expect(easeOutCubic(5)).toBe(1);
  });

  it("lerpView hits both ends and zooms geometrically", () => {
    expect(lerpView(from, to, 0)).toBe(from);
    expect(lerpView(from, to, 1)).toBe(to);
    const mid = lerpView(from, to, 0.5);
    expect(mid.scale).toBeCloseTo(1);
    expect(mid.originX).toBeCloseTo(150);
    expect(mid.originY).toBeCloseTo(400);
  });
});

describe("CameraAnimation", () => {
  function rig() {
    const steps: ViewTransform[] = [];
    const queue: (() => void)[] = [];
    const clock = { now: 1000 };
    const animation = new CameraAnimation({
      schedule: (callback) => queue.push(callback),
      now: () => clock.now,
      apply: (view) => steps.push(view),
    });
    const frame = (advance: number) => {
      clock.now += advance;
      queue.shift()?.();
    };
    return { animation, steps, frame, queue };
  }

  it("glides in 220 ms and ends exactly on the target", () => {
    const { animation, steps, frame, queue } = rig();
    animation.start(from, to);
    expect(animation.running).toBe(true);
    expect(steps).toHaveLength(1);
    frame(100);
    expect(steps[1]?.scale).toBeGreaterThan(from.scale);
    expect(steps[1]?.scale).toBeLessThan(to.scale);
    frame(FIT_DURATION_MS);
    expect(steps[steps.length - 1]).toEqual(to);
    expect(animation.running).toBe(false);
    expect(queue).toHaveLength(0);
  });

  it("any input cancels it where it is", () => {
    const { animation, steps, frame } = rig();
    animation.start(from, to);
    frame(60);
    const count = steps.length;
    animation.cancel();
    expect(animation.running).toBe(false);
    frame(60);
    expect(steps).toHaveLength(count);
  });

  it("keeps going, offset, when the page under it moves", () => {
    const { animation, steps, frame } = rig();
    animation.start(from, to);
    frame(60);
    expect(animation.offset(300)).toBe(true);
    frame(FIT_DURATION_MS);
    expect(steps[steps.length - 1]).toEqual({ ...to, originY: to.originY + 300 });
    expect(animation.offset(10)).toBe(false);
  });

  it("a new glide replaces the running one", () => {
    const { animation, steps, frame } = rig();
    animation.start(from, to);
    frame(50);
    animation.start(to, from);
    frame(FIT_DURATION_MS);
    frame(FIT_DURATION_MS);
    expect(steps[steps.length - 1]).toEqual(from);
  });
});

describe("Settle", () => {
  it("fires once, SETTLE_MS after the last change", () => {
    const timers = new Map<number, { at: number; run: () => void }>();
    let next = 1;
    let now = 0;
    let fired = 0;
    const settle = new Settle(
      () => {
        fired += 1;
      },
      {
        set: (run, ms) => {
          timers.set(next, { at: now + ms, run });
          return next++;
        },
        clear: (handle) => {
          timers.delete(handle);
        },
      },
    );
    const advance = (ms: number) => {
      now += ms;
      for (const [handle, timer] of [...timers]) {
        if (timer.at <= now) {
          timers.delete(handle);
          timer.run();
        }
      }
    };
    settle.poke();
    advance(SETTLE_MS - 10);
    settle.poke();
    advance(SETTLE_MS - 10);
    expect(fired).toBe(0);
    advance(20);
    expect(fired).toBe(1);
    advance(1000);
    expect(fired).toBe(1);
  });
});
