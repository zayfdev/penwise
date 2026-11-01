// Renderer and crash probes: SurfaceFlinger layer frame counts, BufferPool stalls, crash buffer, FATAL lines.
import { pid, run, sh } from './adb.mjs';

/** Frame counter of the app's MultiBufferedLayer (the low-latency ink renderer); null when there is none. */
export async function inkFrames() {
  const p = await pid();
  if (!p) return null;
  const dump = await sh('dumpsys SurfaceFlinger', { timeout: 30000 });
  const hex = Number(p).toString(16);
  const owned = new Set();
  for (const m of dump.matchAll(/MultiBufferedLayer#(\d+) pid=([0-9a-f]+)/g)) if (m[2] === hex) owned.add(m[1]);
  let best = null;
  for (const m of dump.matchAll(/Layer \[(\d+)\] MultiBufferedLayer#\d+\s*\n\s*visible reason=.*?frame=(\d+)/g)) {
    if (owned.has(m[1])) best = Math.max(best ?? 0, Number(m[2]));
  }
  return best;
}

export async function waitInkFramesAbove(base, timeoutMs = 4000) {
  const end = Date.now() + timeoutMs;
  let now = await inkFrames();
  while ((now ?? -1) <= base && Date.now() < end) {
    await new Promise((r) => setTimeout(r, 300));
    now = await inkFrames();
  }
  return now;
}

/** Accumulates the whole run's log: drain() appends what logcat holds, then clears it. */
export const logbook = {
  text: '',
  async drain() {
    const now = await logText();
    this.text += `${now}\n`;
    await run(['logcat', '-b', 'all', '-c']).catch(() => {});
    return now;
  },
};

export const clearLogs = async () => {
  await run(['logcat', '-b', 'all', '-c']).catch(() => {});
};

/** Logcat since the last clear, as text (threadtime format). */
export async function logText(buffers = 'main,system,crash') {
  return run(['logcat', '-b', buffers, '-d', '-v', 'threadtime'], { timeout: 60000 }).catch(() => '');
}

/** Lines of the log that belong to the given pid. */
export function forPid(text, p) {
  return text.split('\n').filter((l) => new RegExp(`^\\S+\\s+\\S+\\s+${p}\\s`).test(l));
}

/** Counts "Waiting for buffer" BufferPool stalls from any pid of the app (pids change on relaunch). */
export function bufferStalls(text, pids) {
  const set = new Set(pids);
  return text.split('\n').filter((l) => {
    if (!l.includes('BufferPool') || !/Waiting for buffer/i.test(l)) return false;
    const m = /^\S+\s+\S+\s+(\d+)\s/.exec(l);
    return m && set.has(m[1]);
  });
}

export async function crashBuffer() {
  const text = (await run(['logcat', '-b', 'crash', '-d'], { timeout: 30000 }).catch(() => '')).trim();
  return text.split('\n').filter((l) => l && !l.startsWith('--------'));
}

/** FATAL EXCEPTION / native-signal lines that belong to one of the app's pids, plus "Process: dev.penwise" lines. */
export function fatalLines(text, pids) {
  const set = new Set(pids);
  return text.split('\n').filter((l) => {
    if (/Process: dev\.penwise,/.test(l)) return true;
    if (!/FATAL EXCEPTION|Fatal signal/.test(l)) return false;
    const m = /^\S+\s+\S+\s+(\d+)\s/.exec(l);
    return m && set.has(m[1]);
  });
}

export async function totalPss() {
  const out = await sh('dumpsys meminfo dev.penwise', { timeout: 30000 });
  const m = /TOTAL PSS:\s+(\d+)/.exec(out) ?? /^\s*TOTAL\s+(\d+)/m.exec(out);
  return m ? Number(m[1]) : null;
}
