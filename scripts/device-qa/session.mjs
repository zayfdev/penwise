// One device-QA session: saved device state (restored on every exit path), landscape, opening a frame on the tablet.
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import * as adb from './adb.mjs';
import { livePage, waitForPage } from './cdp.mjs';
import { Mcp } from './mcp.mjs';
import { PORT, PROJECT_ID, QA_DIR } from './server.mjs';

const STATE_FILE = join(QA_DIR, 'saved-device-state.json');
let saved = null;
let mcp = null;
const pids = new Set();

/** Remembers the app pids seen this run, for log filtering. */
export async function notePid() {
  const p = await adb.pid();
  if (p) pids.add(p);
  return p;
}
export const appPids = () => [...pids];

export async function mcpClient() {
  mcp ??= await new Mcp({ port: PORT, project: PROJECT_ID }).init();
  return mcp;
}

/** Saves rotation + Wi-Fi first (a state file survives a killed run: the next run restores it before saving anew). */
export async function saveState() {
  await mkdir(QA_DIR, { recursive: true });
  try {
    const stale = JSON.parse(await readFile(STATE_FILE, 'utf8'));
    await adb.restoreDeviceState(stale);
  } catch {}
  saved = await adb.snapshotDeviceState();
  await writeFile(STATE_FILE, JSON.stringify(saved));
}

export async function restoreState() {
  try {
    mcp?.close();
  } catch {}
  mcp = null;
  if (!saved) return;
  await adb.restoreDeviceState(saved);
  await rm(STATE_FILE, { force: true });
  saved = null;
}

let targetRot = null;

/** Locks the display to rotation `rot` (0 portrait, 1 landscape); null leaves the owner's current rotation, locked. */
export async function holdOrientation(rot = targetRot) {
  await adb.wake();
  if (rot === null) rot = (await adb.screen()).rot;
  targetRot = rot;
  await adb.setRotation(rot);
  for (let i = 0; i < 20; i++) {
    const s = await adb.screen();
    if (s.rot === rot) return s;
    await adb.sleep(300);
  }
  return adb.screen();
}
export const orientationName = (rot) => (rot % 2 === 1 ? 'landscape' : 'portrait');

/**
 * The screen in the current rotation plus what the app does with it: `pixels` is the WebView size PagePixels.kt gives
 * the 1440x900 preset (never taller than the screen's short side, page zoomed to match), `live` is the part of the
 * screen the live WebView covers (the view drawn scaled to the screen width, from the top).
 */
export async function measureScreen(cssWidth = 1440, cssHeight = 900) {
  const s = await adb.screen();
  const density = (await adb.density()) / 160;
  const shortSide = Math.min(s.w, s.h);
  const fit = Math.min(1, shortSide / (cssHeight * density));
  const width = Math.floor(cssWidth * density * fit + 1e-6);
  const height = Math.floor((cssHeight * width) / cssWidth + 1e-6);
  const liveH = Math.min(s.h, Math.round((s.w * height) / width));
  return { ...s, density, pixels: { width, height }, live: { w: s.w, h: liveH } };
}

/** If the app shows the library (notebook list), taps the QA board's card. */
async function leaveLibrary() {
  const nodes = await adb.uiNodes().catch(() => []);
  const card = nodes.find((n) => n.pkg === adb.PKG && n.text === 'device-qa board');
  const inCanvas = nodes.some((n) => n.pkg === adb.PKG && (n.desc === 'Open tools' || n.desc === 'Close tools'));
  if (card && !inCanvas) {
    await adb.tap(...adb.center(card));
    await adb.sleep(1500);
  }
}

/** Starts the app when it is not running, goes to the QA board. */
export async function openApp() {
  await holdOrientation();
  if (!(await adb.pid())) await adb.startApp();
  else await adb.sh(`am start -n ${adb.PKG}/.MainActivity`);
  for (let i = 0; i < 30 && !(await adb.pid()); i++) await adb.sleep(300);
  await notePid();
  await adb.sleep(2500);
  await leaveLibrary();
}

/** Shows a route as a desktop frame (reused when it exists), focuses it, waits for its live page. */
export async function openFrame(route, { timeoutMs = 90000 } = {}) {
  await openApp();
  const client = await mcpClient();
  const shown = await client.call('show', { frame: { route } });
  if (shown.isError) throw new Error(`show frame ${route}: ${shown.text}`);
  const focus = await client.call('show', { focus: { frame: shown.json.frame } });
  if (focus.isError) throw new Error(`focus ${route}: ${focus.text}`);
  const page = await waitLive(route, timeoutMs);
  await waitSynced(20000);
  await adb.sleep(1200);
  return { frame: shown.json.frame, route, page };
}

/** Waits until the toolbar status dot says Synced; returns the ms it took, null on timeout. */
export async function waitSynced(timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const nodes = await adb.uiNodes().catch(() => []);
    if (nodes.some((n) => n.pkg === adb.PKG && n.desc === 'Synced')) return Date.now() - start;
    await adb.sleep(1000);
  }
  return null;
}

/**
 * Waits for the live page of `route`. Within 10 s of the owner's last pen stroke the app does not move the camera for
 * an agent focus: it holds the move as a chip "Agent: open <route> →" (frames/AgentMoves.kt, QUIET_MS), and a tap
 * on the chip does it. The harness draws constantly, so it taps the chip when it sees one.
 */
export async function waitLive(route, timeoutMs) {
  const end = Date.now() + timeoutMs;
  let lastTap = 0;
  while (Date.now() < end) {
    const page = await waitForPage(route, 2500);
    if (page) return page;
    if (Date.now() - lastTap > 3000) {
      const chip = await findNode((n) => /→/.test(n.text) && /open/i.test(n.text));
      if (chip) {
        await adb.tap(...adb.center(chip));
        lastTap = Date.now();
      }
    }
  }
  return null;
}

/** Re-focuses an already shown frame (the owner's "fit this frame" move). */
export async function refocus(frame, route) {
  const client = await mcpClient();
  await client.call('show', { focus: { frame } });
  const page = await waitLive(route, 30000);
  await adb.sleep(1200);
  return page;
}

export { livePage };

/** Finds the first node of the app for which `pred` holds, in the dump of all windows. */
export async function findNode(pred) {
  const nodes = await adb.uiNodes().catch(() => []);
  return nodes.find((n) => n.pkg === adb.PKG && pred(n)) ?? null;
}
