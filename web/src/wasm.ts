/**
 * The `penwise-wasm` module: one place that loads it and names the types the rest of the viewer uses.
 *
 * All protocol bytes, replica state and stroke geometry live behind these exports; TypeScript never parses a
 * frame or computes geometry (AGENTS.md "TypeScript").
 */

import init, * as wasm from "./generated/penwise-wasm/penwise_wasm.js";

export type {
  DrawList,
  NotebookEntry,
  PageEntry,
  Received,
  Viewer,
} from "./generated/penwise-wasm/penwise_wasm.js";
export { DrawKind, MessageKind } from "./generated/penwise-wasm/penwise_wasm.js";

/** Functions of the module that the viewer calls (a type so tests can pass the real module or a fake). */
export type Wasm = typeof wasm;

/** Instantiates the wasm module (browser: fetched next to the bundle) and returns its exports. */
export async function loadWasm(): Promise<Wasm> {
  await init();
  return wasm;
}

/** Instantiates from bytes already in hand (tests, workers). */
export function loadWasmSync(bytes: BufferSource): Wasm {
  wasm.initSync({ module: bytes });
  return wasm;
}
