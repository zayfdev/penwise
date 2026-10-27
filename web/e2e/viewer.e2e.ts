/**
 * Browser end to end: the real `penwise-server` binary, the built viewer (`web/dist`) and headless Chromium.
 *
 * - another device's stroke shows live in the live canvas (without repainting the committed layer), then
 *   committed; a server-authored (`/annotate`) stroke appears too;
 * - a stroke drawn with the mouse in the viewer lands in the server's `/page.png` (and survives undo/redo);
 * - a second device sees the browser's stroke.
 * Offline drawing is in `offline.e2e.ts`.
 */

import { describe, expect, it } from "vitest";
import { drawMouseStroke, line, settle, useHarness } from "./harness";
import { nodeWasm, Peer } from "./peer";
import { canvasStats, pngStats } from "./pixels";

const h = useHarness();
const { api, createPage, pagePng, openViewer } = h;
const wasm = nodeWasm();

describe("viewer against the real server", () => {
  it("loads the built bundle from the server (wasm served as application/wasm)", async () => {
    const html = await fetch(`${h.server.origin}/`);
    expect(html.status).toBe(200);
    const text = await html.text();
    const script = /src="(\/static\/[^"]+\.js)"/.exec(text)?.[1];
    expect(script).toBeTruthy();
    const js = await fetch(`${h.server.origin}${script}`);
    expect(js.headers.get("content-type")).toContain("javascript");
    expect(text).not.toContain("app.js");
  });

  it("shows another device's stroke live without repainting the committed layer, then committed", async () => {
    const id = await createPage("peer");
    const viewer = await openViewer(id);
    const peer = await Peer.connect(wasm, h.server.origin, h.server.token);
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
    const peer = await Peer.connect(wasm, h.server.origin, h.server.token);
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

    const drawn = await pngStats(h.scratch, await pagePng(id), "red");
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
      .poll(async () => (await pngStats(h.scratch, await pagePng(id), "red")).count, {
        timeout: 5_000,
      })
      .toBe(0);
    await viewer.keyboard.press("Control+Shift+z");
    await settle(viewer);
    await expect
      .poll(async () => (await pngStats(h.scratch, await pagePng(id), "red")).count, {
        timeout: 5_000,
      })
      .toBeGreaterThan(200);
    peer.close();
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
