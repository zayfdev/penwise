/**
 * Browser end to end: the camera follows another device (design-harness v2 contract §3).
 *
 * A `Focus` from another device switches the page and fits the frame (which becomes the live one). While the
 * owner has just drawn here, the move waits behind an "Agent: open … →" chip until it is pressed.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { drawMouseStroke, useHarness } from "./harness";
import { nodeWasm, Peer } from "./peer";

const h = useHarness();

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

const SITE = `<!doctype html><html><head><meta charset="utf-8"><title>focus</title></head>
<body style="margin:0"><h1>focus site</h1><div style="height:3000px">tall</div></body></html>`;

/** A page with one labelled frame to be focused, and a blank page to start on. */
async function setup(): Promise<{ blank: string; target: string; frame: string }> {
  const root = mkdtempSync(join(tmpdir(), "penwise-focus-"));
  roots.push(root);
  writeFileSync(join(root, "index.html"), SITE);
  const project = await h.api("POST", "/projects", { name: "focus", root, static: root });
  const blank = await h.createPage("blank");
  const target = await h.createPage("target");
  const created = await h.api("POST", `/pages/${target}/frames`, {
    project: String(project.id),
    route: "/",
    preset: "phone",
  });
  return { blank, target, frame: String(created.id) };
}

/** Picking a page in the sidebar leaves "Follow tablet" (the owner chose); switch it back on. */
async function openFollowing(page: string): Promise<Page> {
  const viewer = await h.openViewer(page);
  await viewer.selectOption("#tool-mode", "follow");
  return viewer;
}

const state = (viewer: Page) => viewer.evaluate(() => window.__penwise?.state());

describe("Focus from another device", () => {
  it("switches the page and fits the frame, which becomes the live one", async (ctx) => {
    const { blank, target, frame } = await setup();
    const viewer = await openFollowing(blank);
    const peer = await Peer.connect(nodeWasm(), h.server.origin, h.server.token);
    if (!peer.focus(target, frame)) ctx.skip("the server does not report protocol 2 (Focus)");

    await expect.poll(async () => (await state(viewer))?.page, { timeout: 10_000 }).toBe(target);
    await expect
      .poll(async () => (await state(viewer))?.live.frame, { timeout: 10_000 })
      .toBe(frame);
    // Fit to the stage width (after the glide): the 390 px phone fills the stage.
    const stage = await viewer.locator("#stage").boundingBox();
    await expect
      .poll(async () => (await viewer.locator("#frames iframe").boundingBox())?.width ?? 0, {
        timeout: 5_000,
      })
      .toBeCloseTo(stage?.width ?? 0, -1);
    expect(await viewer.locator("#agent-chip").isVisible()).toBe(false);
    peer.close();
    await viewer.close();
  });

  it("fits the page's content for an item that is not a frame", async (ctx) => {
    const { blank, target } = await setup();
    const viewer = await openFollowing(blank);
    const peer = await Peer.connect(nodeWasm(), h.server.origin, h.server.token);
    if (!peer.focus(target, null)) ctx.skip("the server does not report protocol 2 (Focus)");
    await expect.poll(async () => (await state(viewer))?.page, { timeout: 10_000 }).toBe(target);
    expect((await state(viewer))?.live.frame).toBeNull();
    peer.close();
    await viewer.close();
  });

  it("waits behind a chip while the owner has just drawn", async (ctx) => {
    const { blank, target, frame } = await setup();
    const viewer = await openFollowing(blank);
    await drawMouseStroke(viewer, [0.2, 0.3, 0.5, 0.5]);
    const peer = await Peer.connect(nodeWasm(), h.server.origin, h.server.token);
    if (!peer.focus(target, frame)) ctx.skip("the server does not report protocol 2 (Focus)");

    const chip = viewer.locator("#agent-chip");
    await expect.poll(() => chip.isVisible(), { timeout: 10_000 }).toBe(true);
    expect(await chip.textContent()).toMatch(/^Agent: open .+ →$/);
    // Nothing moved under the pen.
    expect((await state(viewer))?.page).toBe(blank);

    await chip.click();
    await expect.poll(async () => (await state(viewer))?.page, { timeout: 10_000 }).toBe(target);
    await expect
      .poll(async () => (await state(viewer))?.live.frame, { timeout: 10_000 })
      .toBe(frame);
    expect(await chip.isVisible()).toBe(false);
    peer.close();
    await viewer.close();
  });
});
