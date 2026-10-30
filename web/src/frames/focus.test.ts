import { describe, expect, it } from "vitest";
import { FOCUS_TAKEOVER, type FocusBox, type FocusInput, LiveSet, pickFocus } from "./focus";

const viewport = { width: 1000, height: 800 };
const at1 = { scale: 1, originX: 0, originY: 0 };

const box = (id: string, x: number, y: number, w: number, h: number): FocusBox => ({
  id,
  x,
  y,
  w,
  h,
});

const input = (over: Partial<FocusInput> & Pick<FocusInput, "boxes">): FocusInput => ({
  view: at1,
  viewport,
  current: null,
  pinned: null,
  ...over,
});

describe("pickFocus", () => {
  it("focuses nothing until a frame is 60 % of the stage wide", () => {
    const frame = box("a", 0, 0, 590, 900);
    expect(pickFocus(input({ boxes: [frame] }))).toBeNull();
    expect(pickFocus(input({ boxes: [box("a", 0, 0, 600, 900)] }))).toBe("a");
  });

  it("measures the width on screen, so zooming decides", () => {
    const frames = [box("a", 0, 0, 390, 844)];
    expect(pickFocus(input({ boxes: frames }))).toBeNull();
    expect(pickFocus(input({ boxes: frames, view: { scale: 2, originX: 0, originY: 0 } }))).toBe(
      "a",
    );
  });

  it("takes the frame that covers most of the stage", () => {
    const a = box("a", 0, 0, 800, 300);
    const b = box("b", 0, 300, 800, 500);
    expect(pickFocus(input({ boxes: [a, b] }))).toBe("b");
    // Scrolled so that a long frame's lower part fills the stage, it is still the one.
    const tall = box("tall", 0, 0, 800, 5000);
    expect(
      pickFocus(input({ boxes: [tall, b], view: { scale: 1, originX: 0, originY: 2000 } })),
    ).toBe("tall");
  });

  it("ignores frames that are off screen", () => {
    expect(pickFocus(input({ boxes: [box("a", 5000, 5000, 900, 900)] }))).toBeNull();
  });

  it("keeps the focused frame down to 50 % width (hysteresis)", () => {
    const frame = box("a", 0, 0, 550, 900);
    expect(pickFocus(input({ boxes: [frame] }))).toBeNull();
    expect(pickFocus(input({ boxes: [frame], current: "a" }))).toBe("a");
    expect(pickFocus(input({ boxes: [box("a", 0, 0, 450, 900)], current: "a" }))).toBeNull();
  });

  it("makes a challenger cover a quarter more before it takes over", () => {
    const a = box("a", 0, 0, 800, 400);
    // An equal neighbour is not enough to take the focus.
    const equal = box("b", 0, 400, 800, 400);
    expect(pickFocus(input({ boxes: [a, equal], current: "a" }))).toBe("a");
    const wide = box("b", 0, 400, 1000, 400);
    // 1000x400 vs 800x400 = 1.25x: exactly the takeover factor.
    expect(wide.w * wide.h).toBeGreaterThanOrEqual(a.w * a.h * FOCUS_TAKEOVER);
    expect(pickFocus(input({ boxes: [a, wide], current: "a" }))).toBe("b");
  });

  it("lets an explicit focus win while its frame stays half the stage wide", () => {
    const pinned = box("a", 0, 0, 800, 100);
    const bigger = box("b", 0, 100, 1000, 700);
    expect(pickFocus(input({ boxes: [pinned, bigger] }))).toBe("b");
    expect(pickFocus(input({ boxes: [pinned, bigger], pinned: "a" }))).toBe("a");
    // Narrower than half the stage, the pin lets go.
    expect(pickFocus(input({ boxes: [box("a", 0, 0, 450, 100), bigger], pinned: "a" }))).toBe("b");
  });
});

describe("LiveSet", () => {
  it("keeps one live frame, the previous one hidden, and drops older ones", () => {
    const set = new LiveSet();
    expect(set.advance("a")).toEqual({ live: "a", warm: null, drop: [] });
    expect(set.advance("b")).toEqual({ live: "b", warm: "a", drop: [] });
    expect(set.advance("c")).toEqual({ live: "c", warm: "b", drop: ["a"] });
  });

  it("returns to the previous frame without dropping anything", () => {
    const set = new LiveSet();
    set.advance("a");
    set.advance("b");
    expect(set.advance("a")).toEqual({ live: "a", warm: "b", drop: [] });
  });

  it("board view hides the live frame but keeps it for the way back", () => {
    const set = new LiveSet();
    set.advance("a");
    set.advance("b");
    expect(set.advance(null)).toEqual({ live: null, warm: "b", drop: ["a"] });
    expect(set.advance(null)).toEqual({ live: null, warm: "b", drop: [] });
    expect(set.advance("b")).toEqual({ live: "b", warm: null, drop: [] });
  });

  it("forgets a deleted frame", () => {
    const set = new LiveSet();
    set.advance("a");
    set.advance("b");
    expect(set.forget("b")).toEqual({ live: null, warm: "a", drop: ["b"] });
    expect(set.forget("a")).toEqual({ live: null, warm: null, drop: ["a"] });
  });
});
