/**
 * Browser end to end: frames on a canvas page.
 *
 * A frame created over HTTP (`POST /pages/:page/frames`) shows the linked static site in an iframe at its preset
 * size, scaled by the canvas view; the wheel scrolls the page inside the frame, Ctrl+wheel zooms the canvas and
 * the chip switches the preset.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { focusFrame, settle, useHarness } from "./harness";

const h = useHarness();

/** A tall static site: enough height for a wheel to scroll it. */
const SITE = `<!doctype html><html><head><meta charset="utf-8"><title>e2e site</title></head>
<body><h1 id="marker">Penwise e2e site</h1><div style="height:4000px">tall</div></body></html>`;

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

/** Links a throwaway static folder as a project; returns its id and live port. */
async function linkSite(name: string): Promise<{ id: string; port: number }> {
  const root = mkdtempSync(join(tmpdir(), "penwise-frames-"));
  roots.push(root);
  writeFileSync(join(root, "index.html"), SITE);
  const project = await h.api("POST", "/projects", { name, root, static: root });
  return { id: String(project.id), port: Number(project.port) };
}

/** A page with one server-created frame. */
async function pageWithFrame(project: string, preset: string): Promise<string> {
  const id = await h.createPage(`frames ${preset}`);
  const created = await h.api("POST", `/pages/${id}/frames`, { project, route: "/", preset });
  expect(typeof created.id).toBe("string");
  return id;
}

const viewScale = (page: Page) => page.evaluate(() => window.__penwise?.state().view.scale ?? 0);

