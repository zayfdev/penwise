// Check (rotate): the same WebView is rotated in place; the page keeps its document and scroll, the view is laid out for
// the new window (frames/PagePixels.kt), the visual viewport is whole, and an ink stroke still moves with the page.
import * as adb from '../adb.mjs';
import { evalLive, sample } from '../cdp.mjs';
import { scrollToTop, settle, swipePage } from '../gestures.mjs';
import { holdOrientation, measureScreen, orientationName } from '../session.mjs';
import { ROUTE, openMain } from './open.mjs';
import { glueMeasure } from './glue.mjs';

export default async function rotate(ctx) {
  const g = 'rotate';
  const { report } = ctx;
  const home = (await adb.screen()).rot;
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  for (let i = 0; i < 2; i++) {
    await swipePage(ctx.screen, { down: true, frac: 0.4 });
    await settle(ROUTE);
  }
  await evalLive('window.__pwRotate = 42', ROUTE);
  const before = await sample(ROUTE);
  for (const rot of [1 - (home % 2), home % 2]) {
    const label = `rotated in place to ${orientationName(rot)}`;
    await holdOrientation(rot);
    await adb.sleep(3000);
    ctx.screen = await measureScreen();
    await settle(ROUTE);
    const s = await sample(ROUTE);
    if (!s) {
      report.add(g, label, 'fail', { measured: 'no live page after the rotation', threshold: 'page attached' });
      continue;
    }
    const marker = await evalLive('window.__pwRotate', ROUTE);
    const { width, height } = ctx.screen.pixels;
    report.check(g, `${label}: same WebView and document (page marker kept)`, marker === 42, { measured: String(marker), threshold: '42' });
    report.check(g, `${label}: scrollY kept`, Math.abs(s.scrollY - before.scrollY) <= 2, { measured: `${s.scrollY} vs ${before.scrollY}`, threshold: '+-2 css px' });
    report.check(g, `${label}: view = page pixels for the new window`, s.view.width === width && s.view.height === height, { measured: `${s.view.width}x${s.view.height}`, threshold: `${width}x${height}` });
    report.check(g, `${label}: innerWidth/innerHeight = 1440x900`, s.innerWidth === 1440 && s.innerHeight === 900, { measured: `${s.innerWidth}x${s.innerHeight}`, threshold: '1440x900' });
    report.check(g, `${label}: visualViewport.height = innerHeight, offsetTop 0`, Math.abs(s.vvHeight - s.innerHeight) < 0.5 && Math.abs(s.vvOffsetTop) < 0.5, { measured: `${s.vvHeight} vs ${s.innerHeight}, offsetTop ${s.vvOffsetTop}`, threshold: '< 0.5 px' });
    await glueMeasure(ctx, label);
    await scrollToTop(ctx, ROUTE);
    await settle(ROUTE);
    before.scrollY = 0;
  }
  ctx.screen = await measureScreen();
}
