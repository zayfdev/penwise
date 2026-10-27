/** Viewer entry: load wasm, build the app, connect. */

import { App } from "./app/app";
import "./style.css";
import { loadWasm } from "./wasm";

async function main(): Promise<void> {
  const wasm = await loadWasm();
  new App(wasm).start();
}

main().catch((error: unknown) => {
  console.error("penwise viewer failed to start", error);
  document.body.textContent = `Penwise could not start: ${String(error)}`;
});
