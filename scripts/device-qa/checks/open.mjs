// Check 1 (open): the QA board opens fitted on its frame; the WebView, the page and the server agree on sizes.
import * as adb from '../adb.mjs';
import { sample } from '../cdp.mjs';
import { scrollToTop, settle, shot } from '../gestures.mjs';
import { docHeight } from '../server.mjs';
import { measureScreen, openFrame } from '../session.mjs';

export const ROUTE = '/sticky-header/';

/** Opens the main frame and returns the sample; shared by later groups through ctx.frame. */
export async function openMain(ctx) {
  ctx.screen = await measureScreen();
  ctx.frame = await openFrame(ROUTE);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  return sample(ROUTE);
}

export default async function open(ctx) {
  const { report } = ctx;
  const g = 'open';
  const s = await openMain(ctx);
  if (!s) return report.add(g, 'live page', 'fail', { measured: 'no live WebView page after 90 s', threshold: 'page attached' });
  await shot(ctx, 'open');
  const { width: w, height: h } = ctx.screen.pixels;
  const v = s.view;
  report.check(g, 'WebView = page pixels for the preset', v.width === w && v.height === h, {
    measured: `${v.width}x${v.height}`,
    threshold: `${w}x${h} (PagePixels: 1440x900 at density ${ctx.screen.density}, window ${ctx.screen.w}x${ctx.screen.h}; live window ${ctx.screen.live.w}x${ctx.screen.live.h} of ${ctx.screen.w}x${ctx.screen.h})`,
  });
  report.check(g, 'WebView screenY = 0', v.screenY === 0, { measured: v.screenY, threshold: '0' });
  report.check(g, 'innerWidth = 1440', s.innerWidth === 1440, { measured: s.innerWidth, threshold: '1440' });
  report.check(g, 'innerHeight = 900', s.innerHeight === 900, { measured: s.innerHeight, threshold: '900' });
  report.check(g, 'visualViewport.height = innerHeight', Math.abs(s.vvHeight - s.innerHeight) < 0.5, {
    measured: `${s.vvHeight} vs ${s.innerHeight}`,
    threshold: 'equal',
  });
  const doc = await docHeight(ctx.frame.frame);
  report.check(g, 'scrollHeight = server still doc height', !!doc && Math.abs(s.scrollHeight - doc.height) <= 2, {
    measured: `page ${s.scrollHeight} vs still ${doc?.height}`,
    threshold: '+-2 px',
  });
}
