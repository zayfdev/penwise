import { describe, expect, it } from "vitest";
import type { Frame } from "./model";
import { readableFrame, readableView } from "./readable";

const frame = (over: Partial<Frame>): Frame => ({
  id: "f",
  project: "p",
  route: "/",
  preset: "desktop",
  cssW: 1440,
  cssH: 900,
  extent: "full",
  docH: 5000,
  boxH: 5000,
  hex: "0".repeat(32),
  x: 0,
  y: 0,
  patch: null,
  label: "",
  group: null,
  chosen: false,
  ...over,
});

describe("readableFrame", () => {
  const frames = [
    frame({ id: "low", y: 6000 }),
    frame({ id: "right", x: 2000, y: 0 }),
    frame({ id: "top", x: 0, y: 0 }),
  ];

  it("is the top-left frame by default", () => {
    expect(readableFrame(frames, null)?.id).toBe("top");
  });

  it("is the tablet's frame when it is on the page", () => {
    expect(readableFrame(frames, "low")?.id).toBe("low");
  });

  it("ignores a tablet frame that is not here", () => {
    expect(readableFrame(frames, "gone")?.id).toBe("top");
  });

  it("is null without frames", () => {
    expect(readableFrame([], null)).toBeNull();
  });
});

describe("readableView", () => {
  const viewport = { width: 1920, height: 1000 };

  it("fits the frame's width and starts at its top, not the whole tall frame", () => {
    const view = readableView(frame({ x: 100, y: 300 }), viewport);
    expect(view.scale).toBeCloseTo(1920 / 1440);
    expect(view.originX).toBeCloseTo(100);
    expect(view.originY).toBeCloseTo(300);
  });
});
