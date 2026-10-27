/**
 * The `/sync` WebSocket: handshake, keep-alive, reconnect and the push gate. Binary frames only; every frame
 * goes through the wasm `Viewer` (`begin_session`, `receive`, `poll_push`) or a wasm `encode_*` function, so
 * this module never parses or builds protocol bytes.
 *
 * - open → `Viewer.begin_session(token, now)` (the `Hello`); `Welcome` → state `open`, then `Watch` of the
 *   wanted page. Browser scope delivers a fresh catalog snapshot after every `Welcome`; the page snapshot
 *   follows the `Watch`.
 * - After every received frame the outbox is flushed (`poll_push`): the viewer decides when a `PushOps` may go.
 * - Keep-alive: answer `Ping` with `Pong`; ping after {@link PING_INTERVAL_MS} of silence; drop the socket
 *   (and reconnect) after {@link IDLE_TIMEOUT_MS}.
 * - Reconnect with {@link RECONNECT_DELAYS_MS} backoff (last value repeats), except after `Reject`.
 */

import { MessageKind, type Received, type Viewer } from "../wasm";

/** Mirrors proto `PING_INTERVAL_MS`. */
export const PING_INTERVAL_MS = 15_000;
/** Mirrors proto `IDLE_TIMEOUT_MS`. */
export const IDLE_TIMEOUT_MS = 45_000;
/** Reconnect backoff steps. */
export const RECONNECT_DELAYS_MS: readonly number[] = [500, 1_000, 2_000, 5_000, 10_000];
/** How often keep-alive state is checked. */
const TICK_MS = 5_000;

/** Lifecycle as shown in the UI. `rejected` = the server refused the token (no retry). */
export type ConnectionState = "connecting" | "open" | "closed" | "rejected";

/** The slice of a `WebSocket` the connection uses. */
export interface SocketLike {
  binaryType: BinaryType;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  send(data: Uint8Array): void;
  close(): void;
}

/** The slice of the wasm `Viewer` the connection drives. */
export type SyncSession = Pick<Viewer, "begin_session" | "end_session" | "receive" | "poll_push">;

/** Client frames the connection builds itself. */
export interface FrameEncoder {
  encode_watch(page?: string | null): Uint8Array;
  encode_ping(nonce: bigint): Uint8Array;
  encode_pong(nonce: bigint): Uint8Array;
}

/** Timer functions (injected so tests control time). */
export interface Timers {
  setTimeout(callback: () => void, ms: number): number;
  clearTimeout(handle: number): void;
  setInterval(callback: () => void, ms: number): number;
  clearInterval(handle: number): void;
}

/** What the app hears from the connection. */
export interface ConnectionEvents {
  state(state: ConnectionState): void;
  /** A frame was applied by the viewer. */
  received(summary: Received): void;
  /** Something the user may want to see (undecodable frame, server error). Never fatal. */
  problem(message: string): void;
}

/** Construction options. */
export interface ConnectionOptions {
  readonly url: string;
  readonly token: () => string;
  readonly now: () => number;
  readonly createSocket: (url: string) => SocketLike;
  readonly timers: Timers;
}

/** One logical connection with automatic reconnects. */
export class Connection {
  private socket: SocketLike | null = null;
  private state: ConnectionState = "closed";
  private attempt = 0;
  private wanted: string | null = null;
  private stopped = true;
  private lastRx = 0;
  private lastTx = 0;
  private retry: number | null = null;
  private tick: number | null = null;
  private pingNonce = 0n;

  constructor(
    private readonly session: SyncSession,
    private readonly frames: FrameEncoder,
    private readonly events: ConnectionEvents,
    private readonly options: ConnectionOptions,
  ) {}

  /** Current state. */
  get current(): ConnectionState {
    return this.state;
  }

  /** Connects and keeps the socket open until {@link Connection.stop}. */
  start(): void {
    this.stopped = false;
    this.connect();
  }

  /** Closes the socket and stops reconnecting. */
  stop(): void {
    this.stopped = true;
    this.clearTimers();
    this.socket?.close();
  }

