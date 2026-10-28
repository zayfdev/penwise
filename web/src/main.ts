/** Viewer entry: load wasm, build the app, connect. */

import { App } from "./app/app";
import "./style.css";
import { claimDevice, type DeviceLocks } from "./net/credentials";
import { loadWasm } from "./wasm";

async function main(): Promise<void> {
  const wasm = await loadWasm();
  const random = (bytes: Uint8Array): Uint8Array => {
    crypto.getRandomValues(bytes as Uint8Array<ArrayBuffer>);
    return bytes;
  };
  const device = await claimDevice(
    sessionStorage,
    localStorage,
    navigator.locks as unknown as DeviceLocks | undefined,
    wasm,
    Date.now(),
    random,
  );
  await new App(wasm, device).start();
}

main().catch((error: unknown) => {
  console.error("penwise viewer failed to start", error);
  document.body.textContent = `Penwise could not start: ${String(error)}`;
});
