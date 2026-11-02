// The hand-over of a committed stroke from the pen's front-buffered layer to the committed ink (over frames, the
// window's own view): no frame may show the stroke missing (a blink) or drawn twice. The screen is recorded while
// fast short strokes and long ones are drawn, and in every frame the ink mass in each stroke's box is compared with
// its final value once the stroke is complete. Needs ffmpeg on the PC.
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as adb from '../adb.mjs';
import { decode } from '../png.mjs';
import { scrollToTop, settle, swipePage } from '../gestures.mjs';
import { ROUTE } from './open.mjs';

/** Once complete, a stroke's ink mass stays within these shares of its final value (0.998 to 1.004 measured). */
const LOW = 0.97;
const HIGH = 1.03;
const DEVICE_FILE = '/sdcard/penwise-handoff.mp4';

const lum = (img, x, y) => {
  const o = (y * img.w + x) * 4;
  return 0.299 * img.data[o] + 0.587 * img.data[o + 1] + 0.114 * img.data[o + 2];
};

/** Ten fast short strokes and three long ones, in rows of the live window clear of the toolbar. */
function plan(screen) {
  const { w } = screen;
  const h = screen.live.h;
  const strokes = [];
  for (let i = 0; i < 10; i++) {
    strokes.push({ x: w * (0.1 + 0.16 * (i % 5)), y: h * (0.2 + 0.1 * Math.floor(i / 5)), dx: w * 0.09, dy: h * 0.025, ms: 90, kind: 'short' });
  }
  for (let i = 0; i < 3; i++) strokes.push({ x: w * 0.1, y: h * (0.45 + 0.1 * i), dx: w * 0.6, dy: h * 0.02, ms: 1200, kind: 'long' });
  return strokes.map((s) => ({ ...s, box: [s.x - 14, s.y - 14, s.x + s.dx + 30, s.y + s.dy + 14].map(Math.round) }));
}

/** Per stroke: the frame it is complete in and the lowest and highest ink mass after it, as shares of the final. */
function verdicts(series) {
  return series.map((mass) => {
    const tail = [...mass.slice(-8)].sort((a, b) => a - b);
    const final = tail[Math.floor(tail.length / 2)];
    const done = mass.findIndex((v) => v >= 0.97 * final);
    const after = done < 0 ? [] : mass.slice(done);
    return {
      done,
      frames: after.length,
      min: after.length ? Math.min(...after) / final : 0,
      max: after.length ? Math.max(...after) / final : 0,
    };
  });
}

export default async function inkHandoff(ctx) {
  const g = 'ink';
  const name = (kind) => `stroke hand-over: no blink, no double (${kind} strokes)`;
  try {
    execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' });
  } catch {
    ctx.report.add(g, name('all'), 'error', { measured: 'ffmpeg not found on the PC', threshold: 'ffmpeg installed' });
    return;
  }
  await scrollToTop(ctx, ROUTE);
  // A part of the page the other strokes of the run leave alone.
  await swipePage(ctx.screen, { down: true, frac: 0.6 });
  await settle(ROUTE);
  const dir = join(ctx.dir, `handoff${ctx.report.suffix.replace(/\W+/g, '-')}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const strokes = plan(ctx.screen);
  const seconds = Math.ceil(strokes.reduce((t, s) => t + s.ms + 800, 2500) / 1000);
  const rec = spawn(adb.ADB, ['shell', 'screenrecord', '--bit-rate', '60000000', '--time-limit', String(seconds), DEVICE_FILE]);
  await adb.sleep(1200);
  for (const s of strokes) {
    await adb.pen(s.x, s.y, s.x + s.dx, s.y + s.dy, s.ms);
    await adb.sleep(450);
  }
  await new Promise((resolve) => rec.on('exit', resolve));
  await adb.run(['pull', DEVICE_FILE, join(dir, 'screen.mp4')]);
  await adb.sh(`rm -f ${DEVICE_FILE}`).catch(() => {});
  execFileSync('ffmpeg', ['-hide_banner', '-loglevel', 'fatal', '-i', join(dir, 'screen.mp4'), '-fps_mode', 'passthrough', join(dir, 'f%05d.png')], {
    stdio: 'ignore',
  });
  const files = readdirSync(dir).filter((f) => /^f\d+\.png$/.test(f)).sort();
  const first = decode(readFileSync(join(dir, files[0])));
  const series = strokes.map(() => []);
  for (const f of files) {
    const img = decode(readFileSync(join(dir, f)));
    strokes.forEach((s, i) => {
      const [x0, y0, x1, y1] = s.box;
      let mass = 0;
      for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) mass += Math.max(0, lum(first, x, y) - lum(img, x, y));
      series[i].push(mass);
    });
    rmSync(join(dir, f));
  }
  const results = verdicts(series);
  writeFileSync(join(dir, 'handoff.json'), JSON.stringify({ strokes, series, results }));
  for (const kind of ['short', 'long']) {
    const rows = results.filter((_, i) => strokes[i].kind === kind);
    // The recording only has frames where the screen changed: a still stroke after its last change has nothing to judge.
    const judged = rows.filter((r) => r.frames >= 1);
    const low = Math.min(...judged.map((r) => r.min));
    const high = Math.max(...judged.map((r) => r.max));
    ctx.report.check(g, name(kind), judged.length === rows.length && low >= LOW && high <= HIGH, {
      measured: `${judged.length} of ${rows.length} strokes judged over ${files.length} frames; ink mass after completion ${low.toFixed(3)} to ${high.toFixed(3)} of final`,
      threshold: `every stroke judged, ${LOW} to ${HIGH}`,
      evidence: `per-frame mass in ${dir}/handoff.json, recording ${dir}/screen.mp4`,
    });
  }
}
