// Edits a served file of the QA copy and watches the live page for the reload that should follow.
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import * as adb from './adb.mjs';
import { evalLive } from './cdp.mjs';
import { SITES } from './server.mjs';

const FILE = join(SITES, 'sticky-header/index.html');

/** Marks the page (window.__qaMarker), edits the heading "One", waits for a page that lost the marker and shows it. */
export async function probeLiveReload(route, timeoutMs = 30000) {
  const original = await readFile(FILE, 'utf8');
  const tag = `edited ${Date.now() % 100000}`;
  await evalLive('window.__qaMarker = 1; 1', route);
  const t0 = Date.now();
  await writeFile(FILE, original.replace('<h2>One</h2>', `<h2>One (${tag})</h2>`));
  let reloaded = false;
  try {
    while (Date.now() - t0 < timeoutMs && !reloaded) {
      await adb.sleep(500);
      const state = await evalLive('JSON.stringify({m: window.__qaMarker ?? null, t: document.getElementById("s1")?.innerText ?? ""})', route).catch(() => null);
      if (state) {
        const s = JSON.parse(state);
        reloaded = s.m === null && s.t.includes(tag);
      }
    }
  } finally {
    await writeFile(FILE, original);
  }
  return { reloaded, ms: Date.now() - t0 };
}

/** Puts the file back and gives the server's watcher time to notice, so the next probe starts from a quiet page. */
export async function settleAfterProbe(route) {
  await adb.sleep(3500);
  await evalLive('1', route).catch(() => null);
}
