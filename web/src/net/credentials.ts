/**
 * Who this browser is: the shared access token (kept in `localStorage`, taken from `?token=` or a pairing
 * code) and a per-tab device id (`sessionStorage`; two tabs must not share a device id because ops are
 * stamped per device).
 */

import type { Wasm } from "../wasm";
import { OUTBOX_KEY_PREFIX } from "./outbox-store";

/** Storage key of the access token. */
export const TOKEN_KEY = "penwise.token.v1";
/** Storage key of the tab's device id. */
export const DEVICE_KEY = "penwise.device.v1";

/** The token from `?token=` (saved) or storage; `null` when neither exists. */
export function resolveToken(
  search: string,
  storage: Pick<Storage, "getItem" | "setItem">,
): string | null {
  const fromUrl = new URLSearchParams(search).get("token");
  if (fromUrl) {
    storage.setItem(TOKEN_KEY, fromUrl);
    return fromUrl;
  }
  return storage.getItem(TOKEN_KEY) || null;
}

/** Forgets a rejected token. */
export function clearToken(storage: Pick<Storage, "removeItem">): void {
  storage.removeItem(TOKEN_KEY);
}

/** The slice of `navigator.locks` device claiming uses. */
export interface DeviceLocks {
  request(
    name: string,
    options: { ifAvailable: true },
    callback: (lock: unknown) => Promise<void>,
  ): Promise<unknown>;
}

/**
 * Claims `device` for the lifetime of this page: a Web Lock held until the tab goes away, so no other tab
 * adopts the same device id. False when another live tab holds it.
 */
export function holdDevice(locks: DeviceLocks, device: string): Promise<boolean> {
  return new Promise((resolve) => {
    void locks.request(`penwise-device-${device}`, { ifAvailable: true }, (lock) => {
      resolve(lock !== null);
      return lock === null ? Promise.resolve() : new Promise<void>(() => {});
    });
  });
}

/** Devices that left unsent edits in `storage` (see `OutboxStore`), newest key order unspecified. */
function strandedDevices(storage: Pick<Storage, "length" | "key">): string[] {
  const found: string[] = [];
  for (let i = 0; i < storage.length; i++) {
    const key = storage.key(i);
    if (key?.startsWith(OUTBOX_KEY_PREFIX) && !key.endsWith(".unreadable"))
      found.push(key.slice(OUTBOX_KEY_PREFIX.length));
  }
  return found;
}

/**
 * This page's device id. Order: the tab's own id (survives a reload), then a device whose saved outbox no live
 * tab holds (a tab closed with unsent drawing: its ink is delivered by this one), else a freshly minted id.
 * Two live tabs never share an id (ops are stamped per device). Without Web Locks only the first and last apply.
 */
export async function claimDevice(
  session: Pick<Storage, "getItem" | "setItem">,
  persistent: Pick<Storage, "length" | "key">,
  locks: DeviceLocks | undefined,
  wasm: Pick<Wasm, "new_device_id">,
  nowMs: number,
  random: (bytes: Uint8Array) => Uint8Array,
): Promise<string> {
  const own = session.getItem(DEVICE_KEY);
  if (own && (!locks || (await holdDevice(locks, own)))) return own;
  if (locks) {
    for (const device of strandedDevices(persistent)) {
      if (await holdDevice(locks, device)) {
        session.setItem(DEVICE_KEY, device);
        return device;
      }
    }
  }
  const device = wasm.new_device_id(nowMs, random(new Uint8Array(10)));
  if (locks) await holdDevice(locks, device);
  session.setItem(DEVICE_KEY, device);
  return device;
}

/** `ws(s)://host/sync` of the page's own origin. */
export function syncUrl(location: Pick<Location, "protocol" | "host">): string {
  return `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/sync`;
}
