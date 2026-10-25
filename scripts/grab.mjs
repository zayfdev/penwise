#!/usr/bin/env bun
/**
 * grab: cut components out of a running page and lay them on one Penwise page.
 *
 * ```
 * bun scripts/grab.mjs --url http://localhost:3000 --selectors ".card,.hero,#pricing" --title "Pricing ideas"
 * ```
 *
 * Drives Chrome over the DevTools protocol (`Page.captureScreenshot` with a clip per element), so no browser
 * automation package is needed — only a Chrome/Chromium binary. Each cut-out is uploaded as an asset and placed
 * in a grid with its selector as the label, ready to be rearranged and drawn on.
 */

import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createPage, placePicture, selectPage, uploadPng } from "../extension/server.js";

const GAP = 48;
const PADDING = 48;
const MAX_TILE_WIDTH = 640;
const MAX_TILE_HEIGHT = 480;

const CHROME_CANDIDATES = [
  process.env.CHROME,
  "/usr/bin/google-chrome-stable",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "/usr/bin/brave",
  "/snap/bin/chromium",
].filter(Boolean);

function parseArgs(argv) {
  const args = { url: "", selectors: [], title: "Components", server: "http://127.0.0.1:7420", token: "", width: 1440, height: 900, all: false };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i];
    const value = argv[i + 1];
    switch (name) {
      case "--url": args.url = value; i++; break;
      case "--selectors": args.selectors.push(...value.split(",").map((s) => s.trim()).filter(Boolean)); i++; break;
      case "--title": args.title = value; i++; break;
      case "--server": args.server = value; i++; break;
      case "--token": args.token = value; i++; break;
      case "--all": args.all = true; break;
      case "--width": args.width = Number(value); i++; break;
      case "--height": args.height = Number(value); i++; break;
      case "--help":
      case "-h": usage(); process.exit(0); break;
      default: throw new Error(`unknown argument ${name}`);
    }
  }
  if (!args.url) throw new Error("--url is required");
  if (!args.selectors.length) throw new Error("--selectors is required");
  return args;
}

function usage() {
  console.log(`grab: components from a page onto a Penwise page

  --url <page>            page to cut from (required)
  --selectors <list>      comma-separated CSS selectors (required)
  --title <text>          Penwise page title (default "Components")
  --server <url>          Penwise server (default http://127.0.0.1:7420)
  --token <token>         access token (default: ~/.local/share/penwise/token)
  --all                   cut every match of each selector, not just the first
  --width/--height <px>   viewport for the render (default 1440x900)

Set CHROME to the browser binary when it is not in a standard location.`);
}

function findChrome() {
  const found = CHROME_CANDIDATES.find((path) => existsSync(path));
  if (!found) throw new Error("no Chrome/Chromium found; set CHROME=/path/to/chrome");
  return found;
}

/** A DevTools session: `send` resolves the reply, events arrive through `on`. */
function cdp(url) {
  const socket = new WebSocket(url);
  const pending = new Map();
  const listeners = new Map();
  let nextId = 1;
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(message.error.message));
      else resolve(message.result);
      return;
    }
    for (const listener of listeners.get(message.method) ?? []) listener(message.params);
  });
  return new Promise((resolve) => {
    socket.addEventListener("open", () =>
      resolve({
        send(method, params = {}) {
          const id = nextId++;
          socket.send(JSON.stringify({ id, method, params }));
          return new Promise((done, fail) => pending.set(id, { resolve: done, reject: fail }));
        },
        on(method, listener) {
          if (!listeners.has(method)) listeners.set(method, []);
          listeners.get(method).push(listener);
        },
        close() {
          socket.close();
        },
      }),
    );
  });
}

/** The first page target's debugger socket, once Chrome has opened the URL. */
async function pageSocket(profile) {
  const path = join(profile, "DevToolsActivePort");
  for (let attempt = 0; attempt < 150; attempt++) {
    try {
      const [port] = (await readFile(path, "utf8")).trim().split("\n");
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // Chrome is still starting.
    }
    await Bun.sleep(100);
  }
  throw new Error("Chrome did not expose a page");
}

/** Runs an expression in the page and returns its value. */
async function evaluate(session, expression) {
  const result = await session.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? "page evaluation failed");
  }
  return result.result?.value;
}

