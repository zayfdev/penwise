/** Shared end-to-end harness: the real `penwise-server`, headless Chromium and request helpers. */

import { type Browser, chromium, type Page } from "playwright-core";
import { afterAll, beforeAll, expect } from "vitest";
import { startServer, type TestServer } from "./server";

export interface Probe {
  stats: { committed: number; live: number };
  state(): {
    page: string | null;
    connection: string;
    pending: number;
    view: { scale: number; originX: number; originY: number };
  };
}
declare global {
  interface Window {
    __penwise?: Probe;
  }
}

/** Server + browser for one test file; call at the top of the file (registers the hooks). */
export function useHarness() {
  const h = {
    server: undefined as unknown as TestServer,
    browser: undefined as unknown as Browser,
    scratch: undefined as unknown as Page,
  };
  beforeAll(async () => {
    h.server = await startServer();
    h.browser = await chromium.launch({
      executablePath: process.env.CHROME ?? "/usr/bin/chromium",
      // No GPU: several gates may run at once next to the desktop browser, and GPU Chromiums can exhaust the
      // NVIDIA driver's address space (froze the desktop on 2026-10-28, Xid 56).
      args: ["--no-sandbox", "--disable-gpu"],
    });
    h.scratch = await h.browser.newPage();
  });
  afterAll(async () => {
    await h.browser?.close();
    await h.server?.stop();
  });

  const auth = () => ({ "X-Penwise-Token": h.server.token });

  async function api(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<Record<string, unknown>> {
    const response = await fetch(`${h.server.origin}${path}`, {
      method,
      headers: { ...auth(), "Content-Type": "application/json" },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    expect(response.ok, `${method} ${path}: ${response.status}`).toBe(true);
    return (await response.json()) as Record<string, unknown>;
  }

  async function createPage(title: string): Promise<string> {
    const created = (await api("POST", "/pages", { title })) as { page: { id: string } };
    return created.page.id;
  }

  async function pagePng(page: string): Promise<Buffer> {
    const response = await fetch(`${h.server.origin}/page.png?page=${page}&scale=1`, {
      headers: auth(),
    });
    expect(response.ok).toBe(true);
    return Buffer.from(await response.arrayBuffer());
  }

  async function openViewer(waitForPage?: string): Promise<Page> {
    const viewer = await h.browser.newPage({ viewport: { width: 1400, height: 900 } });
    const problems: string[] = [];
    viewer.on("pageerror", (error) => problems.push(String(error)));
    viewer.on("console", (m) => m.type() === "error" && problems.push(m.text()));
    await viewer.goto(`${h.server.origin}/?token=${h.server.token}`);
    try {
      await viewer.waitForFunction(
        () => window.__penwise?.state().connection === "open",
        undefined,
        {
          timeout: 10_000,
        },
      );
    } catch (error) {
      throw new Error(`viewer never connected: ${problems.join(" | ")} (${String(error)})`);
    }
    if (waitForPage) {
      // The server's current page is shown first; pick ours from the sidebar like a user would.
      await viewer.click(`#pages [data-page="${waitForPage}"]`);
      await viewer.waitForFunction((id) => window.__penwise?.state().page === id, waitForPage);
    }
    expect(problems).toEqual([]);
    return viewer;
  }

  return {
    get server() {
      return h.server;
    },
    get browser() {
      return h.browser;
    },
    get scratch() {
      return h.scratch;
    },
    api,
    createPage,
    pagePng,
    openViewer,
  };
}

export const settle = (viewer: Page) =>
  viewer.waitForFunction(() => window.__penwise?.state().pending === 0);

/** Draws a mouse stroke across the stage; `f` = start x, start y, end x, end y as fractions of the stage. */
export async function drawMouseStroke(
  viewer: Page,
  f: [number, number, number, number],
): Promise<{ box: { x: number; y: number }; x0: number; y0: number; x1: number; y1: number }> {
  await viewer.fill("#tool-color", "#cc0000");
  await viewer.fill("#tool-size", "3");
  const box = await viewer.locator("#stage").boundingBox();
  if (!box) throw new Error("no stage");
  const [x0, y0] = [box.x + box.width * f[0], box.y + box.height * f[1]];
  const [x1, y1] = [box.x + box.width * f[2], box.y + box.height * f[3]];
  await viewer.mouse.move(x0, y0);
  await viewer.mouse.down();
  await viewer.mouse.move((x0 + x1) / 2, (y0 + y1) / 2 + 30, { steps: 12 });
  await viewer.mouse.move(x1, y1, { steps: 12 });
  await viewer.mouse.up();
  return { box, x0, y0, x1, y1 };
}

export const line = (x0: number, y0: number, x1: number, y1: number, n: number) =>
  Array.from({ length: n }, (_, i) => ({
    x: x0 + ((x1 - x0) * i) / (n - 1),
    y: y0 + ((y1 - y0) * i) / (n - 1),
  }));
