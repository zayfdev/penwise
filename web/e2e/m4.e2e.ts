/**
 * Browser end to end: ghosts and variants in the PC window (design-harness §7.1, M4).
 *
 * A ghost set over HTTP (`POST /frames/{id}/ghost`) appears inside the frame's iframe; the chip's Ghost
 * toggle reverts and re-applies it. Variant frames (`POST /frames/{id}/variants`) show their labels; "Use
 * this" makes one the group's pick (highlighted, siblings dimmed) and the server's `GET /variants/{group}`
 * reports the same choice.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Locator, Page } from "playwright-core";
import { afterAll, describe, expect, it } from "vitest";
import { focusFrame, useHarness } from "./harness";

const h = useHarness();

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

const SITE = `<!doctype html><html><head><meta charset="utf-8"><title>m4 site</title></head>
<body style="margin:0"><h1 id="marker">original</h1><div style="height:1200px"></div></body></html>`;

/** Crockford base32 digits of a ULID. */
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** The 32-hex id the server routes take, from the 26-char ULID `frames()` / the create response use. */
function hexId(ulid: string): string {
  let bits = "";
  for (const char of ulid) {
    const value = CROCKFORD.indexOf(char.toUpperCase());
    if (value < 0) throw new Error(`not a ULID: ${ulid}`);
    bits += value.toString(2).padStart(5, "0");
  }
  return (bits.slice(-128).match(/.{4}/g) ?? [])
    .map((nibble) => Number.parseInt(nibble, 2).toString(16))
    .join("");
}

/** Links a throwaway static folder as a project; returns its id. */
async function linkSite(name: string): Promise<string> {
  const root = mkdtempSync(join(tmpdir(), "penwise-m4-"));
  roots.push(root);
  writeFileSync(join(root, "index.html"), SITE);
  const project = await h.api("POST", "/projects", { name, root, static: root });
  return String(project.id);
}

/** A page with one phone frame of the project; returns the page id and the frame's hex id. */
async function pageWithFrame(project: string): Promise<{ page: string; frame: string }> {
  const page = await h.createInfinitePage("m4");
  const created = await h.api("POST", `/pages/${page}/frames`, {
    project,
    route: "/",
    preset: "phone",
  });
  return { page, frame: hexId(String(created.id)) };
}

/** One frame of `GET /variants/{group}`. */
interface VariantFrame {
  readonly id: string;
  readonly label: string;
  readonly chosen: boolean;
}

/** Validates the variant list of `GET /variants/{group}` (server JSON, so the shape is checked). */
function readVariantFrames(value: unknown): VariantFrame[] {
  if (!Array.isArray(value)) return [];
  const frames: VariantFrame[] = [];
  for (const row of value) {
    if (typeof row !== "object" || row === null) continue;
    const entry = row as Record<string, unknown>;
    if (
      typeof entry.id === "string" &&
      typeof entry.label === "string" &&
      typeof entry.chosen === "boolean"
    ) {
      frames.push({ id: entry.id, label: entry.label, chosen: entry.chosen });
    }
  }
  return frames;
}

/** The index among the page's frame boxes of the frame `id` (the order `focusFrame` takes). */
async function frameIndex(viewer: Page, id: string): Promise<number> {
  const ids = await viewer
    .locator("#frames .frame")
    .evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.frame ?? ""));
  const index = ids.indexOf(id);
  if (index < 0) throw new Error(`no frame box for ${id}`);
  return index;
}

/** The text of the framed page's marker (waits through the patch apply/revert round trip). */
const markerText = (viewer: Page) =>
  viewer.frameLocator("#frames iframe").locator("#marker").textContent();

