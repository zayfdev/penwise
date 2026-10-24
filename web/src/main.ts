/**
 * Viewer entry: load wasm, create the `Viewer`, wire connection, renderer and UI.
 *
 * Owner: slice d.
 *
 * Boot: `await init()` (wasm-bindgen `--target web` default export) → `new Viewer()` → `Connection` with
 * `syncUrl(location)`, `localStorage`, `Date.now`, `crypto.getRandomValues`, `new WebSocket(url)` →
 * `PageList`, `PairingPanel`, `PageRenderer` on `#paper`/`#ink`/`#live` → `connection.start()`.
 * Rendering is driven by `Received` summaries: `changed_pages` containing the shown page → redraw ink;
 * `live_pages` → redraw live; `catalog_changed` → refresh lists and `choosePage`; `SwitchPage` in follow mode
 * → switch. Redraws are coalesced to one `requestAnimationFrame`. `ResizeObserver` on `#stage` → resize + refit.
 */

import "./style.css";

/** Starts the app on the current document; resolves once the first connection attempt has begun. */
export async function main(): Promise<void> {
  throw new Error("unimplemented: main");
}

main().catch((error: unknown) => {
  console.error("penwise viewer failed to start", error);
});
