import { describe, expect, it } from "vitest";
import { routeHit, viewportRect } from "./hit";

describe("routeHit", () => {
  // The live window of a frame whose box is at board (200, 100) and whose page is scrolled 3000 px.
  const window = [200, 3100, 1440, 900] as const;

  it("asks the page's bridge inside the live window, in viewport px (doc minus scroll)", () => {
    const route = routeHit(window, [500, 3400], [300, 3300]);
    expect(route).toEqual({ via: "bridge", x: 300, y: 300 });
  });

  it("asks the server outside the window, in document px", () => {
    expect(routeHit(window, [500, 600], [300, 500])).toEqual({ via: "server", x: 300, y: 500 });
    expect(routeHit(window, [500, 4100], [300, 4000])).toEqual({ via: "server", x: 300, y: 4000 });
  });

  it("asks the server when the frame has no live window", () => {
    expect(routeHit(null, [500, 600], [300, 500])).toEqual({ via: "server", x: 300, y: 500 });
  });

  it("treats the window's far edges as outside", () => {
    expect(routeHit(window, [1640, 3200], [1440, 3100]).via).toBe("server");
    expect(routeHit(window, [200, 3100], [0, 3000]).via).toBe("bridge");
  });
});

describe("viewportRect", () => {
  it("turns a server rect in document px into the viewport rect the core adds the scroll to", () => {
    expect(viewportRect([40, 3300, 200, 60], [0, 3000])).toEqual([40, 300, 200, 60]);
    expect(viewportRect([40, 300, 200, 60], [0, 0])).toEqual([40, 300, 200, 60]);
  });
});
