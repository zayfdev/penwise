/**
 * Browser end to end: a mark drawn over a live frame follows its element.
 *
 * A stroke drawn with the mouse ("Draw with mouse", so the pen path decides the binding) over the framed site
 * binds to the element under the pen-down point. Editing the fixture's CSS moves the element: after the
 * automatic reload the mark's effective rect (via `Viewer.marks`) moved by the same delta. Removing the
 * element hides the mark (`rect: null`).
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { settle, useHarness } from "./harness";

const h = useHarness();

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

/** The framed site: one element at a known position that the test moves and then removes. */
function site(heroOffsetPx: number | null): string {
  const hero =
    heroOffsetPx === null
      ? ""
      : `<div id="hero" style="margin:40px 0 0 ${heroOffsetPx}px;width:200px;height:120px;background:#ddd">
           hero
         </div>`;
  return `<!doctype html><html><head><meta charset="utf-8"><title>marks site</title></head>
<body style="margin:0">${hero}<div style="height:2000px">tall</div></body></html>`;
}

/** A menu the test opens and closes: its visible state is what marks remember. */
const MENU_SITE = `<!doctype html><html><head><meta charset="utf-8"><title>menu site</title>
<style>body{margin:0}#panel{margin:0 20px;width:250px;height:120px;background:#eee}</style></head>
<body>
<button id="menu" aria-expanded="false" aria-controls="panel" style="margin:20px">Menu</button>
<div id="panel" hidden>panel</div>
<div style="height:1500px"></div>
<script>
  const button = document.getElementById("menu");
  const panel = document.getElementById("panel");
  button.addEventListener("click", () => {
    const open = button.getAttribute("aria-expanded") === "true";
    button.setAttribute("aria-expanded", String(!open));
    panel.hidden = open;
  });
</script>
</body></html>`;

/** Links a throwaway static folder as a project; returns its id and root on disk. */
async function linkSite(
  name: string,
  html: string = site(20),
): Promise<{ id: string; root: string }> {
  const root = mkdtempSync(join(tmpdir(), "penwise-marks-"));
  roots.push(root);
  writeFileSync(join(root, "index.html"), html);
  const project = await h.api("POST", "/projects", { name, root, static: root });
  return { id: String(project.id), root };
}

/** A page with one server-created phone frame of the project. */
async function pageWithFrame(project: string): Promise<string> {
  const id = await h.createPage("marks");
  await h.api("POST", `/pages/${id}/frames`, { project, route: "/", preset: "phone" });
  return id;
}

/** Draws a short stroke through the middle of the framed site's `#hero` element. */
async function drawOnHero(viewer: Page): Promise<void> {
  await viewer.check("#tool-draw-mouse");
  const hero = await viewer.frameLocator("#frames iframe").locator("#hero").boundingBox();
  if (!hero) throw new Error("hero is not on screen");
  const [cx, cy] = [hero.x + hero.width / 2, hero.y + hero.height / 2];
  await viewer.mouse.move(cx - 15, cy - 5);
  await viewer.mouse.down();
  await viewer.mouse.move(cx, cy, { steps: 6 });
  await viewer.mouse.move(cx + 15, cy + 5, { steps: 6 });
  await viewer.mouse.up();
  await settle(viewer);
}

