import { describe, expect, it } from "vitest";
import {
  boxNearScreen,
  parseStill,
  type Strip,
  stillLayoutJson,
  stripsToShow,
  stripWidth,
  withToken,
  withWidth,
} from "./still";

const body = {
  version: 7,
  doc: [1440, 9000],
  viewport: [1440, 900],
  strips: [
    { y: 0, h: 4096, url: "/frames/f1/still/7/0.png" },
    { y: 4096, h: 4096, url: "/frames/f1/still/7/4096.png" },
    { y: 8192, h: 808, url: "/frames/f1/still/7/8192.png" },
  ],
  layout: { rects: { "v1|a": [10, 5000, 200, 40], "v1|gone": null }, state: "menu-open" },
};

describe("parseStill", () => {
  it("reads the contract JSON", () => {
    const still = parseStill(body);
    expect(still?.version).toBe("7");
    expect(still?.doc).toEqual([1440, 9000]);
    expect(still?.strips).toHaveLength(3);
    expect(still?.rects).toEqual({ "v1|a": [10, 5000, 200, 40], "v1|gone": null });
    expect(still?.state).toBe("menu-open");
  });

  it("drops what cannot be shown", () => {
    expect(parseStill(null)).toBeNull();
    expect(parseStill({ ...body, doc: [1, "x"] })).toBeNull();
    expect(parseStill({ ...body, version: undefined })).toBeNull();
    const rough = parseStill({
      ...body,
      strips: [null, { y: 0, h: 0, url: "/x" }, { y: 0, h: 10, url: "" }, body.strips[0]],
      layout: { rects: { bad: [1, 2], ok: [1, 2, 3, 4] } },
    });
    expect(rough?.strips).toHaveLength(1);
    expect(rough?.rects).toEqual({ ok: [1, 2, 3, 4] });
    expect(rough?.state).toBe("");
  });
});

describe("stripsToShow", () => {
  const strips: Strip[] = (parseStill(body)?.strips ?? []).slice();
  const urls = (list: readonly Strip[]) => list.map((strip) => strip.y);

  it("picks the strips that overlap the visible range", () => {
    expect(urls(stripsToShow(strips, { from: 0, to: 900 }, null, 0))).toEqual([0]);
    expect(urls(stripsToShow(strips, { from: 4000, to: 4200 }, null, 0))).toEqual([0, 4096]);
    expect(urls(stripsToShow(strips, { from: 8300, to: 9000 }, null, 0))).toEqual([8192]);
  });

  it("widens the range by the margin so a small pan shows no gap", () => {
    expect(urls(stripsToShow(strips, { from: 3500, to: 4000 }, null, 0))).toEqual([0]);
    expect(urls(stripsToShow(strips, { from: 3500, to: 4000 }, null, 1000))).toEqual([0, 4096]);
  });

  it("takes everything when nothing is known to be off screen", () => {
    expect(stripsToShow(strips, null, null, 0)).toHaveLength(3);
  });

  it("leaves out strips entirely under the live window", () => {
    const covered = { from: 0, to: 4200 };
    expect(urls(stripsToShow(strips, { from: 0, to: 9000 }, covered, 0))).toEqual([4096, 8192]);
    // A strip only partly under the window is still needed for the rest.
    expect(urls(stripsToShow(strips, { from: 0, to: 9000 }, { from: 100, to: 4000 }, 0))).toEqual([
      0, 4096, 8192,
    ]);
  });
});

describe("withToken", () => {
  it("appends the token as a query parameter", () => {
    expect(withToken("/frames/f1/still/7/0.png", "t k")).toBe(
      "/frames/f1/still/7/0.png?token=t%20k",
    );
    expect(withToken("/a.png?x=1", "t")).toBe("/a.png?x=1&token=t");
    expect(withToken("/a.png", "")).toBe("/a.png");
  });
});

describe("stillLayoutJson", () => {
  it("is a layout at scroll zero, so document rects are also viewport rects", () => {
    const still = parseStill(body);
    if (!still) throw new Error("no still");
    const layout = JSON.parse(stillLayoutJson(still, "desktop"));
    expect(layout).toMatchObject({
      preset: "desktop",
      scroll: [0, 0],
      doc: [1440, 9000],
      viewport: [1440, 900],
      state: "menu-open",
    });
    expect(layout.rects["v1|a"]).toEqual([10, 5000, 200, 40]);
    expect(layout.rects["v1|gone"]).toBeNull();
  });
});

describe("stripWidth", () => {
  it("asks only for the server's widths: 360, 720, 1440", () => {
    // A 1440 px frame at 0.2 on a 1x screen needs 288 px: the 360 bucket.
    expect(stripWidth(1440, 0.2, 1)).toBe(360);
    expect(stripWidth(1440, 0.4, 1)).toBe(720);
    expect(stripWidth(1440, 0.25, 2)).toBe(720);
    expect(stripWidth(1440, 0.01, 1)).toBe(360);
    for (const scale of [0.05, 0.1, 0.2, 0.3, 0.45, 0.6, 0.8, 1, 2]) {
      expect([null, 360, 720, 1440]).toContain(stripWidth(1440, scale, 1));
    }
  });

  it("asks for the full strip when a smaller one would not help", () => {
    expect(stripWidth(1440, 0.6, 1)).toBeNull();
    expect(stripWidth(1440, 1, 1)).toBeNull();
    expect(stripWidth(1440, 2.9, 1)).toBeNull();
    // A phone is 390 px wide: even the 360 strip saves nothing.
    expect(stripWidth(390, 0.5, 1)).toBeNull();
    expect(stripWidth(0, 1, 1)).toBeNull();
  });
});

describe("withWidth", () => {
  it("appends the width to the url as given", () => {
    expect(withWidth("/frames/f/still/7/0.png", 384)).toBe("/frames/f/still/7/0.png?w=384");
    expect(withWidth("/a.png?x=1", 256)).toBe("/a.png?x=1&w=256");
    expect(withWidth("/a.png", null)).toBe("/a.png");
  });
});

describe("boxNearScreen", () => {
  const stage = { width: 1000, height: 800 };
  const box = (left: number, top: number) => ({ left, top, width: 400, height: 3000 });

  it("keeps boxes on or near the stage", () => {
    expect(boxNearScreen(box(100, 100), stage, 800)).toBe(true);
    expect(boxNearScreen(box(1500, 100), stage, 800)).toBe(true);
    expect(boxNearScreen(box(100, -3500), stage, 800)).toBe(true);
  });

  it("releases boxes beyond the margin, in any direction", () => {
    expect(boxNearScreen(box(2000, 100), stage, 800)).toBe(false);
    expect(boxNearScreen(box(-1300, 100), stage, 800)).toBe(false);
    expect(boxNearScreen(box(100, -4000), stage, 800)).toBe(false);
    expect(boxNearScreen(box(100, 1700), stage, 800)).toBe(false);
  });

  it("keeps everything while the stage size is unknown", () => {
    expect(boxNearScreen(box(9000, 9000), { width: 0, height: 0 }, 0)).toBe(true);
  });
});