  /** Sets the page whose snapshot and live ink we want (sent now if open, and again after reconnects). */
  watch(page: string | null): void {
    this.wanted = page;
    if (this.state === "open") this.send(this.frames.encode_watch(page));
  }

  /** Sends an encoded client frame; false when the socket is not open. */
  send(frame: Uint8Array): boolean {
    if (this.state !== "open" || !this.socket) return false;
    this.socket.send(frame);
    this.lastTx = this.options.now();
    return true;
  }

  /** Sends the next queued `PushOps` if the viewer allows one (call after every local commit). */
  flush(): void {
    for (;;) {
      if (this.state !== "open") return;
      const frame = this.session.poll_push();
      if (!frame) return;
      this.send(frame);
    }
  }

  private connect(): void {
    this.setState("connecting");
    const socket = this.options.createSocket(this.options.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onopen = () => {
      if (this.socket !== socket) return;
      const hello = this.session.begin_session(this.options.token(), this.options.now());
      this.lastRx = this.lastTx = this.options.now();
      socket.send(hello);
      this.startTick();
    };
    socket.onmessage = (event) => {
      if (this.socket === socket) this.onFrame(event.data);
    };
    socket.onclose = () => {
      if (this.socket === socket) this.onClosed();
    };
    socket.onerror = () => {
      /* `close` always follows an error */
    };
  }

  private onFrame(data: unknown): void {
    if (!(data instanceof ArrayBuffer)) {
      this.events.problem("ignored a non-binary frame");
      return;
    }
    this.lastRx = this.options.now();
    let summary: Received;
    try {
      summary = this.session.receive(new Uint8Array(data));
    } catch (error) {
      this.events.problem(`dropped an undecodable frame: ${String(error)}`);
      return;
    }
    this.afterReceive(summary);
    this.events.received(summary);
    this.flush();
  }

  private afterReceive(summary: Received): void {
    switch (summary.kind) {
      case MessageKind.Welcome:
        this.attempt = 0;
        this.setState("open");
        if (this.wanted !== null) this.send(this.frames.encode_watch(this.wanted));
        break;
      case MessageKind.Ping:
        this.send(this.frames.encode_pong(summary.nonce));
        break;
      case MessageKind.Reject:
        this.stopped = true;
        this.setState("rejected");
        this.socket?.close();
        break;
      case MessageKind.Error:
        this.events.problem(`server error: ${summary.error ?? "unknown"} ${summary.detail ?? ""}`);
        break;
      default:
        break;
    }
    if (summary.resync) {
      // Refused ops were rebuilt out of the replica (now empty): reconnect so the server log refills it.
      this.events.problem(`${summary.rejected} op(s) were refused by the server`);
      this.socket?.close();
    }
    if (summary.apply_errors > 0) {
      this.events.problem(`${summary.apply_errors} item(s) from the server were refused`);
    }
  }

  private onClosed(): void {
    this.clearTimers();
    this.socket = null;
    this.session.end_session();
    if (this.state !== "rejected") this.setState("closed");
    if (this.stopped) return;
    const delays = RECONNECT_DELAYS_MS;
    const delay = delays[Math.min(this.attempt, delays.length - 1)] ?? 10_000;
    this.attempt++;
    this.retry = this.options.timers.setTimeout(() => {
      this.retry = null;
      if (!this.stopped) this.connect();
    }, delay);
  }

  private startTick(): void {
    this.tick = this.options.timers.setInterval(() => this.keepAlive(), TICK_MS);
  }

  private keepAlive(): void {
    const now = this.options.now();
    if (now - this.lastRx >= IDLE_TIMEOUT_MS) {
      this.socket?.close();
    } else if (this.state === "open" && now - this.lastTx >= PING_INTERVAL_MS) {
      this.pingNonce++;
      this.send(this.frames.encode_ping(this.pingNonce));
    }
  }

  private clearTimers(): void {
    if (this.retry !== null) this.options.timers.clearTimeout(this.retry);
    if (this.tick !== null) this.options.timers.clearInterval(this.tick);
    this.retry = null;
    this.tick = null;
  }

  private setState(state: ConnectionState): void {
    if (state === this.state) return;
    this.state = state;
    this.events.state(state);
  }
}
