import { describe, expect, it } from "vitest";
import { FIT_MARGIN_PX, fitRect, fitView, panBy, toPage, zoomAt } from "./view";

const viewport = { width: 800, height: 600 };

describe("fitView", () => {
  it("fits a fixed sheet centred with a uniform scale", () => {
    const view = fitView({ fixed: true, width: 1600, height: 2560 }, null, viewport);
    expect(view.scale).toBeCloseTo(600 / 2560);
    const [cx, cy] = toPage(view, 400, 300);
    expect(cx).toBeCloseTo(800);
    expect(cy).toBeCloseTo(1280);
  });

  it("fits infinite pages around their content plus a margin", () => {
    const view = fitView({ fixed: false, width: 0, height: 0 }, [100, 100, 500, 300], viewport);
    const [x0, y0] = toPage(view, 0, 0);
    const [x1, y1] = toPage(view, viewport.width, viewport.height);
    expect(x0).toBeLessThanOrEqual(100 - FIT_MARGIN_PX + 1e-6);
    expect(x1).toBeGreaterThanOrEqual(500 + FIT_MARGIN_PX - 1e-6);
    expect(y0).toBeLessThanOrEqual(100 - FIT_MARGIN_PX + 1e-6);
    expect(y1).toBeGreaterThanOrEqual(300 + FIT_MARGIN_PX - 1e-6);
  });

  it("puts an empty infinite page's origin in the middle at 1:1", () => {
    const view = fitView({ fixed: false, width: 0, height: 0 }, undefined, viewport);
    expect(view.scale).toBe(1);
    expect(toPage(view, 400, 300)).toEqual([0, 0]);
  });
});

describe("zoom and pan", () => {
  it("zoomAt keeps the page point under the cursor fixed", () => {
    const view = { scale: 1, originX: 10, originY: 20 };
    const before = toPage(view, 300, 200);
    const zoomed = zoomAt(view, 2, 300, 200);
    expect(zoomed.scale).toBe(2);
    const after = toPage(zoomed, 300, 200);
    expect(after[0]).toBeCloseTo(before[0]);
    expect(after[1]).toBeCloseTo(before[1]);
  });

  it("clamps the scale", () => {
    expect(zoomAt({ scale: 1, originX: 0, originY: 0 }, 1e9, 0, 0).scale).toBe(8);
    expect(zoomAt({ scale: 1, originX: 0, originY: 0 }, 1e-9, 0, 0).scale).toBe(0.05);
  });

  it("panBy moves content with the pointer", () => {
    const view = panBy({ scale: 2, originX: 0, originY: 0 }, 10, -4);
    expect(view.originX).toBe(-5);
    expect(view.originY).toBe(2);
  });
});

describe("fitRect", () => {
  const stage = { width: 2560, height: 1600 };
  const rest = { scale: 1, originX: 0, originY: 0 };

  it("width: the frame fills the stage width and starts at the left edge", () => {
    const view = fitRect({ x: 300, y: 100, w: 1440, h: 5000 }, "width", stage, rest);
    expect(view.scale).toBeCloseTo(2560 / 1440);
    const [left, top] = toPage(view, 0, 0);
    const [right] = toPage(view, stage.width, 0);
    expect(left).toBeCloseTo(300);
    expect(right).toBeCloseTo(300 + 1440);
    // The camera was above the frame, so the frame's top is the stage top.
    expect(top).toBeCloseTo(100);
  });

  it("width: keeps the top of the visible part of a long frame", () => {
    const rect = { x: 0, y: 0, w: 1440, h: 6000 };
    const scrolled = { scale: 0.5, originX: 0, originY: 2000 };
    const view = fitRect(rect, "width", stage, scrolled);
    expect(toPage(view, 0, 0)[1]).toBeCloseTo(2000);
    // ...but never past the frame's end.
    const below = fitRect(rect, "width", stage, { scale: 0.5, originX: 0, originY: 9000 });
    const [, bottom] = toPage(below, 0, stage.height);
    expect(bottom).toBeCloseTo(6000);
  });

  it("width: centres a frame shorter than the stage", () => {
    const view = fitRect({ x: 0, y: 1000, w: 1440, h: 200 }, "width", stage, rest);
    const [, top] = toPage(view, 0, 0);
    const [, bottom] = toPage(view, 0, stage.height);
    expect((top + bottom) / 2).toBeCloseTo(1100);
  });

  it("all: fits the whole rect with a margin, centred", () => {
    const rect = { x: 0, y: 0, w: 1440, h: 6000 };
    const view = fitRect(rect, "all", stage, rest);
    const [x0, y0] = toPage(view, 0, 0);
    const [x1, y1] = toPage(view, stage.width, stage.height);
    expect(y0).toBeLessThanOrEqual(-FIT_MARGIN_PX + 1e-6);
    expect(y1).toBeGreaterThanOrEqual(6000 + FIT_MARGIN_PX - 1e-6);
    expect((x0 + x1) / 2).toBeCloseTo(720);
  });

  it("keeps the view for a degenerate rect or stage", () => {
    expect(fitRect({ x: 0, y: 0, w: 0, h: 10 }, "width", stage, rest)).toBe(rest);
    expect(fitRect({ x: 0, y: 0, w: 10, h: 10 }, "all", { width: 0, height: 0 }, rest)).toBe(rest);
  });
});
