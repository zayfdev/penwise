/**
 * Browser end to end: a board opens on one readable frame (the frame's width fills the stage), "Board" shows the
 * whole board and "Fit" returns to the current frame.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { useHarness } from "./harness";

const h = useHarness();
const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

const SITE = `<!doctype html><html><head><meta charset="utf-8"><title>tall</title></head>
<body style="margin:0"><h1>tall site</h1><div style="height:4000px">tall</div></body></html>`;

const scale = async (viewer: Page): Promise<number> =>
  (await viewer.evaluate(() => window.__penwise?.state().view.scale)) ?? 0;

describe("Readable frame", () => {
  it("opens on one frame's width, Board shows everything, Fit comes back", async () => {
    const root = mkdtempSync(join(tmpdir(), "penwise-readable-"));
    roots.push(root);
    writeFileSync(join(root, "index.html"), SITE);
    const project = await h.api("POST", "/projects", { name: "readable", root, static: root });
    const blank = await h.createPage("blank");
    const target = await h.createInfinitePage("target");
    for (const preset of ["desktop", "desktop"]) {
      await h.api("POST", `/pages/${target}/frames`, {
        project: String(project.id),
        route: "/",
        preset,
      });
    }
    const viewer = await h.openViewer(blank);
    await viewer.click(`#pages [data-page="${target}"]`);
    await expect
      .poll(async () => await viewer.locator("#frames .frame").count(), { timeout: 10_000 })
      .toBe(2);

    const stage = (await viewer.locator("#stage").boundingBox())?.width ?? 0;
    // The frame is 1440 css px wide: the camera fits its width, not the (very tall) board.
    await expect.poll(() => scale(viewer), { timeout: 10_000 }).toBeCloseTo(stage / 1440, 2);

    await viewer.click("#tool-board");
    await expect.poll(() => scale(viewer), { timeout: 5_000 }).toBeLessThan((stage / 1440) * 0.95);
    await viewer.click("#tool-fit");
    await expect.poll(() => scale(viewer), { timeout: 5_000 }).toBeCloseTo(stage / 1440, 2);
    await viewer.close();
  });
});
