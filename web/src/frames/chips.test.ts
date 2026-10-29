import { describe, expect, it } from "vitest";
import { type ChipBox, liftAbovePlaced } from "./chips";

const at = (left: number, top: number, width = 200): ChipBox => ({ left, top, width });

describe("liftAbovePlaced", () => {
  it("leaves a chip that overlaps nothing where it is", () => {
    expect(liftAbovePlaced(at(0, 10), 30, [at(300, 10)])).toEqual(at(0, 10));
    expect(liftAbovePlaced(at(0, 10), 30, [])).toEqual(at(0, 10));
  });

  it("lifts a chip clear of a neighbour it would cover (variant frames side by side)", () => {
    expect(liftAbovePlaced(at(180, 10), 30, [at(0, 10)])).toEqual(at(180, -20));
  });

  it("keeps lifting until it is clear of a stack", () => {
    const placed = [at(0, 10), at(0, -20)];
    expect(liftAbovePlaced(at(150, 10), 30, placed)).toEqual(at(150, -50));
  });

  it("does not treat a chip that ends before it starts as overlapping", () => {
    expect(liftAbovePlaced(at(200, 10), 30, [at(0, 10, 200)])).toEqual(at(200, 10));
  });
});
