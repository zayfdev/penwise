// Check 4 (glue): after a one-finger page scroll, an ink stroke moves by the same on-screen distance as the page.
import { sample } from '../cdp.mjs';
import { darkPixels, findShift, verticalShift } from '../png.mjs';
import { drawStroke, scrollToTop, settle, shot, swipePage } from '../gestures.mjs';
import * as adb from '../adb.mjs';
import { ROUTE, openMain } from './open.mjs';

/** Draws a stroke, scrolls the page by a slow finger swipe, and measures stroke shift vs page shift. */
export async function glueMeasure(ctx, label) {
  const g = 'glue';
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const stroke = await drawStroke(ctx, { n: ctx.strokeNo });
  const s0 = await sample(ROUTE);
  await swipePage(ctx.screen, { down: true, slow: true, frac: 0.25 });
  const y1 = await settle(ROUTE);
  await adb.sleep(1200);
  const s1 = await sample(ROUTE);
  const after = await shot(ctx, `glue-${label}-scrolled`);
  const scale = s0.view.width / s0.innerWidth;
  const css = (s1?.scrollY ?? y1) - s0.scrollY;
  const expected = -css * scale;
  const strokeBox = stroke.box ?? [0, 0, 1, 1];
  const rect = [Math.round(strokeBox[0]) - 40, Math.round(strokeBox[1]) - 40, Math.round(strokeBox[2]) + 40, Math.round(strokeBox[3]) + 40];
  const points = stroke.dark.length ? stroke.dark : darkPixels(stroke.after, rect, 50);
  const win = 150;
  const found = findShift(after, points, Math.round(expected) - win, Math.round(expected) + win, 60);
  const refRect = [60, 220, Math.round(ctx.screen.w * 0.2), ctx.screen.h - 60];
  const pageShift = verticalShift(stroke.after, after, refRect, Math.round(expected) - 60, Math.round(expected) + 60);
  const diff = found.dy - pageShift.dy;
  ctx.report.check(g, `${label}: page scrolled`, Math.abs(css) >= 100, { measured: `${css.toFixed(1)} css px (${expected.toFixed(1)} screen px)`, threshold: '>= 100 css px' });
  ctx.report.check(g, `${label}: stroke found after scroll`, found.score >= 0.5, { measured: `${(found.score * 100).toFixed(0)}% of ${points.length} px`, threshold: '>= 50%' });
  ctx.report.check(g, `${label}: stroke moves with the page`, found.score >= 0.5 && Math.abs(diff) <= 4, {
    measured: `stroke ${found.dy} px, page ${pageShift.dy} px (scrollY ${expected.toFixed(1)}), diff ${diff}`,
    threshold: '|diff| <= 4 px',
    evidence: `shots glue-${label}-scrolled.png / stroke-*-after.png`,
  });
  return { found, pageShift, expected };
}

export default async function glue(ctx) {
  if (!ctx.frame) await openMain(ctx);
  await glueMeasure(ctx, 'finger scroll');
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
}
