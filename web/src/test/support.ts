/** Shared test helpers: the real wasm module, wire fixtures, a fake socket and fake timers. */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { SocketLike, Timers } from "../net/connection";
import { loadWasmSync, type Wasm } from "../wasm";

const here = (relative: string) => fileURLToPath(new URL(relative, import.meta.url));

/** The wasm module instantiated from the built `src/generated` package (run `npm run wasm` first). */
export function realWasm(): Wasm {
  return loadWasmSync(readFileSync(here("../generated/penwise-wasm/penwise_wasm_bg.wasm")));
}

/** A committed v1 wire fixture of `penwise-proto` (`server_welcome.bin`, …). */
export function fixture(name: string): Uint8Array {
  return new Uint8Array(
    readFileSync(here(`../../../crates/penwise-proto/tests/fixtures/v1/${name}`)),
  );
}

/** A socket the test drives by hand. */
export class FakeSocket implements SocketLike {
  binaryType: BinaryType = "blob";
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  readonly sent: Uint8Array[] = [];
  closed = false;

  send(data: Uint8Array): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.onclose?.();
  }

  /** Server → client frame. */
  deliver(bytes: Uint8Array): void {
    this.onmessage?.({ data: bytes.slice().buffer });
  }
}

/** Manually advanced clock and timers. */
export class FakeClock implements Timers {
  now = 1_700_000_000_000;
  private nextHandle = 1;
  private readonly timeouts = new Map<number, { at: number; callback: () => void }>();
  private readonly intervals = new Map<
    number,
    { every: number; next: number; callback: () => void }
  >();

  setTimeout(callback: () => void, ms: number): number {
    const handle = this.nextHandle++;
    this.timeouts.set(handle, { at: this.now + ms, callback });
    return handle;
  }

  clearTimeout(handle: number): void {
    this.timeouts.delete(handle);
  }

  setInterval(callback: () => void, ms: number): number {
    const handle = this.nextHandle++;
    this.intervals.set(handle, { every: ms, next: this.now + ms, callback });
    return handle;
  }

  clearInterval(handle: number): void {
    this.intervals.delete(handle);
  }

  /** Advances time, firing due timers in order. */
  advance(ms: number): void {
    const target = this.now + ms;
    for (;;) {
      const due = [
        ...[...this.timeouts].map(([h, t]) => ({ h, at: t.at, kind: "t" as const })),
        ...[...this.intervals].map(([h, t]) => ({ h, at: t.next, kind: "i" as const })),
      ]
        .filter((d) => d.at <= target)
        .sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      this.now = due.at;
      if (due.kind === "t") {
        const t = this.timeouts.get(due.h);
        this.timeouts.delete(due.h);
        t?.callback();
      } else {
        const t = this.intervals.get(due.h);
        if (t) {
          t.next += t.every;
          t.callback();
        }
      }
    }
    this.now = target;
  }
}
