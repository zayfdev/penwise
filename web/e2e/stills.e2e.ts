/**
 * Browser end to end: full-length frames (design-harness v2).
 *
 * A frame is as tall as its document and shows the server's still as image strips; only the focused frame has
 * an iframe, at the page's reported scroll. Scrolling that iframe leaves an area mark at the same document
 * position, and moving the camera makes the page scroll to what the camera shows.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { focusFrame, settle, useHarness } from "./harness";

const h = useHarness();

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

/** A long static page: three colour bands, a probe element near the top. */
const longSite = (
  height: number,
) => `<!doctype html><html><head><meta charset="utf-8"><title>long</title></head>
<body style="margin:0"><div id="probe" style="margin-top:100px;height:60px;background:#c33">probe</div>
<div style="height:${height}px;background:linear-gradient(#fdd,#dfd,#ddf)">tall</div></body></html>`;

/** Links a static site; returns the project id and its folder. */
async function linkSite(
  name: string,
  html: string,
): Promise<{ id: string; root: string; port: number }> {
  const root = mkdtempSync(join(tmpdir(), "penwise-stills-"));
  roots.push(root);
  writeFileSync(join(root, "index.html"), html);
  const project = await h.api("POST", "/projects", { name, root, static: root });
  return { id: String(project.id), root, port: Number(project.port) };
}

/** An infinite page with one phone frame (full length by default); returns the page id and the frame id. */
async function pageWithFrame(project: string): Promise<{ page: string; frame: string }> {
  const page = await h.createInfinitePage("stills");
  const created = await h.api("POST", `/pages/${page}/frames`, {
    project,
    route: "/",
    preset: "phone",
  });
  return { page, frame: String(created.id) };
}

const state = (viewer: Page) => viewer.evaluate(() => window.__penwise?.state());

/** The live page inside the frame iframe (the linked site's own port). */
function innerFrame(viewer: Page, port: number) {
  const inner = viewer.frames().find((f) => f.url().startsWith(`http://127.0.0.1:${port}`));
  if (!inner) throw new Error("frame page not found");
  return inner;
}

