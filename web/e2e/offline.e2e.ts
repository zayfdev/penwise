/**
 * Offline drawing: strokes made while the server is unreachable survive reconnects, a page reload and even
 * the tab being closed (another tab adopts the saved outbox), and are delivered once the server is back.
 */

import type { WebSocketRoute } from "playwright-core";
import { describe, expect, it } from "vitest";
import { drawMouseStroke, settle, useHarness } from "./harness";
import { canvasStats, pngStats } from "./pixels";

const h = useHarness();
const { createPage, pagePng, openViewer } = h;

describe("offline drawing", () => {
  it("keeps a stroke drawn while the server is down and delivers it after reconnecting", async () => {
    const id = await createPage("offline");
    const viewer = await openViewer(id);
    await h.server.down();
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "closed");
    await drawMouseStroke(viewer, [0.3, 0.4, 0.7, 0.5]);
    expect(await viewer.evaluate(() => window.__penwise?.state().pending)).toBe(1);
    await h.server.up();
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "open", undefined, {
      timeout: 20_000,
    });
    await settle(viewer);
    const stats = await pngStats(h.scratch, await pagePng(id), "red");
    expect(stats.count).toBeGreaterThan(100);
    // The reconnect rebuilt the page from a snapshot and our stroke is still on it.
    expect((await canvasStats(viewer, "committed", "red")).count).toBeGreaterThan(50);
    await viewer.close();
  });

  it("keeps a stroke drawn offline across a reload and delivers it after reconnecting", async () => {
    const id = await createPage("reload");
    const viewer = await h.browser.newPage({ viewport: { width: 1400, height: 900 } });
    // The page can always load over HTTP; only its /sync socket is cut and restored by the test.
    let blocked = false;
    const live: WebSocketRoute[] = [];
    await viewer.routeWebSocket(/\/sync/, (ws) => {
      if (blocked) {
        ws.close();
        return;
      }
      live.push(ws);
      ws.connectToServer();
    });
    await viewer.goto(`${h.server.origin}/?token=${h.server.token}`);
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "open");
    await viewer.click(`#pages [data-page="${id}"]`);
    await viewer.waitForFunction((page) => window.__penwise?.state().page === page, id);

    blocked = true;
    for (const ws of live) await ws.close();
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "closed");
    await drawMouseStroke(viewer, [0.3, 0.4, 0.7, 0.5]);
    expect(await viewer.evaluate(() => window.__penwise?.state().pending)).toBe(1);
    // Leaving with an unsent stroke asks for confirmation.
    const askedToStay = await viewer.evaluate(
      () => !window.dispatchEvent(new Event("beforeunload", { cancelable: true })),
    );
    expect(askedToStay).toBe(true);

    await viewer.reload();
    await viewer.waitForFunction(() => window.__penwise !== undefined);
    expect(await viewer.evaluate(() => window.__penwise?.state().pending)).toBe(1);

    blocked = false;
    await viewer.waitForFunction(() => window.__penwise?.state().connection === "open", undefined, {
      timeout: 25_000,
    });
    await settle(viewer);
    const stats = await pngStats(h.scratch, await pagePng(id), "red");
    expect(stats.count).toBeGreaterThan(100);
    const calm = await viewer.evaluate(() =>
      window.dispatchEvent(new Event("beforeunload", { cancelable: true })),
    );
    expect(calm).toBe(true);
    await viewer.close();
  });

  it("hands a closed tab's unsent stroke to the next tab, which delivers it", async () => {
    const id = await createPage("closed-tab");
    const context = await h.browser.newContext({ viewport: { width: 1400, height: 900 } });
    let blocked = false;
    const live: WebSocketRoute[] = [];
    await context.routeWebSocket(/\/sync/, (ws) => {
      if (blocked) {
        ws.close();
        return;
      }
      live.push(ws);
      ws.connectToServer();
    });
    const url = `${h.server.origin}/?token=${h.server.token}`;
    const a = await context.newPage();
    await a.goto(url);
    await a.waitForFunction(() => window.__penwise?.state().connection === "open");
    await a.click(`#pages [data-page="${id}"]`);
    await a.waitForFunction((page) => window.__penwise?.state().page === page, id);
    blocked = true;
    for (const ws of live) await ws.close();
    await a.waitForFunction(() => window.__penwise?.state().connection === "closed");
    await drawMouseStroke(a, [0.3, 0.4, 0.7, 0.5]);
    expect(await a.evaluate(() => window.__penwise?.state().pending)).toBe(1);
    await a.close();

    // A brand-new tab (no sessionStorage) adopts the closed tab's device and outbox.
    const b = await context.newPage();
    await b.goto(url);
    await b.waitForFunction(() => window.__penwise !== undefined);
    expect(await b.evaluate(() => window.__penwise?.state().pending)).toBe(1);
    blocked = false;
    await b.waitForFunction(() => window.__penwise?.state().connection === "open", undefined, {
      timeout: 25_000,
    });
    await settle(b);
    expect((await pngStats(h.scratch, await pagePng(id), "red")).count).toBeGreaterThan(100);
    await context.close();
  });
});