describe("ghosts and variants in the PC window", () => {
  it("shows a ghost patch in the iframe and the chip toggle reverts it", async () => {
    const site = await linkSite("m4 ghost");
    const { page, frame } = await pageWithFrame(site);
    const viewer = await h.openViewer(page);
    await focusFrame(viewer);
    await expect.poll(() => markerText(viewer), { timeout: 10_000 }).toBe("original");

    // The agent proposes a ghost over HTTP; it syncs to the window as a frame field.
    const reply = await h.api("POST", `/frames/${frame}/ghost`, {
      patch: { css: "", dom: [{ op: "set_text", selector: "#marker", value: "patched" }] },
    });
    expect(reply).toMatchObject({ ok: true });
    await expect.poll(() => markerText(viewer), { timeout: 10_000 }).toBe("patched");

    // Only patched frames carry the (local) ghost toggle, and it starts on.
    const ghost = viewer.locator("#frame-chips .frame-chip .ghost");
    expect(await ghost.isVisible()).toBe(true);
    const ghostBox = ghost.locator("input");
    expect(await ghostBox.isChecked()).toBe(true);

    // Toggling it off reverts the page exactly, and on applies it again.
    await ghostBox.uncheck();
    await expect.poll(() => markerText(viewer), { timeout: 10_000 }).toBe("original");
    await ghostBox.check();
    await expect.poll(() => markerText(viewer), { timeout: 10_000 }).toBe("patched");

    // Clearing the ghost in the core removes both the patch and the toggle.
    await h.api("POST", `/frames/${frame}/ghost`, { patch: null });
    await expect.poll(() => markerText(viewer), { timeout: 10_000 }).toBe("original");
    await expect.poll(() => ghost.isVisible()).toBe(false);
    await viewer.close();
  });

  it("shows variant labels and the pick from 'Use this' on the frame chips", async () => {
    const site = await linkSite("m4 variants");
    const { page, frame } = await pageWithFrame(site);
    const viewer = await h.openViewer(page);
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(1);

    const made = await h.api("POST", `/frames/${frame}/variants`, {
      variants: [
        { label: "Bold", patch: { css: "#marker{color:rgb(200,0,0)}", dom: [] } },
        { label: "Calm", patch: { css: "#marker{color:rgb(0,0,200)}", dom: [] } },
      ],
    });
    expect(typeof made.group).toBe("string");
    const group = String(made.group);

    await viewer.click("#tool-board");
    // The base plus two variants, each label on its chip and a "Use this" on the variants only. Nothing is
    // focused, so they are stills: no iframe at all.
    await expect.poll(() => viewer.locator("#frames .frame").count(), { timeout: 10_000 }).toBe(3);
    expect(await viewer.locator("#frames iframe").count()).toBe(0);
    const bold = viewer.locator("#frame-chips .frame-chip").filter({ hasText: "Bold" });
    const calm = viewer.locator("#frame-chips .frame-chip").filter({ hasText: "Calm" });
    await expect.poll(() => bold.count()).toBe(1);
    await expect.poll(() => calm.count()).toBe(1);
    await expect.poll(() => viewer.locator("#frame-chips button.use:visible").count()).toBe(2);
    // Variants carry a patch, so each has a ghost toggle; the plain base does not.
    await expect.poll(() => viewer.locator("#frame-chips .ghost:visible").count()).toBe(2);

    // Each variant shows its own patch: focus it and read the colour the patch gave the marker.
    const colourOf = async (chip: Locator): Promise<string> => {
      const id = await chip.getAttribute("data-frame");
      await viewer.click("#tool-board");
      await focusFrame(viewer, await frameIndex(viewer, String(id)));
      return viewer
        .frameLocator('#frames .frame[data-live="live"] iframe')
        .locator("#marker")
        .evaluate((node) => getComputedStyle(node).color);
    };
    await expect.poll(() => colourOf(bold), { timeout: 15_000 }).toBe("rgb(200, 0, 0)");
    await expect.poll(() => colourOf(calm), { timeout: 15_000 }).toBe("rgb(0, 0, 200)");
    await viewer.click("#tool-board");

    // Picking one highlights it and dims the other variant.
    // (At the fit view three chips overlap and the lifted ones can sit under the toolbar: dispatch the click.)
    await bold.locator("button.use").dispatchEvent("click");
    await expect.poll(() => bold.locator("button.use").textContent()).toBe("Chosen");
    await expect.poll(() => viewer.locator("#frames .frame.chosen").count()).toBe(1);
    await expect.poll(() => viewer.locator("#frames .frame.dimmed").count()).toBe(1);

    const state = await h.api("GET", `/variants/${group}`);
    const frames = readVariantFrames(state.frames);
    const boldPick = frames.find((entry) => entry.label === "Bold");
    expect(boldPick?.chosen).toBe(true);
    expect(frames.find((entry) => entry.label === "Calm")?.chosen).toBe(false);
    expect(state.chosen).toBe(boldPick?.id);
    await viewer.close();
  });
});
