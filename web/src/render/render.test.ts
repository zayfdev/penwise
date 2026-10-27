import { describe, expect, it } from "vitest";
import {
  commandAt,
  commandCount,
  cssColor,
  type DrawData,
  EMPTY_DRAW,
  isEmpty,
  Kind,
  runRange,
  styleAt,
} from "./drawdata";
import { apply, mul, scaleOf, viewMatrix } from "./matrix";
import { wrapLines } from "./paint";
import { rulings, visibleRect } from "./paper";

describe("draw data accessors", () => {
  const data: DrawData = {
    ...EMPTY_DRAW,
    points: new Float32Array([0, 0, 1, 0, 1, 1, 5, 5, 6, 6]),
    runs: new Uint32Array([3, 5]),
    commands: new Uint32Array([Kind.PathNonZero, 0, 1, 0, 0, Kind.Mask, 1, 1, 0, 2]),
    styles: new Float32Array([1, 0.5, 0, 1, 0.8, 1, 0.3, 0.1]),
  };

  it("decodes commands, runs and styles by their strides", () => {
    expect(commandCount(data)).toBe(2);
    expect(commandAt(data, 1)).toEqual({ kind: Kind.Mask, first: 1, count: 1, style: 0, aux: 2 });
    expect(runRange(data, 0)).toEqual([0, 6]);
    expect(runRange(data, 1)).toEqual([6, 10]);
    const style = styleAt(data, 0);
    expect(style.multiply).toBe(true);
    expect(style.opacity).toBeCloseTo(0.8, 5);
    expect(style.grainScale).toBeCloseTo(0.3, 5);
    expect(cssColor(style)).toBe("rgb(255 128 0)");
    expect(isEmpty(data)).toBe(false);
    expect(isEmpty(EMPTY_DRAW)).toBe(true);
  });
});

describe("matrices", () => {
  it("viewMatrix maps page → device px with HiDPI", () => {
    const m = viewMatrix(0.5, 100, 40, 2);
    expect(apply(m, 100, 40)).toEqual([0, 0]);
    expect(apply(m, 120, 60)).toEqual([20, 20]);
    expect(scaleOf(m)).toBe(1);
  });
  it("mul applies the right operand first", () => {
    const scale = { a: 2, b: 0, c: 0, d: 2, e: 0, f: 0 };
    const shift = { a: 1, b: 0, c: 0, d: 1, e: 3, f: 0 };
    expect(apply(mul(scale, shift), 1, 1)).toEqual([8, 2]);
  });
});

describe("paper", () => {
  it("rulings are the multiples of the spacing strictly inside the range", () => {
    expect(rulings(0, 300, 100)).toEqual([100, 200]);
    expect(rulings(150, 450, 100)).toEqual([200, 300, 400]);
    expect(rulings(0, 50, 100)).toEqual([]);
  });
  it("visibleRect inverts the view matrix", () => {
    const m = viewMatrix(2, 10, 20, 1);
    expect(visibleRect(m, 200, 100)).toEqual([10, 20, 110, 70]);
  });
});

describe("wrapLines", () => {
  const measure = (s: string) => s.length;
  it("wraps greedily at the box width and keeps explicit newlines", () => {
    expect(wrapLines("aaa bbb ccc", 7, measure)).toEqual(["aaa bbb", "ccc"]);
    expect(wrapLines("a\nb", 10, measure)).toEqual(["a", "b"]);
    expect(wrapLines("longerword x", 3, measure)).toEqual(["longerword", "x"]);
  });
});
