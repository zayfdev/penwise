import { describe, expect, it } from "vitest";
import {
  bindingJson,
  decodeBridge,
  isFromFrame,
  type Layout,
  layoutJson,
  patchFor,
  patchMessage,
  readHit,
  scrollToMessage,
} from "./bridge-messages";

const LAYOUT: Layout = {
  scroll: [0, 120],
  doc: [390, 2400],
  viewport: [390, 844],
  state: "click:#menu",
  cause: "page",
  requestId: null,
  rects: { "v1|a|h1|…": [10, 20, 300, 80], "v1||gone|…": null },
};

describe("decodeBridge", () => {
  it("decodes a full layout with per-key rects and an explicit null", () => {
    const message = decodeBridge({
      t: "pw:layout",
      scroll: [0, 120],
      doc: [390, 2400],
      viewport: [390, 844],
      state: "click:#menu",
      rects: { "v1|a|h1|…": [10, 20, 300, 80], "v1||gone|…": null },
    });
    expect(message).toEqual({ t: "layout", layout: LAYOUT });
  });

  it("defaults a missing state to '', drops malformed rects and accepts the string form", () => {
    const row = {
      t: "pw:layout",
      scroll: [1, 2],
      doc: [3, 4],
      viewport: [5, 6],
      rects: { good: [1, 2, 3, 4], short: [1, 2], text: ["a", 2, 3, 4] },
    };
    const message = decodeBridge(JSON.stringify(row));
    expect(message).toEqual({
      t: "layout",
      layout: {
        scroll: [1, 2],
        doc: [3, 4],
        viewport: [5, 6],
        state: "",
        cause: "page",
        requestId: null,
        rects: { good: [1, 2, 3, 4] },
      },
    });
  });

  it("reads the scroll's cause and the request it answers; an unknown cause is the page's", () => {
    const row = { t: "pw:layout", scroll: [0, 9], doc: [1, 1], viewport: [1, 1], rects: {} };
    const layoutOf = (extra: object) => {
      const message = decodeBridge({ ...row, ...extra });
      return message?.t === "layout" ? message.layout : null;
    };
    expect(layoutOf({ cause: "user" })).toMatchObject({ cause: "user", requestId: null });
    expect(layoutOf({ cause: "request", id: 4 })).toMatchObject({ cause: "request", requestId: 4 });
    expect(layoutOf({ cause: "weird" })).toMatchObject({ cause: "page" });
    expect(layoutOf({})).toMatchObject({ cause: "page" });
  });

  it("drops a layout missing a required array or carrying non-finite numbers", () => {
    expect(decodeBridge({ t: "pw:layout", scroll: [0, 0], doc: [1, 1] })).toBeNull();
    expect(
      decodeBridge({
        t: "pw:layout",
        scroll: [0, Number.NaN],
        doc: [1, 1],
        viewport: [1, 1],
      }),
    ).toBeNull();
  });

  it("decodes a hit answer with its element rect, treating an empty key as no element", () => {
    expect(decodeBridge({ t: "pw:hit", id: 7, key: "v1|src|div", rect: [10, 20, 30, 40] })).toEqual(
      { t: "hit", id: 7, key: "v1|src|div", rect: [10, 20, 30, 40] },
    );
    expect(decodeBridge({ t: "pw:hit", id: 7, key: "v1|src|div" })).toEqual({
      t: "hit",
      id: 7,
      key: "v1|src|div",
      rect: null,
    });
    expect(decodeBridge({ t: "pw:hit", id: 7, key: null, rect: null })).toEqual({
      t: "hit",
      id: 7,
      key: null,
      rect: null,
    });
    expect(decodeBridge({ t: "pw:hit", id: 7, key: "" })).toEqual({
      t: "hit",
      id: 7,
      key: null,
      rect: null,
    });
    expect(decodeBridge({ t: "pw:hit", key: "v1|src|div" })).toBeNull();
  });

  it("decodes hello and a patch result with its failed ops", () => {
    expect(decodeBridge({ t: "pw:hello", url: "/" })).toEqual({ t: "hello" });
    expect(
      decodeBridge({
        t: "pw:patched",
        ok: false,
        failed: [{ index: 0, reason: "bad selector" }, { index: "x" }, { index: 2 }],
      }),
    ).toEqual({
      t: "patched",
      ok: false,
      failed: [
        { index: 0, reason: "bad selector" },
        { index: 2, reason: "" },
      ],
    });
    expect(decodeBridge({ t: "pw:patched", ok: true })).toEqual({
      t: "patched",
      ok: true,
      failed: [],
    });
  });

  it("decodes the replay answer and ignores what the client does not handle", () => {
    expect(decodeBridge({ t: "pw:replayed", state: "click:#menu", ok: true })).toEqual({
      t: "replayed",
      state: "click:#menu",
      ok: true,
    });
    expect(decodeBridge({ t: "pw:scroll", x: 0, y: 0 })).toBeNull();
    expect(decodeBridge({ t: "pw:route", route: "/x" })).toBeNull();
    expect(decodeBridge("not json")).toBeNull();
    expect(decodeBridge(null)).toBeNull();
    expect(decodeBridge([])).toBeNull();
  });
});

