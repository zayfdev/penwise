// Scripted gestures and their measurements: strokes (renderer + server + pixels), scrolling, marks, screenshots.
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import * as adb from './adb.mjs';
import { evalLive } from './cdp.mjs';
import { bbox, darkPixels, decode, diffPixels } from './png.mjs';
import { bufferStalls, inkFrames, logbook, waitInkFramesAbove } from './probes.mjs';
import { appPids, mcpClient } from './session.mjs';

export async function shot(ctx, name) {
  const png = await adb.screencap();
  await writeFile(join(ctx.dir, 'shots', `${name}${ctx.report.suffix.replace(/\W+/g, '-')}.png`), png);
  return decode(png);
}

/** {round, status, marks, open} read from `look` (no images). */
export async function roundState() {
  const client = await mcpClient();
  const { text } = await client.call('look', { images: 'none' });
  const head = /round (\d+) (\w+) \| (\d+) mark\(s\), (\d+) open/.exec(text) ?? [];
  return { round: Number(head[1]), status: head[2], marks: Number(head[3]), open: Number(head[4]), text };
}

/** The board box ("1440x900") the server reports for a frame in `look`'s frames list; null when unlisted. */
export async function frameBox(frameId) {
  const { text } = await roundState();
  const m = new RegExp(`${frameId}[^\\n]*\\(box (\\d+)x(\\d+)\\)`).exec(text);
  return m ? { w: Number(m[1]), h: Number(m[2]) } : null;
}

export async function waitMarks(target, timeoutMs = 12000) {
  const start = Date.now();
  let st = await roundState();
  while (st.marks < target && Date.now() - start < timeoutMs) {
    await adb.sleep(500);
    st = await roundState();
  }
  return { ...st, ms: Date.now() - start };
}

/** Waits until scrollY stops changing (a fling ended); returns the final scrollY (null when no live page). */
export async function settle(match, maxMs = 8000) {
  const end = Date.now() + maxMs;
  let last = null;
  let same = 0;
  while (Date.now() < end) {
    const y = await evalLive('scrollY', match).catch(() => null);
    if (y !== null && y === last) same++;
    else same = 0;
    last = y;
    if (same >= 3) break;
    await adb.sleep(150);
  }
  return last;
}

/**
 * Back to the top with finger swipes (what the owner does). A page-side scrollTo would leave the app's own scroll
 * model out of step (see the scroll group's "page JS scroll" check), so the harness never uses it to navigate.
 */
export async function scrollToTop(ctx, match) {
  let y = await evalLive('scrollY', match).catch(() => null);
  for (let i = 0; i < 16 && y !== null && y > 1; i++) {
    await swipePage(ctx.screen, { down: false, frac: 0.6 });
    y = await settle(match);
  }
  return y;
}

/** One finger swipe up the page (content moves up, scrolling down) or down. `slow` limits the fling. */
export async function swipePage(screen, { down = true, slow = false, frac = 0.4 } = {}) {
  // Inside the live window (the whole screen in landscape, the top band in portrait), right of the strokes the checks
  // draw (x 0.3-0.65) and clear of the toolbar and Send.
  const live = screen.live ?? { w: screen.w, h: screen.h };
  const x = live.w * 0.85;
  const [a, b] = down ? [live.h * 0.8, live.h * (0.8 - frac)] : [live.h * 0.25, live.h * (0.25 + frac)];
  await adb.finger(x, a, x, b, slow ? 1400 : 350);
}

/**
 * A finger drag on the part of the screen below the live window (portrait only): the owner expects "drag anywhere on
 * the locked frame scrolls the page". Returns {scrolled, from, to, before, after} or null when there is no still part.
 */
export async function dragOnStill(ctx, route, { frac = 0.15 } = {}) {
  const s = ctx.screen;
  if (s.live.h >= s.h - 400) return null;
  const x = s.w * 0.7;
  const y0 = s.live.h + (s.h - s.live.h) * 0.7;
  const before = await evalLive('scrollY', route).catch(() => null);
  await adb.finger(x, y0, x, y0 - (s.h - s.live.h) * 0.5, 400);
  await settle(route);
  const after = await evalLive('scrollY', route).catch(() => null);
  return { scrolled: after - before, before, after, from: [x, y0] };
}

/**
 * Draws one pen stroke inside the page area and measures: ink renderer frames, BufferPool stalls, the server's
 * mark count, and the pixels that changed. `n` varies the position so strokes do not stack.
 */
