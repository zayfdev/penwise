import { describe, expect, it } from "vitest";
import { FIT_MARGIN_PX, fitView, panBy, toPage, zoomAt } from "./view";

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
