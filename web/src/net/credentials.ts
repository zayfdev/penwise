/**
 * Who this browser is: the shared access token (kept in `localStorage`, taken from `?token=` or a pairing
 * code) and a per-tab device id (`sessionStorage`; two tabs must not share a device id because ops are
 * stamped per device).
 */

import type { Wasm } from "../wasm";

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

/** The tab's device id, minted once per tab session. */
export function resolveDevice(
  storage: Pick<Storage, "getItem" | "setItem">,
  wasm: Pick<Wasm, "new_device_id">,
  nowMs: number,
  random: (bytes: Uint8Array) => Uint8Array,
): string {
  const existing = storage.getItem(DEVICE_KEY);
  if (existing) return existing;
  const device = wasm.new_device_id(nowMs, random(new Uint8Array(10)));
  storage.setItem(DEVICE_KEY, device);
  return device;
}

/** `ws(s)://host/sync` of the page's own origin. */
export function syncUrl(location: Pick<Location, "protocol" | "host">): string {
  return `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/sync`;
}
