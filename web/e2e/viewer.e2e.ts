/**
 * Browser end to end: the real `penwise-server` binary, the built viewer (`web/dist`) and headless Chromium.
 *
 * - another device's stroke shows live in the live canvas (without repainting the committed layer), then
 *   committed; a server-authored (`/annotate`) stroke appears too;
 * - a stroke drawn with the mouse in the viewer lands in the server's `/page.png` (and survives undo/redo);
 * - a second device sees the browser's stroke.
 */

import { type Browser, chromium, type Page } from "playwright-core";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { nodeWasm, Peer } from "./peer";
import { canvasStats, pngStats } from "./pixels";
import { startServer, type TestServer } from "./server";

let server: TestServer;
let browser: Browser;
let scratch: Page;
const wasm = nodeWasm();

beforeAll(async () => {
  server = await startServer();
  browser = await chromium.launch({
    executablePath: process.env.CHROME ?? "/usr/bin/chromium",
    args: ["--no-sandbox"],
  });
  scratch = await browser.newPage();
});

afterAll(async () => {
  await browser?.close();
  await server?.stop();
});

const auth = () => ({ "X-Penwise-Token": server.token });

async function api(method: string, path: string, body?: unknown): Promise<Record<string, unknown>> {
  const response = await fetch(`${server.origin}${path}`, {
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
  const response = await fetch(`${server.origin}/page.png?page=${page}&scale=1`, {
    headers: auth(),
  });
  expect(response.ok).toBe(true);
  return Buffer.from(await response.arrayBuffer());
}

interface Probe {
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

async function openViewer(waitForPage?: string): Promise<Page> {
  const viewer = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const problems: string[] = [];
  viewer.on("pageerror", (error) => problems.push(String(error)));
  viewer.on("console", (m) => m.type() === "error" && problems.push(m.text()));
  await viewer.goto(`${server.origin}/?token=${server.token}`);
  try {
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "open", undefined, {
      timeout: 10_000,
    });
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

const settle = (viewer: Page) =>
  viewer.waitForFunction(() => window.__penwise?.state().pending === 0);

/** Draws a mouse stroke across the stage; `f` = start x, start y, end x, end y as fractions of the stage. */
async function drawMouseStroke(
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

const line = (x0: number, y0: number, x1: number, y1: number, n: number) =>
  Array.from({ length: n }, (_, i) => ({
    x: x0 + ((x1 - x0) * i) / (n - 1),
    y: y0 + ((y1 - y0) * i) / (n - 1),
  }));

describe("viewer against the real server", () => {
  it("loads the built bundle from the server (wasm served as application/wasm)", async () => {
    const html = await fetch(`${server.origin}/`);
    expect(html.status).toBe(200);
    const text = await html.text();
    const script = /src="(\/static\/[^"]+\.js)"/.exec(text)?.[1];
    expect(script).toBeTruthy();
    const js = await fetch(`${server.origin}${script}`);
    expect(js.headers.get("content-type")).toContain("javascript");
    expect(text).not.toContain("app.js");
  });

  it("shows another device's stroke live without repainting the committed layer, then committed", async () => {
    const id = await createPage("peer");
    const viewer = await openViewer(id);
    const peer = await Peer.connect(wasm, server.origin, server.token);
    await peer.watch(id);

    let committedBefore = -1;
    let liveBefore = -1;
    await peer.stroke(id, line(400, 500, 2000, 900, 96), {
      chunk: 8,
      pause: 40,
      beforeCommit: async () => {
        await expect
          .poll(async () => (await canvasStats(viewer, "live", "red")).count, { timeout: 5_000 })
          .toBeGreaterThan(0);
        expect((await canvasStats(viewer, "committed", "red")).count).toBe(0);
        const stats = await viewer.evaluate(() => ({ ...window.__penwise?.stats }));
        committedBefore = stats.committed ?? -1;
        liveBefore = stats.live ?? -1;
        // More live frames arrive while we look: only the live layer repaints.
        await new Promise((r) => setTimeout(r, 300));
        const after = await viewer.evaluate(() => ({ ...window.__penwise?.stats }));
        expect(after.committed).toBe(committedBefore);
        expect(after.live).toBeGreaterThanOrEqual(liveBefore);
      },
    });

    await expect
      .poll(async () => (await canvasStats(viewer, "committed", "red")).count, { timeout: 5_000 })
      .toBeGreaterThan(50);
    await expect
      .poll(async () => (await canvasStats(viewer, "live", "red")).count, { timeout: 5_000 })
      .toBe(0);
    const after = await viewer.evaluate(() => ({ ...window.__penwise?.stats }));
    expect(after.committed).toBeGreaterThan(committedBefore);
    peer.close();
    await viewer.close();
  });

  it("shows a server-authored stroke (/annotate)", async () => {
    const id = await createPage("annotate");
    const viewer = await openViewer(id);
    await api("POST", "/annotate", {
      page: id,
      strokes: [
        {
          points: [
            [300, 300],
            [1200, 700],
            [2200, 400],
          ],
          color: "#0000ff",
          width: 8,
        },
      ],
    });
    await expect
      .poll(async () => (await canvasStats(viewer, "committed", "blue")).count, { timeout: 5_000 })
      .toBeGreaterThan(50);
    await viewer.close();
  });

  it("a stroke drawn in the browser lands in /page.png, and undo/redo reach the server", async () => {
    const id = await createPage("draw");
    const viewer = await openViewer(id);
    const peer = await Peer.connect(wasm, server.origin, server.token);
    await peer.watch(id);

    const { box, x0, y0, x1, y1 } = await drawMouseStroke(viewer, [0.3, 0.4, 0.7, 0.6]);
    await settle(viewer);

    const { view } = await viewer.evaluate(() => ({ view: window.__penwise?.state().view }));
    if (!view) throw new Error("no view");
    const toPage = (cx: number, cy: number) => [
      view.originX + (cx - box.x) / view.scale,
      view.originY + (cy - box.y) / view.scale,
    ];
    const [px0, py0] = toPage(x0, y0);
    const [px1] = toPage(x1, y1);

    const drawn = await pngStats(scratch, await pagePng(id), "red");
    expect(drawn.count).toBeGreaterThan(200);
    const bbox = drawn.bbox ?? [0, 0, 0, 0];
    expect(Math.abs(bbox[0] - (px0 ?? 0))).toBeLessThan(25);
    expect(Math.abs(bbox[2] - (px1 ?? 0))).toBeLessThan(25);
    expect(bbox[1]).toBeGreaterThan((py0 ?? 0) - 40 - 25);

    // The browser's own canvas shows it too, and the second device received it.
    expect((await canvasStats(viewer, "committed", "red")).count).toBeGreaterThan(50);
    await expect
      .poll(() => peer.viewer.render_page_paths(id, 7).commands().length, { timeout: 5_000 })
      .toBeGreaterThan(0);

    // Undo (Ctrl+Z) removes it on the server; redo brings it back.
    await viewer.keyboard.press("Control+z");
    await settle(viewer);
    await expect
      .poll(async () => (await pngStats(scratch, await pagePng(id), "red")).count, {
        timeout: 5_000,
      })
      .toBe(0);
    await viewer.keyboard.press("Control+Shift+z");
    await settle(viewer);
    await expect
      .poll(async () => (await pngStats(scratch, await pagePng(id), "red")).count, {
        timeout: 5_000,
      })
      .toBeGreaterThan(200);
    peer.close();
    await viewer.close();
  });

  it("keeps a stroke drawn while the server is down and delivers it after reconnecting", async () => {
    const id = await createPage("offline");
    const viewer = await openViewer(id);
    await server.down();
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "closed");
    await drawMouseStroke(viewer, [0.3, 0.4, 0.7, 0.5]);
    expect(await viewer.evaluate(() => window.__penwise?.state().pending)).toBe(1);
    await server.up();
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "open", undefined, {
      timeout: 20_000,
    });
    await settle(viewer);
    const stats = await pngStats(scratch, await pagePng(id), "red");
    expect(stats.count).toBeGreaterThan(100);
    // The reconnect rebuilt the page from a snapshot and our stroke is still on it.
    expect((await canvasStats(viewer, "committed", "red")).count).toBeGreaterThan(50);
    await viewer.close();
  });

  it("lists pages from the catalog, creates one over HTTP and opens it", async () => {
    const viewer = await openViewer();
    await expect.poll(() => viewer.locator("#pages .page").count()).toBeGreaterThan(0);
    const before = await viewer.locator("#pages .page").count();
    await viewer.fill("#sidebar .form input", "Brand new");
    await viewer.click("#add-page");
    await expect.poll(() => viewer.locator("#pages .page").count()).toBe(before + 1);
    await expect.poll(() => viewer.locator("#pages .page.active").textContent()).toBe("Brand new");
    await viewer.close();
  });
});
