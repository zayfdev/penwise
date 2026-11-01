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

export async function landscape() {
  await adb.wake();
  await adb.setRotation(1);
  for (let i = 0; i < 20; i++) {
    const s = await adb.screen();
    if (s.w > s.h) return s;
    await adb.sleep(300);
  }
  return adb.screen();
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
  await landscape();
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
  const page = await waitForPage(route, timeoutMs);
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

/** Re-focuses an already shown frame (the owner's "fit this frame" move). */
export async function refocus(frame, route) {
  const client = await mcpClient();
  await client.call('show', { focus: { frame } });
  const page = await waitForPage(route, 30000);
  await adb.sleep(1200);
  return page;
}

export { livePage };