describe("marks in the PC window", () => {
  it("follows its element across a CSS edit and hides when the element is removed", async () => {
    const project = await linkSite("marks static");
    const pageId = await pageWithFrame(project.id);
    const viewer = await h.openViewer(pageId);

    // The injected bridge reports the frame's layout; until then a bound stroke has nothing to bind to.
    await expect
      .poll(() => viewer.evaluate(() => window.__penwise?.state().layouts ?? 0), {
        timeout: 15_000,
      })
      .toBe(1);

    await drawOnHero(viewer);
    const marks = () => viewer.evaluate(() => window.__penwise?.state().marks ?? []);
    await expect.poll(async () => (await marks()).length, { timeout: 10_000 }).toBe(1);
    const first = (await marks())[0];
    expect(first?.status).toBe("new");
    expect(first?.preset).toBe("phone");
    expect(first?.rect).not.toBeNull();
    expect(await viewer.locator("#mark-chips .mark-chip").count()).toBe(1);
    const before = first?.rect;
    if (!before) throw new Error("mark has no rect");

    // Move the element 60px right: the static watcher reloads the frame and the bridge reports the new rect.
    writeFileSync(join(project.root, "index.html"), site(80));
    await expect
      .poll(async () => (await marks())[0]?.rect?.[0] ?? before[0], { timeout: 15_000 })
      .toBeCloseTo(before[0] + 60, 0);

    // Remove the element: its key disappears from the layout and the core hides the mark.
    writeFileSync(join(project.root, "index.html"), site(null));
    await expect.poll(async () => (await marks())[0]?.rect, { timeout: 15_000 }).toBeNull();
    expect(await viewer.locator("#mark-chips .mark-chip").count()).toBe(0);
    await viewer.close();
  });

  it("hides a done mark unless show done is on, and reopens it from the chip menu", async () => {
    const project = await linkSite("marks status");
    const pageId = await pageWithFrame(project.id);
    const viewer = await h.openViewer(pageId);
    await expect
      .poll(() => viewer.evaluate(() => window.__penwise?.state().layouts ?? 0), {
        timeout: 15_000,
      })
      .toBe(1);
    await drawOnHero(viewer);

    const chip = viewer.locator("#mark-chips .mark-chip");
    await expect.poll(() => chip.count(), { timeout: 10_000 }).toBe(1);
    expect(await chip.getAttribute("data-status")).toBe("new");

    // Done hides the mark while "show done" is off.
    await chip.click();
    await viewer.getByRole("menuitem", { name: "Done" }).click();
    await expect.poll(() => chip.count(), { timeout: 10_000 }).toBe(0);

    // "Show done" brings it back, greyed out as done...
    await viewer.check("#tool-show-done");
    await expect.poll(() => chip.count(), { timeout: 10_000 }).toBe(1);
    expect(await chip.getAttribute("data-status")).toBe("done");

    // ...and the menu reopens it, which shows it again even with "show done" off.
    await chip.click();
    await viewer.getByRole("menuitem", { name: "Reopen" }).click();
    await expect.poll(() => chip.getAttribute("data-status")).toBe("reopened");
    await viewer.uncheck("#tool-show-done");
    await expect.poll(() => chip.count()).toBe(1);
    await viewer.close();
  });

  it("remembers the page state, greys the mark when it is inactive and replays it", async () => {
    const project = await linkSite("marks state", MENU_SITE);
    const pageId = await pageWithFrame(project.id);
    const viewer = await h.openViewer(pageId);
    const states = () => viewer.evaluate(() => window.__penwise?.state().states ?? []);
    await expect
      .poll(() => viewer.evaluate(() => window.__penwise?.state().layouts ?? 0), {
        timeout: 15_000,
      })
      .toBe(1);

    // Open the menu (a recorded state step), then draw over its button.
    const frame = viewer.frameLocator("#frames iframe");
    const menu = frame.locator("#menu");
    await menu.click();
    await expect.poll(states, { timeout: 10_000 }).toEqual(["click:#menu"]);

    await viewer.check("#tool-draw-mouse");
    const box = await menu.boundingBox();
    if (!box) throw new Error("menu button is not on screen");
    const [cx, cy] = [box.x + box.width / 2, box.y + box.height / 2];
    await viewer.mouse.move(cx - 8, cy - 4);
    await viewer.mouse.down();
    await viewer.mouse.move(cx + 8, cy + 4, { steps: 8 });
    await viewer.mouse.up();
    await settle(viewer);

    const chip = viewer.locator("#mark-chips .mark-chip");
    await expect.poll(() => chip.count(), { timeout: 10_000 }).toBe(1);
    expect((await viewer.evaluate(() => window.__penwise?.state().marks ?? []))[0]?.state).toBe(
      "click:#menu",
    );
    await expect.poll(() => chip.getAttribute("class")).not.toContain("dimmed");

    // Closing the menu leaves the mark greyed: its state is not active any more.
    await viewer.uncheck("#tool-draw-mouse");
    await menu.click();
    await expect.poll(states, { timeout: 10_000 }).toEqual([""]);
    await expect.poll(() => chip.getAttribute("class")).toContain("dimmed");

    // The chip offers the replay, which opens the menu again and makes the mark active.
    await chip.click();
    await viewer.getByRole("menuitem", { name: "Replay state" }).click();
    await expect.poll(states, { timeout: 10_000 }).toEqual(["click:#menu"]);
    await expect.poll(() => chip.getAttribute("class")).not.toContain("dimmed");
    await viewer.close();
  });
});
