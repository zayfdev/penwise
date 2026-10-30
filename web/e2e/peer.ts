/**
 * A second device in Node: the same wasm `Viewer` the browser uses, over the real `/sync` socket. It plays the
 * role of "another tablet/browser" that draws strokes (live deltas, then the committed op).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadWasmSync, MessageKind, type Viewer, type Wasm } from "../src/wasm";

const here = (relative: string) => fileURLToPath(new URL(relative, import.meta.url));

/** The wasm module, instantiated from the generated package. */
export function nodeWasm(): Wasm {
  return loadWasmSync(readFileSync(here("../src/generated/penwise-wasm/penwise_wasm_bg.wasm")));
}

/** One point of a stroke in page px. */
export interface Point {
  readonly x: number;
  readonly y: number;
  readonly pressure?: number;
}

/** A headless device connected to `/sync`. */
export class Peer {
  readonly viewer: Viewer;
  private readonly socket: WebSocket;
  private readonly waiters: { kind: MessageKind; scope?: string; resolve: () => void }[] = [];
  private scope: string | null = null;

  private constructor(
    private readonly wasm: Wasm,
    socket: WebSocket,
    viewer: Viewer,
  ) {
    this.socket = socket;
    this.viewer = viewer;
    socket.binaryType = "arraybuffer";
    socket.onmessage = (event) => this.onFrame(new Uint8Array(event.data as ArrayBuffer));
  }

  /** Connects, authenticates and waits for the catalog snapshot. */
  static async connect(wasm: Wasm, origin: string, token: string): Promise<Peer> {
    const socket = new WebSocket(`${origin.replace("http", "ws")}/sync`);
    const viewer = new wasm.Viewer();
    viewer.identify(
      wasm.new_device_id(Date.now(), crypto.getRandomValues(new Uint8Array(10))),
      42n,
    );
    const peer = new Peer(wasm, socket, viewer);
    await new Promise<void>((resolve, reject) => {
      socket.onopen = () => resolve();
      socket.onerror = () => reject(new Error("peer socket failed"));
    });
    const catalog = peer.until(MessageKind.SnapshotEnd, "catalog");
    socket.send(viewer.begin_session(token, Date.now()) as Uint8Array<ArrayBuffer>);
    await catalog;
    return peer;
  }

  /** Watches `page` and waits for its snapshot. */
  async watch(page: string): Promise<void> {
    const done = this.until(MessageKind.SnapshotEnd, page);
    this.socket.send(this.wasm.encode_watch(page) as Uint8Array<ArrayBuffer>);
    await done;
  }

  /**
   * Draws one stroke: a live delta per `chunk` points (with `pause` ms between), then the committed op.
   * `beforeCommit` runs after the last delta (assert on live previews there).
   */
  async stroke(
    page: string,
    points: readonly Point[],
    options: { chunk?: number; pause?: number; beforeCommit?: () => Promise<void> } = {},
  ): Promise<void> {
    const chunk = options.chunk ?? 8;
    this.viewer.stroke_begin(page, 0, 0xffcc0000, 3, Date.now());
    let t = 0;
    for (let i = 0; i < points.length; i += chunk) {
      const part = points.slice(i, i + chunk);
      const events = new Float32Array(part.length * 5);
      const times = new Uint32Array(part.length);
      part.forEach((p, k) => {
        events.set([p.x, p.y, p.pressure ?? 0.6, 0, 0], k * 5);
        times[k] = t;
        t += 8_000;
      });
      const frame = this.viewer.stroke_push(this.wasm.pointer_samples(events), times);
      this.socket.send(frame as Uint8Array<ArrayBuffer>);
      if (options.pause) await new Promise((r) => setTimeout(r, options.pause));
    }
    await options.beforeCommit?.();
    this.viewer.stroke_end(Date.now());
    this.flush();
    await this.settled();
  }

  /**
   * Sends `Focus { page, item }` (switch to `page`, fit the frame). `false` when the server did not report
   * protocol 2, so a client must not send it.
   */
  focus(page: string, item: string | null): boolean {
    const bytes = this.viewer.focus(page, item);
    if (!bytes) return false;
    this.socket.send(bytes as Uint8Array<ArrayBuffer>);
    return true;
  }

  /** Waits until every local op is acknowledged. */
  async settled(): Promise<void> {
    const deadline = Date.now() + 10_000;
    while (this.viewer.pending_ops() > 0) {
      if (Date.now() > deadline) throw new Error("peer ops were never acknowledged");
      await new Promise((r) => setTimeout(r, 20));
    }
  }

  close(): void {
    this.socket.close();
  }

  private until(kind: MessageKind, scope?: string): Promise<void> {
    return new Promise((resolve) => {
      this.waiters.push(scope === undefined ? { kind, resolve } : { kind, scope, resolve });
    });
  }

  private flush(): void {
    for (let frame = this.viewer.poll_push(); frame; frame = this.viewer.poll_push()) {
      this.socket.send(frame as Uint8Array<ArrayBuffer>);
    }
  }

  private onFrame(bytes: Uint8Array): void {
    const received = this.viewer.receive(bytes);
    if (received.kind === MessageKind.SnapshotBegin) this.scope = received.scope ?? null;
    if (received.kind === MessageKind.Ping) {
      this.socket.send(this.wasm.encode_pong(received.nonce) as Uint8Array<ArrayBuffer>);
    }
    if (received.kind === MessageKind.SnapshotEnd) {
      const ended = this.scope;
      this.scope = null;
      this.resolve(MessageKind.SnapshotEnd, ended);
    }
    this.flush();
  }

  private resolve(kind: MessageKind, scope: string | null): void {
    const index = this.waiters.findIndex(
      (w) => w.kind === kind && (w.scope === undefined || w.scope === scope),
    );
    const [waiter] = index >= 0 ? this.waiters.splice(index, 1) : [];
    waiter?.resolve();
  }
}
