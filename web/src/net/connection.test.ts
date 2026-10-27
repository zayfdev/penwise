import { beforeAll, describe, expect, it } from "vitest";
import { FakeClock, FakeSocket, fixture, realWasm } from "../test/support";
import { MessageKind, type Received, type Wasm } from "../wasm";
import {
  Connection,
  type ConnectionState,
  IDLE_TIMEOUT_MS,
  PING_INTERVAL_MS,
  RECONNECT_DELAYS_MS,
} from "./connection";

let wasm: Wasm;
beforeAll(() => {
  wasm = realWasm();
});

interface Rig {
  readonly connection: Connection;
  readonly sockets: FakeSocket[];
  readonly clock: FakeClock;
  readonly states: ConnectionState[];
  readonly kinds: MessageKind[];
  readonly problems: string[];
}

function rig(): Rig {
  const viewer = new wasm.Viewer();
  viewer.identify(wasm.new_device_id(1_700_000_000_000, new Uint8Array(10).fill(3)), 7n);
  const sockets: FakeSocket[] = [];
  const clock = new FakeClock();
  const states: ConnectionState[] = [];
  const kinds: MessageKind[] = [];
  const problems: string[] = [];
  const connection = new Connection(
    viewer,
    wasm,
    {
      state: (s) => states.push(s),
      received: (r: Received) => kinds.push(r.kind),
      problem: (m) => problems.push(m),
    },
    {
      url: "ws://test/sync",
      token: () => "secret",
      now: () => clock.now,
      createSocket: () => {
        const socket = new FakeSocket();
        sockets.push(socket);
        return socket;
      },
      timers: clock,
    },
  );
  return { connection, sockets, clock, states, kinds, problems };
}

const clientKinds = (socket: FakeSocket) => socket.sent.map((f) => wasm.decode_client_kind(f));

describe("Connection", () => {
  it("sends Hello on open, becomes open on Welcome and then watches the wanted page", () => {
    const { connection, sockets, states } = rig();
    connection.watch("0000000000000000000000000A");
    connection.start();
    const socket = sockets[0];
    expect(socket?.binaryType).toBe("arraybuffer");
    socket?.onopen?.();
    expect(socket && clientKinds(socket)).toEqual(["Hello"]);
    socket?.deliver(fixture("server_welcome.bin"));
    expect(states).toEqual(["connecting", "open"]);
    expect(socket && clientKinds(socket)).toEqual(["Hello", "Watch"]);
  });

  it("answers Ping with Pong and applies catalog ops", () => {
    const { connection, sockets, kinds } = rig();
    connection.start();
    const socket = sockets[0];
    socket?.onopen?.();
    socket?.deliver(fixture("server_welcome.bin"));
    socket?.deliver(fixture("server_ping.bin"));
    socket?.deliver(fixture("server_ops_since.bin"));
    expect(kinds).toEqual([MessageKind.Welcome, MessageKind.Ping, MessageKind.OpsSince]);
    expect(socket && clientKinds(socket)).toContain("Pong");
  });

  it("does not retry after the server rejects the token", () => {
    const { connection, sockets, states, clock } = rig();
    connection.start();
    sockets[0]?.onopen?.();
    sockets[0]?.deliver(fixture("server_reject_bad_token.bin"));
    expect(states.at(-1)).toBe("rejected");
    clock.advance(60_000);
    expect(sockets).toHaveLength(1);
  });

  it("reconnects with backoff and re-sends Hello and Watch", () => {
    const { connection, sockets, states, clock } = rig();
    connection.watch("0000000000000000000000000A");
    connection.start();
    sockets[0]?.onopen?.();
    sockets[0]?.deliver(fixture("server_welcome.bin"));
    sockets[0]?.close();
    expect(states.at(-1)).toBe("closed");
    clock.advance((RECONNECT_DELAYS_MS[0] ?? 0) - 1);
    expect(sockets).toHaveLength(1);
    clock.advance(1);
    expect(sockets).toHaveLength(2);
    sockets[1]?.onopen?.();
    sockets[1]?.deliver(fixture("server_welcome.bin"));
    expect(sockets[1] && clientKinds(sockets[1])).toEqual(["Hello", "Watch"]);
    // A successful Welcome resets the backoff.
    sockets[1]?.close();
    clock.advance(RECONNECT_DELAYS_MS[0] ?? 0);
    expect(sockets).toHaveLength(3);
  });

  it("backs off further while the server stays unreachable", () => {
    const { connection, sockets, clock } = rig();
    connection.start();
    sockets[0]?.close();
    clock.advance(RECONNECT_DELAYS_MS[0] ?? 0);
    sockets[1]?.close();
    clock.advance((RECONNECT_DELAYS_MS[1] ?? 0) - 1);
    expect(sockets).toHaveLength(2);
    clock.advance(1);
    expect(sockets).toHaveLength(3);
  });

  it("pings after silence and drops a silent socket", () => {
    const { connection, sockets, clock } = rig();
    connection.start();
    const socket = sockets[0];
    socket?.onopen?.();
    socket?.deliver(fixture("server_welcome.bin"));
    clock.advance(PING_INTERVAL_MS + 5_000);
    expect(socket && clientKinds(socket)).toContain("Ping");
    clock.advance(IDLE_TIMEOUT_MS);
    expect(socket?.closed).toBe(true);
    clock.advance(RECONNECT_DELAYS_MS[0] ?? 0);
    expect(sockets).toHaveLength(2);
  });

  it("drops undecodable and non-binary frames without dying", () => {
    const { connection, sockets, problems, kinds } = rig();
    connection.start();
    sockets[0]?.onopen?.();
    sockets[0]?.deliver(new Uint8Array([9, 9, 9]));
    sockets[0]?.onmessage?.({ data: "text" });
    expect(problems).toHaveLength(2);
    sockets[0]?.deliver(fixture("server_welcome.bin"));
    expect(kinds).toEqual([MessageKind.Welcome]);
  });

  it("stop closes without reconnecting", () => {
    const { connection, sockets, clock } = rig();
    connection.start();
    connection.stop();
    clock.advance(60_000);
    expect(sockets).toHaveLength(1);
  });

  it("send reports false while not open", () => {
    const { connection } = rig();
    expect(connection.send(new Uint8Array([1]))).toBe(false);
  });
});
