import { describe, expect, it } from "vitest";
import type { Layout } from "./bridge-messages";
import { LIVE_POLL_MS, LiveCoordinator, type LiveDeps, WARM_KEEP_MS } from "./live";
import type { Frame } from "./model";
import { parseStill, type Still } from "./still";
import type { StillReply } from "./still-store";

const frame = (id: string, x: number): Frame => ({
  id,
  hex: id.padEnd(32, "0"),
  project: "p",
  route: "/",
  preset: "phone",
  cssW: 390,
  cssH: 844,
  extent: "full",
  docH: 0,
  boxH: 844,
  x,
  y: 0,
  patch: null,
  label: "",
  group: null,
  chosen: false,
});

const still = parseStill({
  version: 3,
  doc: [390, 3000],
  viewport: [390, 844],
  strips: [{ y: 0, h: 3000, url: "/s.png" }],
  layout: { rects: { k: [1, 2, 3, 4] }, state: "s" },
});

/** Records what the coordinator does to the core and the layer. */
function rig(reply: StillReply = { kind: "none" }) {
  const calls: string[] = [];
  const fetched: string[][] = [[]];
  let live: string | null = null;
  const reported = new Map<string, Layout>();
  const timers = new Map<number, { ms: number; run: () => void }>();
  let nextTimer = 1;
  const frames = [frame("a", 0), frame("b", 500)];
  const deps: LiveDeps = {
    layer: {
      liveId: () => live,
      layoutOf: (id) => reported.get(id) ?? null,
      setLive: (plan) => {
        live = plan.live;
        calls.push(`setLive ${plan.live} ${plan.warm}`);
      },
      placeWindow: () => undefined,
      renderStills: () => undefined,
      setStill: (id, s) => calls.push(`setStill ${id} ${s === null ? "none" : "still"}`),
      scrollTo: () => undefined,
    },
    core: {
      clear_frame_layout: (id) => {
        calls.push(`clear ${id}`);
        return true;
      },
      set_frame_layout: (id, json) => {
        const layout = JSON.parse(json) as { state: string; scroll: number[] };
        calls.push(`set ${id} ${layout.state} @${layout.scroll[1]}`);
        return true;
      },
      frame_live_window: () => new Float32Array([0, 0, 390, 844]),
    },
    frames: () => frames,
    view: () => ({ scale: 1, originX: 0, originY: 0 }),
    viewport: () => ({ width: 1000, height: 800 }),
    fetchStill: async (hex) => {
      fetched[fetched.length - 1]?.push(hex.slice(0, 1));
      return reply;
    },
    timers: {
      set: (run, ms) => {
        timers.set(nextTimer, { ms, run });
        return nextTimer++;
      },
      clear: (handle) => {
        timers.delete(handle);
      },
    },
    reread: () => calls.push("reread"),
    layoutChanged: () => undefined,
    problem: (message) => calls.push(`problem ${message}`),
  };
  const coordinator = new LiveCoordinator(deps);
  return { coordinator, calls, reported, timers, fetched };
}

const LAYOUT: Layout = {
  scroll: [0, 2000],
  doc: [390, 4000],
  viewport: [390, 844],
  state: "live",
  cause: "page",
  requestId: null,
  rects: {},
};

describe("LiveCoordinator demotion", () => {
  it("keeps the live layout of a frame that has no still yet (rects are document coords)", () => {
    const { coordinator, calls, reported } = rig();
    coordinator.focusFrame("a");
    reported.set("a", LAYOUT);
    calls.length = 0;
    coordinator.focusFrame("b");
    // `a` goes warm: neither cleared nor set, so a mark below the first screen keeps its box.
    expect(calls.filter((c) => /^(clear|set) a/.test(c))).toEqual([]);
  });

  it("replaces it with the still's layout once it has one", async () => {
    const { coordinator, calls } = rig({
      kind: "fresh",
      still: still as Still,
      etag: '"3"',
    });
    coordinator.focusFrame("a");
    coordinator.poll(LIVE_POLL_MS);
    await new Promise((resolve) => setTimeout(resolve, 0));
    calls.length = 0;
    coordinator.focusFrame("b");
    expect(calls.indexOf("clear a")).toBeGreaterThanOrEqual(0);
    expect(calls.indexOf("set a s @0")).toBeGreaterThan(calls.indexOf("clear a"));
  });

  it("gives a returning frame its last live layout back, replacing the still's", () => {
    const { coordinator, calls, reported } = rig();
    coordinator.focusFrame("a");
    reported.set("a", LAYOUT);
    coordinator.focusFrame("b");
    calls.length = 0;
    coordinator.focusFrame("a");
    expect(calls.indexOf("clear a")).toBeGreaterThanOrEqual(0);
    expect(calls.indexOf("set a live @2000")).toBeGreaterThan(calls.indexOf("clear a"));
  });
});

describe("LiveCoordinator warm frame", () => {
  it("drops the hidden previous frame a minute after it was hidden", () => {
    const { coordinator, calls, timers } = rig();
    coordinator.focusFrame("a");
    coordinator.focusFrame("b");
    const [timer] = [...timers.values()];
    expect(timer?.ms).toBe(WARM_KEEP_MS);
    calls.length = 0;
    timer?.run();
    expect(calls).toContain("setLive b null");
  });

  it("does not drop a frame that came back before the minute was up", () => {
    const { coordinator, timers } = rig();
    coordinator.focusFrame("a");
    coordinator.focusFrame("b");
    coordinator.focusFrame("a");
    // The timer now belongs to b, the frame that is warm.
    expect([...timers.values()]).toHaveLength(1);
  });
});

describe("LiveCoordinator polling", () => {
  it("re-checks the live frame less often than the others", async () => {
    const { coordinator, fetched } = rig();
    coordinator.focusFrame("a");
    const tick = async (now: number) => {
      fetched.push([]);
      coordinator.poll(now);
      await new Promise((resolve) => setTimeout(resolve, 0));
    };
    await tick(LIVE_POLL_MS);
    await tick(LIVE_POLL_MS + 3_000);
    await tick(LIVE_POLL_MS + 6_000);
    await tick(LIVE_POLL_MS * 2);
    expect(fetched.slice(1).map((ids) => ids.sort())).toEqual([
      ["a", "b"],
      ["b"],
      ["b"],
      ["a", "b"],
    ]);
  });
});
