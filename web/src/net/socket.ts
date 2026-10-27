import type { ConnectionOptions, SocketLike } from "./connection";
import { syncUrl } from "./credentials";

/** Adapts a browser `WebSocket` to the connection's narrow socket interface. */
export function openSocket(url: string): SocketLike {
  const ws = new WebSocket(url);
  const socket: SocketLike = {
    get binaryType() {
      return ws.binaryType;
    },
    set binaryType(value) {
      ws.binaryType = value;
    },
    onopen: null,
    onmessage: null,
    onclose: null,
    onerror: null,
    // wasm-bindgen returns fresh copies out of linear memory, never views of a SharedArrayBuffer.
    send: (data) => ws.send(data as Uint8Array<ArrayBuffer>),
    close: () => ws.close(),
  };
  ws.onopen = () => socket.onopen?.();
  ws.onmessage = (event) => socket.onmessage?.({ data: event.data });
  ws.onclose = () => socket.onclose?.();
  ws.onerror = () => socket.onerror?.();
  return socket;
}

/** Connection options for a real browser: the page's own origin, wall clock, `WebSocket`, window timers. */
export function browserOptions(token: () => string): ConnectionOptions {
  return {
    url: syncUrl(location),
    token,
    now: () => Date.now(),
    createSocket: openSocket,
    timers: {
      setTimeout: (cb, ms) => window.setTimeout(cb, ms),
      clearTimeout: (h) => window.clearTimeout(h),
      setInterval: (cb, ms) => window.setInterval(cb, ms),
      clearInterval: (h) => window.clearInterval(h),
    },
  };
}
