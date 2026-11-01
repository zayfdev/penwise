// Check 3 (ink): a pen stroke makes the low-latency renderer present frames, reaches the server, changes pixels.
import { scrollToTop, settle, drawStroke, strokeVerdicts } from '../gestures.mjs';
import { ROUTE, openMain } from './open.mjs';

export default async function ink(ctx) {
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const r = await drawStroke(ctx, { n: ctx.strokeNo });
  strokeVerdicts(ctx.report, 'ink', 'pen stroke', r);
  ctx.lastStroke = r;
}
