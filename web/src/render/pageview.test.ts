import { describe, expect, it } from "vitest";
import type { PageEntry } from "../wasm";
import { EMPTY_DRAW } from "./drawdata";
import { PaperGrain } from "./grain";
import { ImageCache } from "./images";
import { PageView } from "./pageview";

/** A 2D context stand-in that accepts every call (the painters are exercised in the browser e2e). */
function fakeCanvas(): HTMLCanvasElement {
  const canvas = { width: 100, height: 100 } as HTMLCanvasElement;
  const ctx = new Proxy(
    { canvas },
    {
      get: (target, key) => (key in target ? Reflect.get(target, key) : () => undefined),
      set: () => true,
    },
  );
  canvas.getContext = (() => ctx) as unknown as HTMLCanvasElement["getContext"];
  return canvas;
}

const page = { fixed: true, width: 100, height: 100, background: 0 } as PageEntry;

function rig() {
  const frames: (() => void)[] = [];
  const view = new PageView(
    fakeCanvas(),
    fakeCanvas(),
    { page: () => page, committed: () => EMPTY_DRAW, live: () => EMPTY_DRAW },
    new PaperGrain(),
    new ImageCache(
      () => Promise.reject(new Error("no assets")),
      () => undefined,
    ),
    (cb) => frames.push(cb),
  );
  return { view, frames };
}

describe("PageView", () => {
  it("coalesces invalidations into one frame", () => {
    const { view, frames } = rig();
    view.invalidateCommitted();
    view.invalidateLive();
    view.invalidateLive();
    expect(frames).toHaveLength(1);
  });

  it("live-only frames never repaint the committed layer", () => {
    const { view, frames } = rig();
    view.invalidateCommitted();
    frames.shift()?.();
    const base = { ...view.stats };
    for (let i = 0; i < 5; i++) {
      view.invalidateLive();
      frames.shift()?.();
    }
    expect(view.stats.live).toBe(base.live + 5);
    expect(view.stats.committed).toBe(base.committed);
  });

  it("a committed change, a view change and a resize each repaint the committed layer", () => {
    const { view, frames } = rig();
    view.invalidateCommitted();
    frames.shift()?.();
    const base = view.stats.committed;
    view.invalidateCommitted();
    frames.shift()?.();
    view.setView({ scale: 2, originX: 0, originY: 0 });
    frames.shift()?.();
    view.resize({ width: 50, height: 50 }, 2);
    frames.shift()?.();
    expect(view.stats.committed).toBe(base + 3);
    expect(view.live.width).toBe(100);
  });
});
