// Frame skew between ink and page while the page moves (owner, 2026-11-02: ink and site "move in together in sync and
// never being swayed / sliding around"). The screen is recorded during finger drags and flings, and in every frame
// the row of a horizontal stroke is compared with the row of a section boundary of the page in the same column:
// both must move by the same amount from frame to frame. Needs ffmpeg on the PC to split the recording.
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as adb from '../adb.mjs';
import { decode } from '../png.mjs';
import { drawStroke, scrollToTop, settle } from '../gestures.mjs';
import { ROUTE } from './open.mjs';

const MAX_SKEW = 2;
/** A frame counts as moving when the page moved at least this far (screen px) since the frame before. */
const MOVING_PX = 1;
/** Page moves of at least this many px per recorded frame are fling speed. */
const FLING_PX = 10;
const DEVICE_FILE = '/sdcard/penwise-skew.mp4';

const lum = (r, g, b) => 0.299 * r + 0.587 * g + 0.114 * b;
const near = (v, want) => Math.abs(v - want) < 30;

/**
 * Centres of runs of rows (consecutive within 2 px). A run that touches the read band's edge ([top], [bottom]) is
 * cut off there, and its centre would be off: it is left out.
 */
function runs(rows, top = -Infinity, bottom = Infinity) {
  const out = [];
  for (const y of rows) {
    const run = out.at(-1);
    if (run && y - run.at(-1) <= 2) run.push(y);
    else out.push([y]);
  }
  return out.filter((run) => run[0] > top + EDGE_PX && run.at(-1) < bottom - EDGE_PX).map((run) => run.reduce((a, v) => a + v, 0) / run.length);
}

/** A run this close to the read band's edge may be cut off. */
const EDGE_PX = 3;

/**
 * One frame's column: the stroke's row (the only dark run) and the section boundaries (a white band between two
 * #99c border lines, the sticky-header fixture's 24 css px gap). Rows above `top` (the page's dark sticky header)
 * and below `bottom` (the toolbar) are not read.
 */
function readColumn(img, top, bottom) {
  const dark = [];
  const border = [];
  const white = [];
  for (let y = top; y < bottom; y++) {
    const o = (y * img.w + 1) * 4;
    const [r, g, b] = [img.data[o], img.data[o + 1], img.data[o + 2]];
    if (lum(r, g, b) < 70) dark.push(y);
    else if (near(r, 153) && near(g, 153) && near(b, 204)) border.push(y);
    else if (r > 245 && g > 245 && b > 245) white.push(y);
  }
  const gaps = runs(white).filter((c) => border.some((y) => y < c && c - y < 40) && border.some((y) => y > c && y - c < 40));
  const strokes = runs(dark, top, bottom);
  return { stroke: strokes.length === 1 ? strokes[0] : null, gaps };
}

/** Per frame: page shift (one tracked section boundary), ink shift and their difference, from the first frame. */
function skewRows(frames) {
  const base = frames.find((f) => f.stroke !== null && f.gaps.length);
  if (!base) return [];
  let shift = 0;
  const out = [];
  for (const f of frames) {
    const want = base.gaps[0] + shift;
    const gap = f.gaps.reduce((best, g) => (best === null || Math.abs(g - want) < Math.abs(best - want) ? g : best), null);
    if (gap === null || Math.abs(gap - want) > 500) {
      out.push({ t: f.t, page: null });
      continue;
    }
    const prev = shift;
    shift = gap - base.gaps[0];
    const ink = f.stroke === null ? null : f.stroke - base.stroke;
    out.push({ t: f.t, page: shift, v: shift - prev, ink, skew: ink === null ? null : ink - shift });
  }
  return out;
}

