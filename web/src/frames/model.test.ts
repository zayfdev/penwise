import { describe, expect, it } from "vitest";
import {
  diffFrames,
  type Frame,
  framesInteractive,
  frameTransform,
  frameUrl,
  intersects,
  isLoopback,
  normalizeRoute,
  parseFrames,
  placeFrame,
  presetSize,
  readProjects,
} from "./model";

const view = { scale: 2, originX: 10, originY: 20 };

const frame = (over: Partial<Frame> = {}): Frame => ({
  id: "fr_1",
  project: "prj_a",
  route: "/",
  preset: "phone",
  cssW: 390,
  cssH: 844,
  x: 10,
  y: 20,
  ...over,
});

describe("frameUrl", () => {
  it("loads the project port directly on loopback", () => {
    expect(frameUrl("127.0.0.1", 7421, "/", "tok")).toBe("http://127.0.0.1:7421/");
    expect(frameUrl("localhost", 7430, "/work?x=1", "tok")).toBe("http://localhost:7430/work?x=1");
  });

  it("goes through the enter route off loopback", () => {
    const url = new URL(frameUrl("192.168.1.5", 7421, "/work", "tok"));
    expect(url.origin).toBe("http://192.168.1.5:7421");
    expect(url.pathname).toBe("/__penwise/enter");
    expect(url.searchParams.get("token")).toBe("tok");
    expect(url.searchParams.get("to")).toBe("/work");
  });

  it("brackets a bare IPv6 host", () => {
    expect(frameUrl("::1", 7421, "/", "")).toBe("http://[::1]:7421/");
    expect(frameUrl("[::1]", 7421, "/", "")).toBe("http://[::1]:7421/");
  });

  it("adds the leading slash a route may miss", () => {
    expect(normalizeRoute("work")).toBe("/work");
    expect(normalizeRoute("   ")).toBe("/");
    expect(frameUrl("127.0.0.1", 7421, "work", "")).toBe("http://127.0.0.1:7421/work");
  });

  it("knows the loopback hosts", () => {
    for (const host of ["127.0.0.1", "127.1.2.3", "localhost", "::1", "[::1]", "LOCALHOST"]) {
      expect(isLoopback(host)).toBe(true);
    }
    expect(isLoopback("192.168.1.5")).toBe(false);
    expect(isLoopback("penwise.local")).toBe(false);
  });
});

describe("parseFrames", () => {
  it("reads the contract JSON", () => {
    const frames = parseFrames(
      JSON.stringify([
        {
          id: "ab12",
          project: "prj_a",
          route: "/work",
          preset: "desktop",
          cssW: 1440,
          cssH: 900,
          rect: [100, 200, 1440, 900],
        },
      ]),
    );
    expect(frames).toEqual([
      {
        id: "ab12",
        project: "prj_a",
        route: "/work",
        preset: "desktop",
        cssW: 1440,
        cssH: 900,
        x: 100,
        y: 200,
      },
    ]);
  });

  it("drops malformed entries instead of failing the page", () => {
    const frames = parseFrames(
      JSON.stringify([
        null,
        { id: "" },
        { id: "x", project: "p", route: "/", rect: [0, 0, 0, 0] },
        { id: "y", project: "p", route: "/", cssW: 10, cssH: 10, rect: [1, 2, 10, 10] },
      ]),
    );
    expect(frames.map((f) => f.id)).toEqual(["y"]);
    expect(parseFrames("not json")).toEqual([]);
    expect(parseFrames("{}")).toEqual([]);
  });
});

describe("diffFrames", () => {
  it("only reports what changed", () => {
    const delta = diffFrames(
      [frame(), frame({ id: "fr_2", x: 300 }), frame({ id: "fr_gone" })],
      [frame({ route: "/work" }), frame({ id: "fr_2", x: 300 }), frame({ id: "fr_3" })],
    );
    expect(delta.added.map((f) => f.id)).toEqual(["fr_3"]);
    expect(delta.updated.map((f) => f.id)).toEqual(["fr_1"]);
    expect(delta.removed).toEqual(["fr_gone"]);
  });

  it("does not report an unchanged frame", () => {
    const delta = diffFrames([frame()], [frame()]);
    expect(delta).toEqual({ added: [], updated: [], removed: [] });
  });
});

describe("placement", () => {
  it("maps page units through the view", () => {
    expect(placeFrame(frame(), view)).toEqual({ left: 0, top: 0, width: 780, height: 1688 });
    expect(placeFrame(frame({ x: 110, y: 70 }), view)).toEqual({
      left: 200,
      top: 100,
      width: 780,
      height: 1688,
    });
  });

  it("writes the same mapping as the wrapper transform", () => {
    // translate(-origin · scale) then scale(scale) puts page (originX, originY) on stage (0, 0).
    expect(frameTransform(view)).toBe("translate(-20px, -40px) scale(2)");
  });

  it("tells whether a frame is on screen", () => {
    const box = placeFrame(frame(), { scale: 1, originX: 0, originY: 0 });
    expect(intersects(box, { width: 500, height: 900 })).toBe(true);
    expect(intersects({ ...box, left: 900 }, { width: 500, height: 900 })).toBe(false);
    expect(intersects({ ...box, left: -400 }, { width: 500, height: 900 })).toBe(false);
    expect(intersects({ ...box, left: 900 }, { width: 0, height: 0 })).toBe(true);
  });
});

describe("framesInteractive", () => {
  it("lets mouse and touch through unless a pen, draw-with-mouse or a modifier needs the stage", () => {
    expect(framesInteractive("mouse", false, false)).toBe(true);
    expect(framesInteractive("touch", false, false)).toBe(true);
    expect(framesInteractive("pen", false, false)).toBe(false);
    expect(framesInteractive("mouse", true, false)).toBe(false);
    expect(framesInteractive("mouse", false, true)).toBe(false);
  });
});

describe("presetSize", () => {
  it("knows the three presets and nothing else", () => {
    expect(presetSize("phone")).toEqual({ cssW: 390, cssH: 844 });
    expect(presetSize("tablet")).toEqual({ cssW: 834, cssH: 1194 });
    expect(presetSize("desktop")).toEqual({ cssW: 1440, cssH: 900 });
    expect(presetSize("custom")).toBeNull();
  });
});

describe("readProjects", () => {
  it("keeps the projects a frame URL can use", () => {
    expect(
      readProjects([
        { id: "prj_a", name: "Site", port: 7423, kind: "static", root: "/srv/site" },
        { id: "prj_b", port: 0 },
        { name: "no id", port: 7424 },
      ]),
    ).toEqual([
      {
        id: "prj_a",
        name: "Site",
        root: "/srv/site",
        kind: "static",
        origin: "",
        port: 7423,
        notebook: null,
      },
    ]);
    expect(readProjects({ projects: [] })).toEqual([]);
  });
});
