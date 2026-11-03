// Check 4 (glue): after a one-finger page scroll, an ink stroke moves by the same on-screen distance as the page.
import { sample } from '../cdp.mjs';
import { darkPixels, findShift, verticalShift } from '../png.mjs';
import { drawStroke, scrollToTop, settle, shot, swipePage } from '../gestures.mjs';
import * as adb from '../adb.mjs';
import { ROUTE, openMain } from './open.mjs';
import glueSkew from './glue-skew.mjs';
import glueSteps from './glue-steps.mjs';

/** Draws a stroke, scrolls the page by a slow finger swipe, and measures stroke shift vs page shift. */
// A full run draws many strokes and a later one can partly land on an earlier one: 200 new dark px still give a
// clear shift (a portrait stroke leaves about 2300).
const MIN_STROKE_PX = 200;

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
  const scale = ctx.screen.live.w / s0.innerWidth; // screen px per css px (the view is drawn scaled to the screen width)
  const css = (s1?.scrollY ?? y1) - s0.scrollY;
  const expected = -css * scale;
  const strokeBox = stroke.box ?? [0, 0, 1, 1];
  const rect = [Math.round(strokeBox[0]) - 40, Math.round(strokeBox[1]) - 40, Math.round(strokeBox[2]) + 40, Math.round(strokeBox[3]) + 40];
  const points = stroke.dark.length ? stroke.dark : darkPixels(stroke.after, rect, 50);
  // Page content clear of the strokes and of the fixed popups (frame chip top left, toolbar top centre) that would
  // pull the correlation toward a zero shift: the left band in landscape, the right band in portrait.
  const portrait = ctx.screen.h > ctx.screen.w;
  // Rows that stay clear of the page's fixed header (about 150 px) in both screenshots over the whole search window.
  const top = Math.max(260, 150 - (Math.round(expected) - 60));
  const bottom = ctx.screen.live.h - 60 - Math.max(0, Math.round(expected) + 60);
  const refRect = [Math.round(ctx.screen.w * (portrait ? 0.72 : 0.04)), top, Math.round(ctx.screen.w * (portrait ? 0.9 : 0.2)), bottom];
  const pageShift = verticalShift(stroke.after, after, refRect, Math.round(expected) - 60, Math.round(expected) + 60);
  // Strokes of earlier checks sit on the board at regular spacing (about 60 px), so the stroke is searched only within
  // +-20 px of where the page itself moved: a stroke that stayed put or lagged by more than that is not found.
  const found = findShift(after, points, pageShift.dy - 20, pageShift.dy + 20, 60);
  const diff = found.dy - pageShift.dy;
  ctx.report.check(g, `${label}: page scrolled`, Math.abs(css) >= 100, { measured: `${css.toFixed(1)} css px (${expected.toFixed(1)} screen px)`, threshold: '>= 100 css px' });
  ctx.report.check(g, `${label}: stroke found after scroll`, found.score >= 0.5, { measured: `${(found.score * 100).toFixed(0)}% of ${points.length} px`, threshold: '>= 50%' });
  if (points.length < MIN_STROKE_PX) {
    // A handful of stroke pixels cannot give a trustworthy shift: neither pass nor fail.
    ctx.report.add(g, `${label}: stroke moves with the page`, 'error', {
      measured: `only ${points.length} stroke px found, diff ${diff}`,
      threshold: `>= ${MIN_STROKE_PX} stroke px`,
      evidence: `inconclusive; see stroke-*-after.png to check the stroke was drawn at all`,
    });
    return { found, pageShift, expected };
  }
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
  await glueSteps(ctx);
  await glueSkew(ctx);
}
