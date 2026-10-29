import { describe, expect, it } from "vitest";
import { parseMarks, placeMark } from "./marks";

const MARK = {
  id: "mk1",
  frame: "fr1",
  preset: "phone",
  status: "new",
  state: "click:#menu",
  strokes: ["s1", "s2"],
  rect: [10, 120, 80, 40],
};

describe("parseMarks", () => {
  it("reads the core's marks JSON, keeping a null rect as 'hidden'", () => {
    const hidden = { ...MARK, id: "mk2", rect: null };
    expect(parseMarks(JSON.stringify([MARK, hidden]))).toEqual([MARK, hidden]);
  });

  it("drops malformed entries and unknown statuses instead of breaking the overlay", () => {
    const marks = parseMarks(
      JSON.stringify([
        MARK,
        { ...MARK, id: 3 },
        { ...MARK, status: "archived" },
        { ...MARK, rect: [1, 2, 3] },
        { ...MARK, rect: [1, 2, 3, "x"] },
        { ...MARK, strokes: ["s1", 9] },
      ]),
    );
    expect(marks).toEqual([
      MARK,
      { ...MARK, rect: null },
      { ...MARK, rect: null },
      { ...MARK, strokes: ["s1"] },
    ]);
  });

  it("returns nothing for a broken or non-array payload", () => {
    expect(parseMarks("{")).toEqual([]);
    expect(parseMarks("{}")).toEqual([]);
  });
});

describe("placeMark", () => {
  it("maps a mark rect through the canvas view", () => {
    expect(placeMark([10, 20, 30, 40], { scale: 2, originX: 5, originY: 6 })).toEqual({
      left: 10,
      top: 28,
      width: 60,
      height: 80,
    });
  });
});