async function record(ctx, label, moves) {
  const dir = join(ctx.dir, `skew-${label.replace(/\W+/g, '-')}${ctx.report.suffix.replace(/\W+/g, '-')}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const rec = spawn(adb.ADB, ['shell', 'screenrecord', '--bit-rate', '60000000', '--time-limit', '7', DEVICE_FILE]);
  await adb.sleep(1200);
  for (const move of moves) {
    await move();
    await adb.sleep(1100);
  }
  await new Promise((resolve) => rec.on('exit', resolve));
  await adb.run(['pull', DEVICE_FILE, join(dir, 'screen.mp4')]);
  await adb.sh(`rm -f ${DEVICE_FILE}`).catch(() => {});
  return dir;
}

/** Splits the recording into one 3 px column per frame (with its time) and reads each. */
function frames(dir, column, top, bottom) {
  const info = execFileSync(
    'ffmpeg',
    ['-hide_banner', '-i', join(dir, 'screen.mp4'), '-vf', `crop=3:ih:${column - 1}:0,showinfo`, '-fps_mode', 'passthrough', join(dir, 'f%05d.png')],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
  );
  const times = [...String(info).matchAll(/pts_time:([\d.]+)/g)].map((m) => Number(m[1]));
  return readdirSync(dir)
    .filter((f) => /^f\d+\.png$/.test(f))
    .sort()
    .map((f, i) => ({ t: times[i] ?? i / 60, ...readColumn(decode(readFileSync(join(dir, f))), top, bottom) }));
}

const stats = (rows) => ({
  frames: rows.length,
  over: rows.filter((r) => Math.abs(r.skew) > MAX_SKEW).length,
  max: rows.reduce((m, r) => Math.max(m, Math.abs(r.skew)), 0),
  mean: +(rows.reduce((a, r) => a + Math.abs(r.skew), 0) / Math.max(1, rows.length)).toFixed(1),
});

/**
 * A horizontal stroke right of the other checks' strokes, then drags and flings both ways while the screen records.
 * Over frames the committed ink is drawn in the window, in the same frame as the page ([DryInkView]).
 */
export default async function glueSkew(ctx) {
  const g = 'glue';
  const name = (kind) => `frame skew while the page moves: ${kind}`;
  try {
    execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' });
  } catch {
    ctx.report.add(g, name('drag and fling'), 'error', { measured: 'ffmpeg not found on the PC', threshold: 'ffmpeg installed' });
    return;
  }
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const { w, live } = { w: ctx.screen.w, live: ctx.screen.live };
  const y = Math.round(live.h * 0.56);
  await drawStroke(ctx, { n: ctx.strokeNo, line: [w * 0.66, y, w * 0.76, y + 1] });
  const x = live.w * 0.85;
  const drag = (a, b, ms) => () => adb.finger(x, live.h * a, x, live.h * b, ms);
  const dir = await record(ctx, 'drag and fling', [drag(0.8, 0.45, 900), drag(0.45, 0.8, 900), drag(0.75, 0.6, 90), drag(0.55, 0.75, 90)]);
  const rows = skewRows(frames(dir, Math.round(w * 0.71), Math.round(95 * ctx.scale), live.h - 60));
  writeFileSync(join(dir, 'skew.json'), JSON.stringify(rows));
  for (const f of readdirSync(dir)) if (/^f\d+\.png$/.test(f)) rmSync(join(dir, f));
  const moving = rows.filter((r) => r.skew !== null && r.skew !== undefined && Math.abs(r.v) >= MOVING_PX);
  if (moving.length < 20) {
    ctx.report.add(g, name('drag and fling'), 'error', { measured: `${moving.length} moving frames with the stroke in view`, threshold: '>= 20' });
    return;
  }
  for (const [kind, list] of [
    ['drag', moving.filter((r) => Math.abs(r.v) < FLING_PX)],
    ['fling', moving.filter((r) => Math.abs(r.v) >= FLING_PX)],
  ]) {
    const s = stats(list);
    ctx.report.check(g, name(kind), list.length > 0 && s.max <= MAX_SKEW, {
      measured: `${s.over} of ${s.frames} recorded frames off by more than ${MAX_SKEW} px; worst ${s.max} px, mean ${s.mean} px`,
      threshold: `|ink shift - page shift| <= ${MAX_SKEW} px in every moving frame`,
      evidence: `per-frame rows in ${dir}/skew.json`,
    });
  }
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
}
