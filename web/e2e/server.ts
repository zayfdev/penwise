/** Starts the real `penwise-server` binary on a temp data dir and a free loopback port. */

import { type ChildProcess, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));

/** A running server. */
export interface TestServer {
  readonly origin: string;
  readonly token: string;
  readonly host: string;
  /** Kills the server process (data dir and port are kept). */
  down(): Promise<void>;
  /** Starts it again on the same port and data dir. */
  up(): Promise<void>;
  stop(): Promise<void>;
}

/** A free port on 127.0.0.1 (the OS picks; never the production port). */
export function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address ? address.port : 0;
      probe.close(() => resolve(port));
    });
  });
}

/** Whether `port` can be bound on loopback right now. */
function portFree(port: number): Promise<boolean> {
  // `Promise.withResolvers` needs lib ES2024; tsconfig targets ES2022.
  return new Promise((resolve) => {
    const probe = createServer();
    probe.once("error", () => resolve(false));
    probe.listen(port, "127.0.0.1", () => probe.close(() => resolve(true)));
  });
}

/**
 * Base port for the linked projects (`PENWISE_PROJECT_PORT`). Tests never bind 7421–7430 (the live service
 * uses them), so start above 17421 and take the first free port.
 */
async function projectPortBase(): Promise<number> {
  for (let port = 17421; port < 17600; port++) {
    if (await portFree(port)) return port;
  }
  throw new Error("no free project port base below 17600");
}

/**
 * Spawns `target/debug/penwise-server` (or `$PENWISE_SERVER_BIN`) serving `web/dist` and waits until it
 * answers. The data dir is removed on stop; `restart` keeps it (and the port).
 */
export async function startServer(): Promise<TestServer> {
  const bin = process.env.PENWISE_SERVER_BIN ?? join(root, "target/debug/penwise-server");
  const port = await freePort();
  const projectBase = await projectPortBase();
  const data = mkdtempSync(join(tmpdir(), "penwise-e2e-"));
  const token = "e2e-token";
  const origin = `http://127.0.0.1:${port}`;
  let child = spawnServer(bin, port, data, token, projectBase);
  await waitReady(child.process, origin, token, child.log);

  const kill = async () => {
    const exited = new Promise((r) => child.process.once("exit", r));
    child.process.kill("SIGTERM");
    await exited;
  };
  return {
    origin,
    token,
    host: `127.0.0.1:${port}`,
    down: kill,
    async up() {
      child = spawnServer(bin, port, data, token, projectBase);
      await waitReady(child.process, origin, token, child.log);
    },
    async stop() {
      if (child.process.exitCode === null) await kill();
      rmSync(data, { recursive: true, force: true });
    },
  };
}

function spawnServer(bin: string, port: number, data: string, token: string, projectBase: number) {
  const process_ = spawn(bin, [], {
    env: {
      ...process.env,
      PENWISE_HOST: "127.0.0.1",
      PENWISE_PORT: String(port),
      PENWISE_DATA: data,
      PENWISE_WEB: join(root, "web/dist"),
      PENWISE_TOKEN: token,
      PENWISE_PROJECT_PORT: String(projectBase),
    },
    stdio: ["ignore", "ignore", "pipe"],
  });
  const log = { text: "" };
  process_.stderr?.on("data", (chunk: Buffer) => {
    log.text += chunk.toString();
  });
  return { process: process_ as ChildProcess, log };
}

async function waitReady(
  child: ChildProcess,
  origin: string,
  token: string,
  log: { text: string },
): Promise<void> {
  const deadline = Date.now() + 30_000;
  for (;;) {
    if (child.exitCode !== null) throw new Error(`penwise-server exited early:\n${log.text}`);
    try {
      const response = await fetch(`${origin}/health?token=${token}`);
      if (response.ok) return;
    } catch {
      /* not listening yet */
    }
    if (Date.now() > deadline) throw new Error(`penwise-server did not start:\n${log.text}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}