/** The boxes of the requested selectors, in page CSS px. */
async function boxes(session, selectors, all) {
  return evaluate(
    session,
    `(() => {
      const found = [];
      for (const selector of ${JSON.stringify(selectors)}) {
        const elements = [...document.querySelectorAll(selector)];
        if (!elements.length) { found.push({ selector, missing: true }); continue; }
        const wanted = ${all ? "elements" : "elements.slice(0, 1)"};
        for (const [index, element] of wanted.entries()) {
          const box = element.getBoundingClientRect();
          found.push({
            selector,
            label: wanted.length > 1 ? selector + "[" + index + "]" : selector,
            x: box.left + window.scrollX,
            y: box.top + window.scrollY,
            width: box.width,
            height: box.height,
            text: (element.textContent ?? "").trim().slice(0, 120),
          });
        }
      }
      return found;
    })()`,
  );
}

/** Screenshots one clipped region as PNG bytes. */
async function screenshot(session, clip) {
  const { data } = await session.send("Page.captureScreenshot", {
    format: "png",
    clip: { ...clip, scale: 1 },
    captureBeyondViewport: true,
  });
  return Buffer.from(data, "base64");
}

/** Lays the tiles out in a grid and returns the page size plus each tile's rectangle. */
function layout(tiles) {
  const columns = tiles.length <= 4 ? Math.max(1, tiles.length) : 3;
  const cells = tiles.map((tile) => {
    const scale = Math.min(1, MAX_TILE_WIDTH / tile.width, MAX_TILE_HEIGHT / tile.height);
    return { width: Math.round(tile.width * scale), height: Math.round(tile.height * scale) };
  });
  const rows = [];
  for (let index = 0; index < cells.length; index += columns) rows.push(cells.slice(index, index + columns));
  const widths = rows.map((row) => row.reduce((sum, cell) => sum + cell.width, 0) + GAP * (row.length - 1));
  const width = Math.max(...widths) + PADDING * 2;
  let y = PADDING;
  const placed = [];
  for (const row of rows) {
    const height = Math.max(...row.map((cell) => cell.height));
    let x = PADDING;
    for (const cell of row) {
      placed.push({ ...cell, x, y });
      x += cell.width + GAP;
    }
    y += height + GAP;
  }
  return { width: Math.ceil(width), height: Math.ceil(y - GAP + PADDING), placed };
}

async function resolveToken(args) {
  if (args.token) return args.token;
  const home = process.env.HOME ?? "";
  try {
    return (await readFile(join(home, ".local/share/penwise/token"), "utf8")).trim();
  } catch {
    return "";
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const settings = { url: args.server, token: await resolveToken(args) };
  const profile = await mkdtemp(join(tmpdir(), "penwise-grab-"));
  const chrome = spawn(
    findChrome(),
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      `--window-size=${args.width},${args.height}`,
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      args.url,
    ],
    { stdio: "ignore", detached: false },
  );

  let session;
  try {
    session = await cdp(await pageSocket(profile));
    await session.send("Page.enable");
    await session.send("Emulation.setDeviceMetricsOverride", {
      width: args.width,
      height: args.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    const loaded = new Promise((resolve) => session.on("Page.loadEventFired", resolve));
    await session.send("Page.navigate", { url: args.url });
    await Promise.race([loaded, Bun.sleep(15_000)]);

    const found = await boxes(session, args.selectors, args.all);
    const tiles = [];
    for (const box of found) {
      if (box.missing) {
        console.warn(`skip ${box.selector}: not on the page`);
        continue;
      }
      if (box.width < 1 || box.height < 1) {
        console.warn(`skip ${box.selector}: zero size`);
        continue;
      }
      const png = await screenshot(session, { x: box.x, y: box.y, width: box.width, height: box.height });
      tiles.push({ ...box, png });
      console.log(`cut ${box.selector} (${Math.round(box.width)}x${Math.round(box.height)})`);
    }
    if (!tiles.length) throw new Error("nothing to place");

    const grid = layout(tiles);
    const page = await createPage(settings, { title: args.title, width: grid.width, height: grid.height });
    for (const [index, tile] of tiles.entries()) {
      const stored = await uploadPng(settings, tile.png);
      const cell = grid.placed[index];
      await placePicture(settings, page.id, {
        asset: stored.asset,
        x: cell.x,
        y: cell.y,
        w: cell.width,
        h: cell.height,
        label: tile.label ?? tile.selector,
      });
      console.log(`placed ${tile.selector} at ${cell.x},${cell.y}`);
    }
    await selectPage(settings, page.id);
    console.log(`page ${page.id}: ${tiles.length} cut-out(s), ${grid.width}x${grid.height}`);
  } finally {
    session?.close();
    chrome.kill("SIGTERM");
    await rm(profile, { recursive: true, force: true });
  }
}


main().catch((error) => {
  console.error(String(error?.message ?? error));
  process.exit(1);
});