describe("full-length frames in the PC window", () => {
  it("shows a long page as a box of its document height with image strips and no iframe", async () => {
    const site = await linkSite("stills long", longSite(5840));
    const { page, frame } = await pageWithFrame(site.id);
    const viewer = await h.openViewer(page);
    const box = viewer.locator("#frames .frame");

    // The still arrives: the box grows to the document (5840 + 100 margin + 60 probe = 6000 px)...
    await expect
      .poll(() => box.evaluate((node) => Number.parseFloat(getComputedStyle(node).height)), {
        timeout: 20_000,
      })
      .toBeGreaterThan(5900);
    // ...and shows it as strips of at most 4096 px: 6000 px is two. Only strips near the screen are images, so
    // fit the whole board first.
    // (The server's height reaches the core a moment after the still, so fit until the strips are there.)
    await expect
      .poll(
        async () => {
          await viewer.click("#tool-fit");
          return (await state(viewer))?.stills[frame]?.length ?? 0;
        },
        { timeout: 20_000, interval: 500 },
      )
      .toBe(2);
    await expect
      .poll(() =>
        viewer
          .locator("#frames .stills img")
          .evaluateAll(
            (images) =>
              images.filter((image) => (image as HTMLImageElement).naturalWidth > 0).length,
          ),
      )
      .toBe(2);
    expect(await box.evaluate((node) => node.classList.contains("no-still"))).toBe(false);
    // Nothing is focused, so no iframe exists.
    expect(await viewer.locator("#frames iframe").count()).toBe(0);
    expect((await state(viewer))?.live.iframes).toBe(0);
    await viewer.close();
  });

  it("re-checks the still and follows the document when the site grows", async () => {
    const site = await linkSite("stills grow", longSite(1000));
    const { page } = await pageWithFrame(site.id);
    const viewer = await h.openViewer(page);
    const height = () =>
      viewer
        .locator("#frames .frame")
        .evaluate((node) => Number.parseFloat(getComputedStyle(node).height));
    await expect.poll(height, { timeout: 20_000 }).toBeGreaterThan(1100);
    writeFileSync(join(site.root, "index.html"), longSite(3000));
    await expect.poll(height, { timeout: 30_000 }).toBeGreaterThan(3100);
    await viewer.close();
  });

  it("keeps an area mark at the same document position while the live page scrolls", async () => {
    const site = await linkSite("stills scroll", longSite(4000));
    const { page } = await pageWithFrame(site.id);
    const viewer = await h.openViewer(page);
    await focusFrame(viewer);
    await expect.poll(async () => (await state(viewer))?.layouts, { timeout: 15_000 }).toBe(1);

    // Pin a stroke to the area (not to an element), drawn with the mouse over the live page.
    await viewer.selectOption("#tool-bind", "area");
    await viewer.check("#tool-draw-mouse");
    const stage = await viewer.locator("#stage").boundingBox();
    if (!stage) throw new Error("no stage");
    const [x0, y0] = [stage.x + stage.width * 0.3, stage.y + stage.height * 0.4];
    await viewer.mouse.move(x0, y0);
    await viewer.mouse.down();
    await viewer.mouse.move(x0 + 80, y0 + 20, { steps: 8 });
    await viewer.mouse.up();
    await settle(viewer);
    await expect.poll(async () => (await state(viewer))?.marks.length, { timeout: 10_000 }).toBe(1);

    const inner = innerFrame(viewer, site.port);
    /**
     * Where the mark sits relative to the probe element on screen, px. (Read through `evaluate`: asking
     * Playwright for an iframe element's box first makes the next wheel miss the page.)
     */
    const offset = async () => {
      const s = await state(viewer);
      const rect = s?.marks[0]?.rect;
      const top = await inner.evaluate(
        () => document.getElementById("probe")?.getBoundingClientRect().top,
      );
      const frameTop = await viewer.evaluate(
        () => document.querySelector("#frames iframe")?.getBoundingClientRect().top,
      );
      if (!s || !rect || top === undefined || frameTop === undefined)
        throw new Error("no mark or probe");
      return stage.y + (rect[1] - s.view.originY) * s.view.scale - (frameTop + top * s.view.scale);
    };
    const before = await state(viewer);
    const markBefore = before?.marks[0]?.rect;
    const windowBefore = before?.live.window;
    if (!markBefore || !windowBefore) throw new Error("no mark or window");
    const offsetBefore = await offset();

    // The wheel over the live page scrolls the page itself.
    await viewer.uncheck("#tool-draw-mouse");
    await viewer.mouse.move(stage.x + stage.width / 2, stage.y + stage.height / 2);
    await viewer.mouse.wheel(0, 1500);
    await expect
      .poll(() => inner.evaluate(() => window.scrollY), { timeout: 10_000 })
      .toBeGreaterThan(100);
    const scrollY = () => inner.evaluate(() => window.scrollY);

    // The window follows the page's reported scroll; the camera follows it, the mark stays put.
    await expect
      .poll(
        async () => {
          const s = await state(viewer);
          return s?.live.window
            ? s.live.window[1] - windowBefore[1] - (await scrollY())
            : Number.NaN;
        },
        { timeout: 10_000, interval: 200 },
      )
      .toBeCloseTo(0, 0);
    const after = await state(viewer);
    const markAfter = after?.marks[0]?.rect;
    expect(markAfter?.[1]).toBeCloseTo(markBefore[1], 0);
    expect(markAfter?.[0]).toBeCloseTo(markBefore[0], 0);
    // On screen the mark and the page content moved together.
    expect(Math.abs((await offset()) - offsetBefore)).toBeLessThan(3);
    await viewer.close();
  });

  it("scrolls the live page to what the camera shows after a pan elsewhere", async () => {
    const site = await linkSite("stills camera", longSite(4000));
    const { page } = await pageWithFrame(site.id);
    const viewer = await h.openViewer(page);
    await focusFrame(viewer);
    await expect.poll(async () => (await state(viewer))?.layouts, { timeout: 15_000 }).toBe(1);

    // With "Draw with mouse" the frames let the wheel through to the board: it pans the camera.
    await viewer.check("#tool-draw-mouse");
    const stage = await viewer.locator("#stage").boundingBox();
    if (!stage) throw new Error("no stage");
    await viewer.mouse.move(stage.x + stage.width / 2, stage.y + stage.height / 2);
    await viewer.mouse.wheel(0, 8000);

    const inner = innerFrame(viewer, site.port);
    await expect
      .poll(() => inner.evaluate(() => window.scrollY), { timeout: 10_000 })
      .toBeGreaterThan(1000);
    // Once it settled the window covers the visible part of the frame.
    await expect
      .poll(
        async () => {
          const s = await state(viewer);
          const win = s?.live.window;
          if (!s || !win) return false;
          const top = s.view.originY;
          const bottom = top + stage.height / s.view.scale;
          return win[1] <= top + 1 && win[1] + win[3] >= bottom - 1;
        },
        { timeout: 10_000 },
      )
      .toBe(true);
    // The scroll the camera asked for is not the page's own: the camera did not drift on it.
    const rested = (await state(viewer))?.view.originY ?? 0;
    await new Promise((resolve) => setTimeout(resolve, 600));
    expect((await state(viewer))?.view.originY).toBeCloseTo(rested, 0);
    await viewer.close();
  });

  /** A page with two phone frames of the same site; returns the page and the two frame ids. */
  async function pageWithTwoFrames(site: { id: string }) {
    const page = await h.createInfinitePage("stills two");
    const ids: string[] = [];
    for (let i = 0; i < 2; i += 1) {
      const created = await h.api("POST", `/pages/${page}/frames`, {
        project: site.id,
        route: "/",
        preset: "phone",
        x: i * 500,
        y: 0,
      });
      ids.push(String(created.id));
    }
    return { page, a: String(ids[0]), b: String(ids[1]) };
  }

  const liveFrame = async (viewer: Page) => (await state(viewer))?.live.frame ?? null;

  it("keeps the previous frame warm: invisible at the preset size, and not rebuilt on return", async () => {
    const site = await linkSite("stills warm", longSite(3000));
    const { page, a, b } = await pageWithTwoFrames(site);
    const viewer = await h.openViewer(page);
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(2);
    const indexOf = async (id: string) =>
      (
        await viewer
          .locator("#frames .frame")
          .evaluateAll((nodes) => nodes.map((n) => (n as HTMLElement).dataset.frame))
      ).indexOf(id);

    await viewer.click("#tool-fit");
    await focusFrame(viewer, await indexOf(a));
    expect(await liveFrame(viewer)).toBe(a);
    await viewer.click("#tool-fit");
    await focusFrame(viewer, await indexOf(b));
    expect(await liveFrame(viewer)).toBe(b);

    // `a` is warm: still there, laid out at the preset size, but not seen or reached.
    const warm = viewer.locator(`#frames iframe[data-frame="${a}"]`);
    expect(await warm.count()).toBe(1);
    const style = await warm.evaluate((node) => {
      const css = getComputedStyle(node);
      return {
        visibility: css.visibility,
        width: css.width,
        height: css.height,
        display: css.display,
      };
    });
    expect(style).toEqual({
      visibility: "hidden",
      width: "390px",
      height: "844px",
      display: "block",
    });
    expect((await state(viewer))?.live.iframes).toBe(2);
    const builds = (await state(viewer))?.frameBuilds;
    expect(builds).toBe(2);

    // Going back to it shows the same iframe: nothing is rebuilt.
    await viewer.click("#tool-fit");
    await focusFrame(viewer, await indexOf(a));
    expect(await liveFrame(viewer)).toBe(a);
    expect((await state(viewer))?.frameBuilds).toBe(builds);
    expect(
      await viewer
        .locator(`#frames iframe[data-frame="${a}"]`)
        .evaluate((n) => getComputedStyle(n).visibility),
    ).toBe("visible");
    await viewer.close();
  });

  it("keeps a mark below the first screen inside its box after the frame loses its window", async () => {
    const site = await linkSite("stills demote", longSite(4000));
    const { page, a, b } = await pageWithTwoFrames(site);
    const viewer = await h.openViewer(page);
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(2);
    const indexOf = async (id: string) =>
      (
        await viewer
          .locator("#frames .frame")
          .evaluateAll((nodes) => nodes.map((n) => (n as HTMLElement).dataset.frame))
      ).indexOf(id);
    await viewer.click("#tool-fit");
    await focusFrame(viewer, await indexOf(a));
    await expect.poll(async () => (await state(viewer))?.layouts, { timeout: 15_000 }).toBe(1);

    // Scroll the live page well past its first screen, then pin a stroke there.
    const stage = await viewer.locator("#stage").boundingBox();
    if (!stage) throw new Error("no stage");
    await viewer.mouse.move(stage.x + stage.width / 2, stage.y + stage.height / 2);
    await viewer.mouse.wheel(0, 2000);
    await expect
      .poll(() => innerFrame(viewer, site.port).evaluate(() => window.scrollY), { timeout: 10_000 })
      .toBeGreaterThan(1000);
    await viewer.selectOption("#tool-bind", "area");
    await viewer.check("#tool-draw-mouse");
    await viewer.mouse.move(stage.x + stage.width * 0.4, stage.y + stage.height * 0.4);
    await viewer.mouse.down();
    await viewer.mouse.move(stage.x + stage.width * 0.4 + 60, stage.y + stage.height * 0.4 + 20, {
      steps: 6,
    });
    await viewer.mouse.up();
    await settle(viewer);
    await expect.poll(async () => (await state(viewer))?.marks.length, { timeout: 10_000 }).toBe(1);
    await viewer.uncheck("#tool-draw-mouse");

    // Leave the frame: its layout is the still's now. The mark is still drawn, inside the frame's box.
    await viewer.click("#tool-fit");
    await focusFrame(viewer, await indexOf(b));
    const box = await viewer.locator(`#frames .frame[data-frame="${a}"]`).evaluate((n) => ({
      top: Number.parseFloat((n as HTMLElement).style.top),
      height: Number.parseFloat((n as HTMLElement).style.height),
    }));
    const rect = (await state(viewer))?.marks[0]?.rect;
    expect(rect).toBeTruthy();
    if (!rect) return;
    expect(rect[1]).toBeGreaterThan(box.top + 844);
    expect(rect[1] + rect[3]).toBeLessThanOrEqual(box.top + box.height);
    await viewer.close();
  });
});
