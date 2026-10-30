import { describe, expect, it } from "vitest";
import type { Frame, Project } from "./model";
import { framesAreStale, renderableFrames } from "./refresh";

const frame = (over: Partial<Frame> = {}): Frame => ({
  id: "fr_1",
  project: "prj_a",
  route: "/",
  preset: "phone",
  cssW: 390,
  cssH: 844,
  extent: "full",
  docH: 0,
  boxH: 844,
  hex: "0123456789abcdef0123456789abcdef",
  x: 0,
  y: 0,
  patch: null,
  label: "",
  group: null,
  chosen: false,
  ...over,
});

const project = (id: string): Project => ({
  id,
  name: id,
  root: "/srv/site",
  kind: "static",
  origin: "",
  port: 7421,
  notebook: null,
});

describe("framesAreStale", () => {
  it("keeps the last complete list while a snapshot empties the shown page", () => {
    expect(framesAreStale("pg", "pg", 2, true)).toBe(true);
  });

  it("never keeps frames across a page switch, and trusts a finished snapshot", () => {
    expect(framesAreStale("pg2", "pg", 2, true)).toBe(false);
    expect(framesAreStale("pg", "pg", 2, false)).toBe(false);
    expect(framesAreStale("pg", "pg", 0, true)).toBe(false);
    expect(framesAreStale(null, null, 0, true)).toBe(false);
  });
});

describe("renderableFrames", () => {
  it("keeps frames with a known project", () => {
    const frames = [frame(), frame({ id: "fr_2", project: "prj_b" })];
    expect(renderableFrames(frames, [project("prj_a"), project("prj_b")], [])).toEqual(frames);
  });

  it("keeps an on-screen frame whose project is momentarily unknown, so its iframe is not reloaded", () => {
    const frames = [frame()];
    expect(renderableFrames(frames, [], ["fr_1"])).toEqual(frames);
    expect(renderableFrames(frames, [project("prj_other")], new Set(["fr_1"]))).toEqual(frames);
  });

  it("waits to create a frame whose project is not known yet", () => {
    expect(renderableFrames([frame()], [], [])).toEqual([]);
  });
});
