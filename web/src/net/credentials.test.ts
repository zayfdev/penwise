import { describe, expect, it } from "vitest";
import { claimDevice, DEVICE_KEY, type DeviceLocks } from "./credentials";
import { OUTBOX_KEY_PREFIX } from "./outbox-store";

class Store {
  readonly items = new Map<string, string>();
  get length(): number {
    return this.items.size;
  }
  key(i: number): string | null {
    return [...this.items.keys()][i] ?? null;
  }
  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }
}

/** Locks that stay held for the test's lifetime, like a live tab's. */
function locks(held: Set<string> = new Set()): DeviceLocks {
  return {
    request(name, _options, callback) {
      if (held.has(name)) return callback(null);
      held.add(name);
      return callback({});
    },
  };
}

const wasm = () => {
  let n = 0;
  return { new_device_id: () => `new${++n}` };
};
const random = (b: Uint8Array) => b;

describe("claimDevice", () => {
  it("keeps the tab's own id across reloads and mints once", async () => {
    const session = new Store();
    const local = new Store();
    const w = wasm();
    expect(await claimDevice(session, local, undefined, w, 1, random)).toBe("new1");
    expect(await claimDevice(session, local, undefined, w, 2, random)).toBe("new1");
    expect(session.getItem(DEVICE_KEY)).toBe("new1");
  });

  it("adopts the device of a closed tab that left unsent drawing", async () => {
    const session = new Store();
    const local = new Store();
    local.setItem(`${OUTBOX_KEY_PREFIX}OLD`, "blob");
    local.setItem(`${OUTBOX_KEY_PREFIX}OLD2.unreadable`, "blob");
    const device = await claimDevice(session, local, locks(), wasm(), 1, random);
    expect(device).toBe("OLD");
    expect(session.getItem(DEVICE_KEY)).toBe("OLD");
  });

  it("skips a device another live tab holds and mints a new one", async () => {
    const held = new Set(["penwise-device-OLD"]);
    const session = new Store();
    const local = new Store();
    local.setItem(`${OUTBOX_KEY_PREFIX}OLD`, "blob");
    expect(await claimDevice(session, local, locks(held), wasm(), 1, random)).toBe("new1");
    expect(held.has("penwise-device-new1")).toBe(true);
  });

  it("gives a duplicated tab (same session id, id held elsewhere) its own device", async () => {
    const held = new Set(["penwise-device-DUP"]);
    const session = new Store();
    session.setItem(DEVICE_KEY, "DUP");
    expect(await claimDevice(session, new Store(), locks(held), wasm(), 1, random)).toBe("new1");
    expect(session.getItem(DEVICE_KEY)).toBe("new1");
  });
});
