import { describe, expect, it } from "vitest";
import { BridgeChannel } from "./bridge-link";
import type { Hit, Layout } from "./bridge-messages";

/** A window that records what the channel posts. */
function fakeWindow() {
  const posted: { t: string }[] = [];
  const window = {
    postMessage: (message: { t: string }) => posted.push(message),
  };
  // The channel only calls `postMessage`; the cast stands in for the iframe's Window.
  return { window: window as unknown as Window, posted };
}

const LAYOUT: Layout = {
  scroll: [0, 40],
  doc: [390, 2000],
  viewport: [390, 844],
  state: "",
  rects: { a: [10, 20, 30, 40] },
};

describe("BridgeChannel", () => {
  it("posts the watched keys and re-posts them after a new document", () => {
    const { window, posted } = fakeWindow();
    const channel = new BridgeChannel(() => window);
    channel.watch(["a", "b"]);
    expect(posted).toEqual([{ t: "pw:watch", keys: ["a", "b"] }]);
    channel.loaded();
    expect(posted).toEqual([
      { t: "pw:watch", keys: ["a", "b"] },
      { t: "pw:watch", keys: ["a", "b"] },
    ]);
    expect(channel.layout()).toBeNull();
  });

  it("posts a hit request and resolves it once by id", () => {
    const { window, posted } = fakeWindow();
    const channel = new BridgeChannel(() => window);
    const answers: Hit[] = [];
    channel.hit(12, 34, (hit) => answers.push(hit));
    expect(posted).toEqual([{ t: "pw:hit?", id: 1, x: 12, y: 34 }]);
    channel.answer({ t: "hit", id: 1, key: "v1|s|h1", rect: [1, 2, 3, 4] });
    channel.answer({ t: "hit", id: 1, key: "stale", rect: null });
    expect(answers).toEqual([{ key: "v1|s|h1", rect: [1, 2, 3, 4] }]);
  });

  it("answers a hit as gone when the frame is gone", () => {
    const { window } = fakeWindow();
    const channel = new BridgeChannel(() => window);
    const answers: Hit[] = [];
    channel.hit(1, 2, (hit) => answers.push(hit));
    channel.close();
    expect(answers).toEqual([{ key: null, rect: null }]);
    const absent: Hit[] = [];
    new BridgeChannel(() => null).hit(1, 2, (hit) => absent.push(hit));
    expect(absent).toEqual([{ key: null, rect: null }]);
  });

  it("keeps a hit rect merged into the stored layout so the next send carries it", () => {
    const { window } = fakeWindow();
    const channel = new BridgeChannel(() => window);
    expect(channel.mergeRects({ a: [1, 1, 1, 1] })).toBeNull();
    channel.store(LAYOUT);
    const merged = channel.mergeRects({ a: [50, 60, 30, 40], b: null });
    expect(merged?.rects).toEqual({ a: [50, 60, 30, 40], b: null });
    expect(merged?.state).toBe("");
    expect(channel.layout()?.rects).toEqual({ a: [50, 60, 30, 40], b: null });
  });

  it("posts a replay recipe and reads the current state", () => {
    const { window, posted } = fakeWindow();
    const channel = new BridgeChannel(() => window);
    channel.store({ ...LAYOUT, state: "click:#menu" });
    expect(channel.state()).toBe("click:#menu");
    channel.replay("click:#menu");
    expect(posted).toEqual([{ t: "pw:replay", state: "click:#menu" }]);
  });
});
