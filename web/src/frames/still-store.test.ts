import { describe, expect, it } from "vitest";
import { parseStill, type Still } from "./still";
import { type StillFetch, type StillReply, StillStore } from "./still-store";

function still(version: number): Still {
  const parsed = parseStill({
    version,
    doc: [390, 3000],
    viewport: [390, 844],
    strips: [{ y: 0, h: 3000, url: `/frames/f/still/${version}/0.png` }],
    layout: { rects: {}, state: "" },
  });
  if (!parsed) throw new Error("bad still");
  return parsed;
}

function rig(replies: StillReply[]) {
  const calls: { id: string; etag: string | null }[] = [];
  const changes: { id: string; version: string }[] = [];
  const fetchStill: StillFetch = async (id, etag) => {
    calls.push({ id, etag });
    const reply = replies.shift();
    if (!reply) throw new Error("offline");
    return reply;
  };
  const store = new StillStore(fetchStill, (id, s) => changes.push({ id, version: s.version }));
  return { store, calls, changes };
}

describe("StillStore", () => {
  it("keeps a still and asks again with its ETag", async () => {
    const { store, calls, changes } = rig([
      { kind: "fresh", still: still(1), etag: '"v1"' },
      { kind: "unchanged" },
      { kind: "fresh", still: still(2), etag: '"v2"' },
    ]);
    await store.check("f");
    expect(store.get("f")?.version).toBe("1");
    await store.check("f");
    await store.check("f");
    expect(calls.map((c) => c.etag)).toEqual([null, '"v1"', '"v1"']);
    expect(changes).toEqual([
      { id: "f", version: "1" },
      { id: "f", version: "2" },
    ]);
    expect(store.get("f")?.version).toBe("2");
  });

  it("has no still before the server made one, and keeps asking", async () => {
    const { store, changes } = rig([
      { kind: "none" },
      { kind: "fresh", still: still(1), etag: null },
    ]);
    await store.check("f");
    expect(store.get("f")).toBeNull();
    expect(changes).toEqual([]);
    await store.check("f");
    expect(store.get("f")?.version).toBe("1");
  });

  it("survives a failed request and keeps what it has", async () => {
    const { store } = rig([{ kind: "fresh", still: still(1), etag: '"v1"' }]);
    await store.check("f");
    await store.check("f");
    expect(store.get("f")?.version).toBe("1");
  });

  it("does not repeat a check that is still in flight", async () => {
    const { store, calls } = rig([{ kind: "none" }, { kind: "none" }]);
    store.poll(["f", "f"]);
    store.poll(["f"]);
    await store.check("f");
    expect(calls).toHaveLength(1);
  });

  it("forgets a deleted frame, and drops an answer that arrives after", async () => {
    const { store, changes } = rig([{ kind: "fresh", still: still(1), etag: null }]);
    const pending = store.check("f");
    store.forget("f");
    await pending;
    expect(changes).toEqual([]);
    expect(store.get("f")).toBeNull();
  });
});
