// Check 7 (reload): a file change in the served QA copy reloads the page; ink still renders and stays glued.
import * as adb from '../adb.mjs';
import { evalLive, waitForPage } from '../cdp.mjs';
import { drawStroke, scrollToTop, settle, strokeVerdicts } from '../gestures.mjs';
import { probeLiveReload, settleAfterProbe } from '../livereload.mjs';
import { ROUTE, openMain } from './open.mjs';
import { glueMeasure } from './glue.mjs';

export default async function reload(ctx) {
  const { report } = ctx;
  const g = 'reload';
  if (!ctx.frame) await openMain(ctx);
  // Start from a freshly loaded page: a page that lost its live-reload socket earlier (network group) is its own check.
  await evalLive('location.reload(); 1', ROUTE);
  await adb.sleep(2500);
  await waitForPage(ROUTE, 30000);
  await adb.sleep(1500);
  await scrollToTop(ctx, ROUTE);
  const r = await probeLiveReload(ROUTE);
  report.check(g, 'page reloads after a file change', r.reloaded, { measured: r.reloaded ? `${r.ms} ms` : 'no reload within 30 s', threshold: '< 30 s' });
  await settleAfterProbe(ROUTE);
  if (!(await waitForPage(ROUTE, 20000))) return report.check(g, 'live page after reload', false, { measured: 'none', threshold: 'attached' });
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const stroke = await drawStroke(ctx, { n: ctx.strokeNo });
  strokeVerdicts(report, g, 'after reload', stroke);
  await glueMeasure(ctx, 'after reload');
  await scrollToTop(ctx, ROUTE);
}