describe("patch messages", () => {
  const PATCH = '{"css":"h1{color:red}"}';

  it("applies the patch while the ghost is on and reverts while it is off", () => {
    expect(patchFor(PATCH, true)).toEqual({ patch: PATCH, on: true });
    expect(patchFor(PATCH, false)).toEqual({ patch: PATCH, on: false });
  });

  it("sends nothing for a plain frame", () => {
    expect(patchFor(null, true)).toBeNull();
  });

  it("builds the bridge message", () => {
    expect(patchMessage(PATCH, false)).toEqual({ t: "pw:patch", patch: PATCH, on: false });
  });
});

describe("isFromFrame", () => {
  it("accepts only the frame's own window", () => {
    const frame = { window: "frame" };
    expect(isFromFrame(frame, frame)).toBe(true);
    expect(isFromFrame({ window: "other" }, frame)).toBe(false);
    expect(isFromFrame(frame, null)).toBe(false);
    expect(isFromFrame(undefined, undefined)).toBe(false);
  });
});

describe("layoutJson", () => {
  it("adds the frame preset to the bridge reading for the core", () => {
    expect(JSON.parse(layoutJson(LAYOUT, "phone"))).toEqual({
      preset: "phone",
      scroll: [0, 120],
      doc: [390, 2400],
      viewport: [390, 844],
      state: "click:#menu",
      rects: { "v1|a|h1|…": [10, 20, 300, 80], "v1||gone|…": null },
    });
  });
});

describe("bindingJson", () => {
  it("binds to the element when the bridge resolved a key", () => {
    expect(JSON.parse(bindingJson("frame", "bound", "v1|src|h1") ?? "")).toEqual({
      frame: "frame",
      mode: "bound",
      key: "v1|src|h1",
    });
  });

  it("falls back to the area when a bound stroke never got a key", () => {
    expect(JSON.parse(bindingJson("frame", "bound", null) ?? "")).toEqual({
      frame: "frame",
      mode: "area",
    });
    expect(JSON.parse(bindingJson("frame", "bound", "") ?? "")).toEqual({
      frame: "frame",
      mode: "area",
    });
  });

  it("pins to the area when the owner chose that mode, and returns null off a frame", () => {
    expect(JSON.parse(bindingJson("frame", "area", "v1|src|h1") ?? "")).toEqual({
      frame: "frame",
      mode: "area",
    });
    expect(bindingJson(null, "bound", "v1|src|h1")).toBeNull();
    expect(bindingJson("", "area", null)).toBeNull();
  });
});

describe("readHit", () => {
  it("reads the bridge's and the server's hit shape", () => {
    expect(readHit({ key: "v1|a", selector: "a", rect: [1, 2, 3, 4] })).toEqual({
      key: "v1|a",
      rect: [1, 2, 3, 4],
    });
    expect(readHit({ key: "", rect: [1, 2, 3, 4] })).toEqual({ key: null, rect: null });
    expect(readHit({ key: "k", rect: [1, 2] })).toEqual({ key: "k", rect: null });
    expect(readHit("nope")).toEqual({ key: null, rect: null });
  });
});

describe("scrollToMessage", () => {
  it("is the bridge's pw:scrollTo", () => {
    expect(scrollToMessage(0, 2400, 7)).toEqual({ t: "pw:scrollTo", x: 0, y: 2400, id: 7 });
  });
});