describe("frames in the PC window", () => {
  it("shows an HTTP-created frame as an iframe at its preset, scaled by the view", async () => {
    const site = await linkSite("e2e static");
    const page = await pageWithFrame(site.id, "phone");
    const viewer = await h.openViewer(page);

    // A phone is far under 60 % of the stage at the fit view: it is a still, with no iframe, until focused.
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);
    expect(await viewer.locator("#frames iframe").count()).toBe(0);
    await focusFrame(viewer);

    const iframe = viewer.locator("#frames iframe");
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(1);
    // The URL is the project's own port on the host the client used; loopback needs no enter step.
    await expect
      .poll(() => iframe.getAttribute("src"), { timeout: 10_000 })
      .toBe(`http://127.0.0.1:${site.port}/`);
    // The iframe keeps the preset size in CSS px, so the site sees a 390-wide viewport...
    // Poll: the node must never be read while a transient re-render has detached it.
    await expect
      .poll(() => iframe.evaluate((node) => getComputedStyle(node).width), { timeout: 10_000 })
      .toBe("390px");
    // ...and the canvas view scales it on screen.
    const scale = await viewScale(viewer);
    const box = await iframe.boundingBox();
    expect(box?.width).toBeCloseTo(390 * scale, 0);
    expect(box?.height).toBeCloseTo(844 * scale, 0);
    // The site really runs in it, and the chip names it.
    await expect
      .poll(() => viewer.frameLocator("#frames iframe").locator("#marker").textContent())
      .toBe("Penwise e2e site");
    expect(await viewer.locator("#frame-chips .frame-chip").count()).toBe(1);
    // The live site must not be reloaded by a transient refresh (snapshot/refreshProjects race).
    expect(await viewer.evaluate(() => window.__penwise?.state().frameBuilds)).toBe(1);
    await viewer.close();
  });

  it("does not rebuild a frame when the shown page is opened again", async () => {
    const site = await linkSite("e2e reshow");
    const page = await pageWithFrame(site.id, "phone");
    const viewer = await h.openViewer(page);
    const iframe = viewer.locator("#frames iframe");
    // The viewer may have auto-shown another page first; clicking ours is then a real switch.
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);
    await focusFrame(viewer);
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(1);
    await settle(viewer);
    const shown = await iframe.elementHandle();
    if (!shown) throw new Error("no frame iframe");
    const builds = () => viewer.evaluate(() => window.__penwise?.state().frameBuilds ?? 0);
    const before = await builds();

    // Re-opening the page that is already shown is a no-op for its frames: no re-watch, no snapshot,
    // no reload of the live site.
    await viewer.click(`#pages [data-page="${page}"]`);
    await settle(viewer);
    expect(await builds()).toBe(before);
    await expect.poll(() => shown.evaluate((node) => node.isConnected)).toBe(true);
    await expect
      .poll(() => iframe.evaluate((node) => getComputedStyle(node).width), { timeout: 10_000 })
      .toBe("390px");
    await viewer.close();
  });

  it("adds a frame from the sidebar and undoes it", async () => {
    const site = await linkSite("e2e add");
    const page = await h.createPage("add frame");
    const viewer = await h.openViewer(page);

    const iframe = viewer.locator("#frames iframe");
    expect(await iframe.count()).toBe(0);
    // The project list comes from GET /projects.
    await expect
      .poll(() => viewer.locator("#frame-project option").count(), { timeout: 10_000 })
      .toBeGreaterThan(0);
    await viewer.selectOption("#frame-project", site.id);
    await viewer.fill("#frame-route", "/");
    await viewer.selectOption("#frame-preset", "phone");
    await viewer.click("#add-frame");

    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);
    await focusFrame(viewer);
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(1);
    await expect
      .poll(() => iframe.getAttribute("src"), { timeout: 10_000 })
      .toBe(`http://127.0.0.1:${site.port}/`);
    expect(await iframe.evaluate((node) => getComputedStyle(node).width)).toBe("390px");

    // It is an ordinary item: undo removes it from the page and the screen.
    await viewer.keyboard.press("Control+z");
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(0);
    await viewer.close();
  });

  it("switches the preset and the route from the frame chip", async () => {
    const site = await linkSite("e2e switch");
    const page = await pageWithFrame(site.id, "phone");
    const viewer = await h.openViewer(page);

    const iframe = viewer.locator("#frames iframe");
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);
    await focusFrame(viewer);
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(1);
    await viewer.locator("#frame-chips .frame-chip select").selectOption("desktop");
    await expect
      .poll(() => iframe.evaluate((node) => getComputedStyle(node).width), { timeout: 10_000 })
      .toBe("1440px");
    expect(await iframe.evaluate((node) => getComputedStyle(node).height)).toBe("900px");

    // The route field is an ordinary frame edit too (set_frame_route).
    const route = viewer.locator('#frame-chips .frame-chip input[type="text"]');
    await route.fill("/index.html");
    await route.press("Tab");
    await expect
      .poll(() => iframe.getAttribute("src"), { timeout: 10_000 })
      .toBe(`http://127.0.0.1:${site.port}/index.html`);
    await viewer.close();
  });

  it("scrolls the frame page with the wheel and zooms the canvas with Ctrl+wheel", async () => {
    const site = await linkSite("e2e scroll");
    const page = await pageWithFrame(site.id, "phone");
    const viewer = await h.openViewer(page);

    const iframe = viewer.locator("#frames iframe");
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);
    await focusFrame(viewer);
    await expect.poll(() => iframe.count(), { timeout: 10_000 }).toBe(1);
    const box = await iframe.boundingBox();
    if (!box) throw new Error("no frame box");
    // The mouse wheel goes to the page inside the frame.
    await viewer.mouse.move(box.x + 40, box.y + 40);
    await viewer.mouse.wheel(0, 700);
    const inner = viewer.frames().find((f) => f.url().startsWith(`http://127.0.0.1:${site.port}`));
    if (!inner) throw new Error("frame page not found");
    await expect
      .poll(() => inner.evaluate(() => window.scrollY), { timeout: 5_000 })
      .toBeGreaterThan(100);

    // Ctrl+wheel zooms the canvas instead (the frames let the event through to the stage).
    const before = await viewScale(viewer);
    await viewer.keyboard.down("Control");
    await viewer.mouse.wheel(0, -300);
    await viewer.keyboard.up("Control");
    expect(await viewScale(viewer)).toBeGreaterThan(before);
    await viewer.close();
  });
});