let out_t0 = 0;
export async function drawStroke(ctx, { n = 0, wait = true, server = true, line = null } = {}) {
  const s = ctx.screen;
  // `line` [x1, y1, x2, y2] (screen px) draws that segment instead of the n-th default one. Such strokes do not take
  // a default slot, so the default strokes keep cycling through the five places (a portrait glue stroke must not land
  // on a landscape one: same slot, same document point).
  if (line) ctx.lineStrokes = (ctx.lineStrokes ?? 0) + 1;
  const slot = n - (line ? 0 : ctx.lineStrokes ?? 0);
  const y0 = s.live.h * (0.42 + 0.06 * (((slot % 5) + 5) % 5));
  const [x1, y1, x2, y2] = line ?? [s.w * 0.3, y0, s.w * 0.6, y0 + s.live.h * 0.08];
  const marksBefore = server ? (await roundState()).marks : (ctx.offlineMarks ?? 0);
  const before = await shot(ctx, `stroke-${ctx.strokeNo}-before`);
  const framesBefore = await inkFrames();
  await logbook.drain();
  out_t0 = Date.now();
  await adb.pen(x1, y1, x2, y2, 600);
  const out = { marksBefore, framesBefore, from: [x1, y1], to: [x2, y2], before };
  if (!wait) return out;
  out.framesAfter = await waitInkFramesAbove(framesBefore ?? 0, 4000);
  await adb.sleep(900);
  out.after = await shot(ctx, `stroke-${ctx.strokeNo}-after`);
  ctx.strokeNo++;
  const rect = [Math.min(x1, x2) - 40, Math.min(y1, y2) - 40, Math.max(x1, x2) + 40, Math.max(y1, y2) + 40].map(Math.round);
  out.changed = diffPixels(before, out.after, rect);
  out.box = bbox(out.changed);
  out.dark = darkPixels(out.after, rect, 50).filter(([x, y]) => out.changed.some(([cx, cy]) => cx === x && cy === y));
  if (server) {
    const marks = await waitMarks(marksBefore + 1);
    out.marks = marks.marks;
    out.markMs = marks.ms;
  }
  out.stalls = bufferStalls(await logbook.drain(), appPids());
  out.penStart = out_t0;
  return out;
}

/** A BufferPool "Waiting for buffer" line is a hiccup; a renderer that stays blocked is the failure. */
export const MAX_WAITS = 3;

/**
 * "Renderer healthy" after an action that did not itself draw (a scroll): at most MAX_WAITS BufferPool waits since
 * the action AND the ink layer still presents frames, shown by a short pen probe whose frame count must advance.
 * The probe leaves one extra mark on the QA board.
 */
export async function rendererHealthy(ctx, group, label, stalls) {
  const s = ctx.screen;
  const before = await inkFrames();
  await adb.pen(s.w * 0.7, s.live.h * 0.2, s.w * 0.75, s.live.h * 0.22, 300);
  const after = await waitInkFramesAbove(before ?? 0, 4000);
  const ok = stalls.length <= MAX_WAITS && (after ?? 0) > (before ?? 0);
  return ctx.report.check(group, `${label}: renderer healthy`, ok, {
    measured: `${stalls.length} BufferPool wait line(s); ink frames ${before} -> ${after} after a pen probe`,
    threshold: `<= ${MAX_WAITS} waits and frames advance`,
    evidence: stalls.slice(0, 2).join('\n'),
  });
}

/** Adds the standard "a stroke renders and reaches the server" verdicts for a drawStroke result. */
export function strokeVerdicts(report, group, label, r) {
  const rise = (r.framesAfter ?? 0) - (r.framesBefore ?? 0);
  report.check(group, `${label}: ink frames rise`, rise >= 1, { measured: `${r.framesBefore} -> ${r.framesAfter}`, threshold: '>= +1' });
  report.check(group, `${label}: renderer healthy`, r.stalls.length <= MAX_WAITS && rise >= 1, {
    measured: `${r.stalls.length} BufferPool wait line(s), ink frames ${r.framesBefore} -> ${r.framesAfter}`,
    threshold: `<= ${MAX_WAITS} waits and frames advance`,
    evidence: [`pen down at ${new Date(r.penStart).toTimeString().slice(0, 8)}.${String(r.penStart % 1000).padStart(3, '0')} (device clock)`, ...r.stalls.slice(0, 2)].join('\n'),
  });
  report.check(group, `${label}: pixels changed`, r.changed.length >= 300, { measured: `${r.changed.length} px`, threshold: '>= 300 px' });
  report.check(group, `${label}: server gained a mark`, r.marks === r.marksBefore + 1, {
    measured: `${r.marksBefore} -> ${r.marks} in ${r.markMs} ms`,
    threshold: '+1 within 12 s',
  });
}
