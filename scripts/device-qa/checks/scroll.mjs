// Check 2 (scroll): finger-scroll the page to the bottom; the viewport must never shift inside the WebView.
import { Sampler, sample } from '../cdp.mjs';
import { dragOnStill, rendererHealthy, scrollToTop, settle, shot, swipePage } from '../gestures.mjs';
import { bufferStalls, logbook } from '../probes.mjs';
import { appPids } from '../session.mjs';
import { ROUTE, openMain } from './open.mjs';

/** Scrolls to the bottom in finger swipes and returns every sample taken during and between swipes. */
export async function scrollToBottom(ctx, route, maxSteps = 30) {
  await logbook.drain();
  const sampler = new Sampler(route, 70).start();
  let last = -1;
  let steps = 0;
  for (; steps < maxSteps; steps++) {
    await swipePage(ctx.screen, { down: true, frac: 0.55 });
    const y = await settle(route);
    if (y === last) break;
    last = y;
  }
  const samples = await sampler.stop();
  const stalls = bufferStalls(await logbook.drain(), appPids());
  return { samples, steps, stalls, final: await sample(route) };
}

export async function scrollVerdicts(report, g, label, ctx, r, { endless = false, topY = 0 } = {}) {
  const s = r.samples;
  const bad = (pred) => s.filter(pred);
  const vv = bad((x) => Math.abs(x.vvHeight - x.innerHeight) > 0.5);
  const off = bad((x) => Math.abs(x.vvOffsetTop) > 0.5);
    // topY: where the view sits on screen. 0 for the main frame; null = wherever the first sample found it (a frame the
  // camera placed lower must still not move while the page scrolls).
  const wantY = topY ?? s[0]?.view.screenY;
  const sy = bad((x) => x.view.screenY !== wantY || x.view.width !== ctx.screen.pixels.width || x.view.height !== ctx.screen.pixels.height);
  const f = r.final;
  const expected = f ? f.scrollHeight - f.innerHeight : NaN;
  const ev = (list) => (list.length ? `bad samples at scrollY ${Math.min(...list.map((x) => x.scrollY))}..${Math.max(...list.map((x) => x.scrollY))} of ${s.length} (page scrollY max ${f ? f.scrollHeight - f.innerHeight : '?'})\n` : '') + list.slice(0, 3).map((x) => `scrollY=${x.scrollY} vvH=${x.vvHeight} innerH=${x.innerHeight} offTop=${x.vvOffsetTop} screenY=${x.view.screenY}`).join('\n');
  report.check(g, `${label}: visualViewport.height = innerHeight (${s.length} samples)`, vv.length === 0, { measured: `${vv.length} bad`, threshold: '0', evidence: ev(vv) });
  report.check(g, `${label}: visualViewport.offsetTop = 0`, off.length === 0, { measured: `${off.length} bad`, threshold: '0', evidence: ev(off) });
  report.check(g, `${label}: WebView screenY ${topY === null ? 'constant' : '= 0'}, size = page pixels (${ctx.screen.pixels.width}x${ctx.screen.pixels.height})`, sy.length === 0, { measured: `${sy.length} bad`, threshold: topY === null ? `0 (screenY ${wantY})` : '0', evidence: ev(sy) });
  await rendererHealthy(ctx, g, label, r.stalls);
  if (endless) {
    // A feed that keeps growing has no bottom: it must keep scrolling and the viewport invariants above must hold.
    return report.check(g, `${label}: keeps scrolling as the page grows`, !!f && f.scrollY > 3 * f.innerHeight, { measured: f ? `scrollY ${f.scrollY}, scrollHeight ${f.scrollHeight}` : 'no page', threshold: 'scrollY > 3 viewports' });
  }
  report.check(g, `${label}: final scrollY = scrollHeight - innerHeight`, !!f && Math.abs(f.scrollY - expected) <= 2, {
    measured: f ? `${f.scrollY} vs ${expected} (steps ${r.steps})` : 'no page',
    threshold: '+-2 px',
  });
}

export default async function scroll(ctx) {
  const g = 'scroll';
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const r = await scrollToBottom(ctx, ROUTE);
  await shot(ctx, 'scroll-bottom');
  await scrollVerdicts(ctx.report, g, 'to bottom', ctx, r);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  // Portrait only: the part of the screen below the live window. The app does not scroll the page from there yet.
  const still = await dragOnStill(ctx, ROUTE);
  if (still) {
    ctx.report.gap(g, 'drag on the still part scrolls the page', still.scrolled >= 100, {
      measured: `scrollY ${still.before} -> ${still.after} (${still.scrolled} css px) for a drag from ${still.from.map(Math.round)} below the live window (y >= ${ctx.screen.live.h})`,
      threshold: '>= 100 css px',
      evidence: 'App change "drag anywhere on the locked frame scrolls the page" not landed.',
    });
    await scrollToTop(ctx, ROUTE);
    await settle(ROUTE);
  }
}
