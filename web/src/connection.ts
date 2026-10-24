/**
 * WebSocket client for `/sync` (protocol: `docs/contracts/m3-server.md` "Wire protocol").
 *
 * Owner: slice d.
 *
 * - Binary frames only (`binaryType = "arraybuffer"`); every frame goes through the wasm `Viewer` (`receive`)
 *   or a wasm `encode_*` function — this module never parses or builds protocol bytes itself.
 * - Handshake: with stored credentials send `encode_hello(device, token, viewer.head_seq())`; with a pending
 *   pairing code send `encode_pair(code, device, name)` first, store the `Paired` token, then send `Hello`.
 * - Keep-alive: answer `Ping` with `encode_pong(nonce)`; send `encode_ping` after `PING_INTERVAL_MS` of
 *   silence; close and reconnect after `IDLE_TIMEOUT_MS`.
 * - Reconnect with `RECONNECT_DELAYS_MS` backoff (last value repeats); `Reject(bad_token)` clears the stored
 *   credentials and switches to pairing instead of retrying.
 * - Credentials live in `localStorage` under `CREDENTIALS_KEY` (origin-scoped; the token is never logged).
 */

import type { Received, Viewer } from "./generated/penwise-wasm/penwise_wasm.js";

/** localStorage key of {@link StoredCredentials}. */
export const CREDENTIALS_KEY = "penwise.credentials.v1";
/** Mirrors proto `PING_INTERVAL_MS`. */
export const PING_INTERVAL_MS = 15_000;
/** Mirrors proto `IDLE_TIMEOUT_MS`. */
export const IDLE_TIMEOUT_MS = 45_000;
/** Reconnect backoff steps. */
export const RECONNECT_DELAYS_MS: readonly number[] = [500, 1_000, 2_000, 5_000, 10_000];

/** Connection lifecycle as shown in the UI. */
export type ConnectionState = "connecting" | "pairing" | "open" | "closed";

/** Device credentials obtained by pairing. */
export interface StoredCredentials {
  /** DeviceId text. */
  readonly device: string;
  /** AuthToken text (secret). */
  readonly token: string;
  /** ServerId text the token belongs to. */
  readonly server: string;
}

/** Callbacks from the connection to the app. */
export interface ConnectionEvents {
  /** A frame was applied by the viewer. */
  received(summary: Received): void;
  /** Lifecycle change. */
  state(state: ConnectionState): void;
  /** No credentials and no pending code: the UI must obtain a pairing code. */
  needsPairing(): void;
}

/** Construction options (injected for tests). */
export interface ConnectionOptions {
  readonly url: string;
  readonly storage: Storage;
  readonly now: () => number;
  readonly random: (bytes: Uint8Array) => void;
  readonly createSocket: (url: string) => WebSocket;
}

/** Reads credentials; `null` when absent or malformed. */
export function loadCredentials(storage: Storage): StoredCredentials | null {
  throw new Error("unimplemented: loadCredentials");
}

/** Persists credentials. */
export function saveCredentials(storage: Storage, credentials: StoredCredentials): void {
  throw new Error("unimplemented: saveCredentials");
}

/** Removes credentials (after `Reject(bad_token)`). */
export function clearCredentials(storage: Storage): void {
  throw new Error("unimplemented: clearCredentials");
}

/** `ws://` or `wss://` URL of `/sync` on the page's own host (follows the page protocol). */
export function syncUrl(location: Pick<Location, "protocol" | "host">): string {
  throw new Error("unimplemented: syncUrl");
}

/** One logical connection with automatic reconnects. */
export class Connection {
  /** Does not connect yet; call {@link Connection.start}. */
  constructor(viewer: Viewer, events: ConnectionEvents, options: ConnectionOptions) {
    throw new Error("unimplemented: Connection.constructor");
  }

  /** Opens the socket (or reports `needsPairing`) and keeps it open until {@link Connection.stop}. */
  start(): void {
    throw new Error("unimplemented: Connection.start");
  }

  /** Pairs this browser with `code` (typed by the user or fetched from the loopback `/pair`) as `name`. */
  pair(code: string, name: string): void {
    throw new Error("unimplemented: Connection.pair");
  }

  /** Sends an already encoded client frame if open; returns false when not connected. */
  send(frame: Uint8Array): boolean {
    throw new Error("unimplemented: Connection.send");
  }

  /** Closes the socket and stops reconnecting. */
  stop(): void {
    throw new Error("unimplemented: Connection.stop");
  }
}
