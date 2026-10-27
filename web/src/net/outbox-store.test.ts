import { describe, expect, it } from "vitest";
import { fromBase64, OUTBOX_KEY_PREFIX, OutboxStore, toBase64 } from "./outbox-store";

class MemoryStorage {
  readonly items = new Map<string, string>();
  fail = false;
  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    if (this.fail) throw new Error("quota");
    this.items.set(key, value);
  }
  removeItem(key: string): void {
    this.items.delete(key);
  }
}

function viewer() {
  const state = { revision: 0, pending: 1, bytes: new Uint8Array([1, 2, 3]) };
  return {
    state,
    pending: 1,
    export_state: () => state.bytes,
    pending_ops: () => state.pending,
    dead_letters: () => [],
    state_revision: () => state.revision,
  };
}

describe("base64", () => {
  it("round-trips bytes larger than one chunk", () => {
    const bytes = Uint8Array.from({ length: 100_000 }, (_, i) => i % 251);
    expect(fromBase64(toBase64(bytes))).toEqual(bytes);
  });
  it("rejects text that is not base64", () => {
    expect(fromBase64("***")).toBeNull();
  });
});

describe("OutboxStore", () => {
  it("saves only when the viewer's state moved, under the device's key", () => {
    const storage = new MemoryStorage();
    const v = viewer();
    const store = new OutboxStore(storage, "DEV", () => {});
    expect(store.load()).toBeNull();
    expect(store.save(v)).toBe(true);
    expect(storage.items.size).toBe(0);
    v.state.revision = 1;
    store.save(v);
    expect(storage.items.has(`${OUTBOX_KEY_PREFIX}DEV`)).toBe(true);
    expect(store.load()).toEqual(v.state.bytes);
    storage.items.clear();
    store.save(v);
    expect(storage.items.size).toBe(0);
  });

  it("removes the blob once nothing is unsent or refused", () => {
    const storage = new MemoryStorage();
    const v = viewer();
    const store = new OutboxStore(storage, "DEV", () => {});
    v.state.revision = 1;
    store.save(v);
    expect(storage.items.size).toBe(1);
    v.state.revision = 2;
    v.state.pending = 0;
    store.save(v);
    expect(storage.items.size).toBe(0);
  });

  it("reports a failed write and retries it on the next save", () => {
    const storage = new MemoryStorage();
    const problems: string[] = [];
    const v = viewer();
    v.state.revision = 5;
    const store = new OutboxStore(storage, "DEV", (m) => problems.push(m));
    storage.fail = true;
    expect(store.save(v)).toBe(false);
    expect(problems).toHaveLength(1);
    storage.fail = false;
    expect(store.save(v)).toBe(true);
    expect(store.load()).toEqual(v.state.bytes);
  });

  it("moves an unreadable blob aside instead of deleting it", () => {
    const storage = new MemoryStorage();
    storage.setItem(`${OUTBOX_KEY_PREFIX}DEV`, "AAAA");
    const store = new OutboxStore(storage, "DEV", () => {});
    store.quarantine();
    expect(store.load()).toBeNull();
    expect(storage.items.get(`${OUTBOX_KEY_PREFIX}DEV.unreadable`)).toBe("AAAA");
  });
});
