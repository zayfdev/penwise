// Check 9 (round): tap Send on the tablet, look shows the round sent, resolve it, the next look shows the next round open.
import * as adb from '../adb.mjs';
import { drawStroke, roundState, scrollToTop } from '../gestures.mjs';
import { mcpClient, openFrame } from '../session.mjs';
import { ROUTE, openMain } from './open.mjs';

async function tapSend() {
  const nodes = await adb.uiNodes();
  const send = nodes.find((n) => n.pkg === adb.PKG && (n.text === 'Send' || n.desc === 'Send'));
  if (!send) return false;
  await adb.tap(...adb.center(send));
  return true;
}

export default async function round(ctx) {
  const { report } = ctx;
  const g = 'round';
  if (!ctx.frame) await openMain(ctx);
  else ctx.frame = await openFrame(ROUTE);
  await scrollToTop(ctx, ROUTE);
  let st = await roundState();
  if (st.marks === 0) {
    await drawStroke(ctx, { n: ctx.strokeNo });
    st = await roundState();
  }
  const n = st.round;
  report.check(g, 'open round holds marks before Send', st.status === 'open' && st.marks >= 1, { measured: `round ${n} ${st.status}, ${st.marks} mark(s)`, threshold: 'open, >= 1' });
  const found = await tapSend();
  report.check(g, 'Send button found by uiautomator', found, { measured: found ? 'found' : 'no node with text Send', threshold: 'present' });
  let after = st;
  for (let i = 0; i < 20; i++) {
    await adb.sleep(500);
    after = await roundState();
    if (after.status === 'sent') break;
  }
  report.check(g, 'look reports the round as sent', after.round === n && after.status === 'sent', { measured: `round ${after.round} ${after.status}`, threshold: `round ${n} sent` });
  const client = await mcpClient();
  const t0 = Date.now();
  const res = await client.call('resolve', { round: n });
  report.check(g, `resolve round ${n}`, !res.isError, { measured: res.isError ? res.text.slice(0, 160) : `ok in ${Date.now() - t0} ms`, threshold: 'no error' });
  const next = await roundState();
  report.check(g, 'next look shows the next round open', next.round === n + 1 && next.status === 'open', { measured: `round ${next.round} ${next.status}, ${next.marks} mark(s)`, threshold: `round ${n + 1} open` });
  ctx.frame = await openFrame(ROUTE);
}
