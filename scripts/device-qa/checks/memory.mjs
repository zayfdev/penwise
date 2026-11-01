// Check 10 (memory): TOTAL PSS before and after ~5 minutes of scripted scroll + draw on one frame.
//
// Threshold: growth above 128 MB between the median of the first three and the last three readings.
// Measured noise: with no trend at all, TOTAL PSS of a warm app wandered over a 94 MB range in five minutes
// (834, 805, 841, 872, 801, 778 MB), from GC timing and WebView compositor tiles. 128 MB is a little over that
// range and about 15% of the ~830 MB the app holds, so noise cannot trip it; a real leak of 25 MB per minute or more
// does (a day of use would be 36 GB). Readings come about each minute and are the median of three taken 2 s apart
// for the first and last. Override the length with DEVICE_QA_MEMORY_SECONDS.
import * as adb from '../adb.mjs';
import { scrollToTop, swipePage } from '../gestures.mjs';
import { totalPss } from '../probes.mjs';
import { ROUTE, openMain } from './open.mjs';

const LIMIT_KB = 128 * 1024;
const median = (v) => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];

async function reading() {
  const v = [];
  for (let i = 0; i < 3; i++) {
    v.push(await totalPss());
    await adb.sleep(2000);
  }
  return v.sort((a, b) => a - b)[1];
}

async function cycle(ctx, i) {
  const s = ctx.screen;
  const y = s.h * (0.35 + 0.07 * (i % 6));
  await adb.pen(s.w * 0.3, y, s.w * 0.6, y + s.h * 0.06, 500);
  await adb.sleep(300);
  await swipePage(s, { down: i % 8 < 5, frac: 0.35 });
  await adb.sleep(1800);
}

export default async function memory(ctx) {
  const { report } = ctx;
  const g = 'memory';
  const seconds = Number(process.env.DEVICE_QA_MEMORY_SECONDS || 300);
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  for (let i = 0; i < 4; i++) await cycle(ctx, i);
  const before = await reading();
  const series = [before];
  const end = Date.now() + seconds * 1000;
  let i = 4;
  let nextSample = Date.now() + 60000;
  while (Date.now() < end) {
    await cycle(ctx, i++);
    if (Date.now() >= nextSample) {
      series.push(await totalPss());
      nextSample += 60000;
    }
  }
  const after = await reading();
  series.push(after);
  const first = median(series.slice(0, 3));
  const last = median(series.slice(-3));
  const growth = last - first;
  report.check(g, 'TOTAL PSS growth over the run', before !== null && growth <= LIMIT_KB, {
    measured: `${(first / 1024).toFixed(0)} MB -> ${(last / 1024).toFixed(0)} MB (${growth >= 0 ? '+' : ''}${(growth / 1024).toFixed(1)} MB) after ${i - 4} cycles in ${seconds} s`,
    threshold: '<= +128 MB',
    evidence: `PSS series (MB, about each minute): ${series.map((k) => (k / 1024).toFixed(0)).join(', ')}`,
  });
  report.note(`memory: ${i - 4} scroll+draw cycles; strokes drawn on the QA board stay until the next run rolls it back`);
}
