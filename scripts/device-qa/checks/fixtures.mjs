// Check 8 (fixtures): every fixture site opened as a frame, finger-scrolled to the bottom, with check 2's invariants.
// infinite-feed is expected to switch to viewport mode (box = the preset viewport, 1440x900): asserted instead of
// the document-length checks.
import * as adb from '../adb.mjs';
import { sample, waitForPage } from '../cdp.mjs';
import { frameBox, scrollToTop, settle, shot } from '../gestures.mjs';
import { mcpClient, waitSynced } from '../session.mjs';
import { openMain } from './open.mjs';
import { scrollToBottom, scrollVerdicts } from './scroll.mjs';

export const FIXTURES = ['sticky-header', 'hero-100vh', 'reveal-on-scroll', 'lazy-images', 'infinite-feed', 'inner-scroll', 'smooth-scroll', 'animated', 'stacking', 'noisy-image'];

async function focusFrame(route) {
  const client = await mcpClient();
  const shown = await client.call('show', { frame: { route } });
  if (shown.isError) throw new Error(`show ${route}: ${shown.text}`);
  await client.call('show', { focus: { frame: shown.json.frame } });
  const page = await waitForPage(route, 60000);
  await waitSynced(10000);
  await adb.sleep(1500);
  return { id: shown.json.frame, page };
}

export default async function fixtures(ctx) {
  const { report } = ctx;
  if (!ctx.frame) await openMain(ctx);
  for (const name of FIXTURES) {
    const route = `/${name}/`;
    const g = `fixture ${name}`;
    try {
      const f = await focusFrame(route);
      if (!f.page) {
        report.check(g, 'live page', false, { measured: 'no live page in 60 s', threshold: 'attached' });
        continue;
      }
      await scrollToTop(ctx, route);
      await settle(route);
      const top = await sample(route);
      const box = await frameBox(f.id);
      if (name === 'infinite-feed') {
        report.check(g, 'viewport mode (box = 1440x900)', box?.w === 1440 && box?.h === 900, {
          measured: box ? `${box.w}x${box.h}` : 'no box',
          threshold: '1440x900',
        });
      } else {
        report.check(g, 'box height = page scrollHeight', !!top && !!box && Math.abs(box.h - top.scrollHeight) <= 2, {
          measured: `box ${box?.h} vs page ${top?.scrollHeight}`,
          threshold: '+-2 px',
        });
      }
      const endless = name === 'infinite-feed';
      const r = await scrollToBottom(ctx, route, endless ? 8 : 40);
      await shot(ctx, `fixture-${name}-bottom`);
      scrollVerdicts(report, g, 'scroll', ctx, r, { endless });
      await scrollToTop(ctx, route);
    } catch (error) {
      report.add(g, 'harness', 'error', { measured: error.message, evidence: error.stack });
    }
  }
}
