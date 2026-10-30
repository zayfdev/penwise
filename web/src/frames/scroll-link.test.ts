import { describe, expect, it } from "vitest";
import {
  iframeOffset,
  PAGE_WINS_MS,
  ScrollFollower,
  scrollTarget,
  visibleDocRange,
} from "./scroll-link";

const stage = { width: 1200, height: 900 };
// A desktop frame: preset 1440 x 900, document 6000 tall, box at board (200, 100).
const frame = { x: 200, y: 100, cssH: 900, docH: 6000 };
const view = (originY: number, scale = 1) => ({ scale, originX: 0, originY });

describe("visibleDocRange", () => {
  it("is the part of the document on the stage", () => {
    expect(visibleDocRange(100, 6000, view(100), stage)).toEqual({ from: 0, to: 900 });
    expect(visibleDocRange(100, 6000, view(2100), stage)).toEqual({ from: 2000, to: 2900 });
    expect(visibleDocRange(100, 6000, view(2100, 0.5), stage)).toEqual({ from: 2000, to: 3800 });
  });

  it("is null when the box is off screen", () => {
    expect(visibleDocRange(100, 6000, view(7000), stage)).toBeNull();
    expect(visibleDocRange(5000, 600, view(0), stage)).toBeNull();
  });
});

describe("scrollTarget", () => {
  it("asks for nothing while the window already covers the visible part", () => {
    expect(scrollTarget(frame, view(100), stage, 0)).toBeNull();
    expect(scrollTarget(frame, view(1100), stage, 1000)).toBeNull();
    expect(scrollTarget(frame, view(1100 + 40, 2), stage, 1000)).toBeNull();
    // Shifted by 40 px at 1:1 the window no longer covers the bottom of the stage.
    expect(scrollTarget(frame, view(1100 + 40), stage, 1000)).toBe(1040);
  });

  it("scrolls the page to the camera after a pan elsewhere", () => {
    // The camera moved down by 2400 px; the page is still at the top.
    expect(scrollTarget(frame, view(2500), stage, 0)).toBe(2400);
  });

  it("centres a visible part smaller than the window", () => {
    // A zoom-in shows 450 doc px; the window is 900 tall, so it is centred on them.
    expect(scrollTarget(frame, view(2100, 2), stage, 0)).toBe(1775);
  });

  it("clamps to 0..docH - cssH", () => {
    expect(scrollTarget(frame, view(100 + 5900), stage, 0)).toBe(5100);
    expect(scrollTarget(frame, view(100 - 300), stage, 2000)).toBe(0);
  });

  it("leaves a frame that is off screen alone", () => {
    expect(scrollTarget(frame, view(9000), stage, 0)).toBeNull();
  });
});

describe("ScrollFollower", () => {
  it("makes the camera follow only the owner's scrolling", () => {
    const follower = new ScrollFollower();
    expect(follower.reported(0, "page", null, 0)).toBe(0);
    expect(follower.reported(120, "user", null, 10)).toBe(120);
    expect(follower.reported(150, "user", null, 20)).toBe(30);
    // The momentum after a fling is the owner's too (the bridge labels it so).
    expect(follower.reported(180, "user", null, 30)).toBe(30);
    expect(follower.reported(100, "user", null, 40)).toBe(-80);
  });

  it("moves only the window for scrolls the page makes: a reload restore, an anchor jump, scrollIntoView", () => {
    const follower = new ScrollFollower();
    follower.reported(0, "page", null, 0);
    expect(follower.reported(2400, "page", null, 10)).toBe(0);
    expect(follower.reported(0, "page", null, 20)).toBe(0);
  });

  it("ignores the scroll it asked for, matched by id, until it arrives", () => {
    const follower = new ScrollFollower();
    follower.reported(0, "page", null, 0);
    const id = follower.request(2400);
    // A smooth-scrolling page (`scroll-behavior: smooth`) reports steps on the way.
    expect(follower.reported(900, "request", id, 50)).toBe(0);
    expect(follower.reported(2400, "request", id, 100)).toBe(0);
    // Arrived: the owner's next scroll is followed again.
    expect(follower.reported(2500, "user", null, 300)).toBe(100);
  });

  it("does not take another request's scroll for its own", () => {
    const follower = new ScrollFollower();
    follower.reported(0, "page", null, 0);
    const old = follower.request(100);
    const id = follower.request(2400);
    expect(follower.reported(100, "request", old, 10)).toBe(0);
    expect(follower.reported(2400, "request", id, 20)).toBe(0);
    expect(follower.held(30)).toBe(false);
  });

  it("lets the page win when it moves away from the target itself (a Lenis-like page that lerps back)", () => {
    const follower = new ScrollFollower();
    follower.reported(0, "page", null, 0);
    const id = follower.request(2400);
    expect(follower.reported(2400, "request", id, 100)).toBe(0);
    // The page's smoothing pulls it back toward its own target: the window follows, the camera stays, and
    // nothing is asked of the page for a second.
    expect(follower.reported(2300, "page", null, 120)).toBe(0);
    expect(follower.held(130)).toBe(true);
    expect(follower.reported(2100, "page", null, 160)).toBe(0);
    expect(follower.held(160 + PAGE_WINS_MS - 1)).toBe(true);
    expect(follower.held(160 + PAGE_WINS_MS)).toBe(false);
  });

  it("the owner scrolling cancels a request that has not landed", () => {
    const follower = new ScrollFollower();
    follower.reported(0, "page", null, 0);
    const id = follower.request(2400);
    expect(follower.reported(40, "user", null, 10)).toBe(40);
    // The late echo of the abandoned request moves only the window.
    expect(follower.reported(2400, "request", id, 20)).toBe(0);
    expect(follower.held(30)).toBe(false);
  });

  it("starts over after a reload", () => {
    const follower = new ScrollFollower();
    follower.reported(500, "user", null, 0);
    follower.reset();
    expect(follower.reported(0, "user", null, 10)).toBe(0);
  });
});

describe("iframeOffset", () => {
  it("places the iframe at the page's reported scroll inside the box", () => {
    // The core's live window is pos + scroll: box at (200, 100), page scrolled 3000.
    expect(iframeOffset(200, 100, [200, 3100, 1440, 900])).toEqual({ left: 0, top: 3000 });
  });

  it("does not depend on the camera", () => {
    const window = [200, 100 + 750, 1440, 900];
    expect(iframeOffset(200, 100, window)).toEqual(iframeOffset(200, 100, window));
    expect(iframeOffset(200, 100, window).top).toBe(750);
  });

  it("sits at the box's top-left before a layout", () => {
    expect(iframeOffset(200, 100, null)).toEqual({ left: 0, top: 0 });
  });

  it("follows the rounded box origin", () => {
    expect(iframeOffset(200.4, 100.4, [200.4, 600.4, 1440, 900])).toEqual({ left: 0, top: 500 });
  });
});
