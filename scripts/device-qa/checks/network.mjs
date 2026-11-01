// Check 6 (network): strokes drawn with Wi-Fi off reach the server after Wi-Fi returns; a server restart is survived.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as adb from '../adb.mjs';
import { drawStroke, roundState, scrollToTop, strokeVerdicts, waitMarks } from '../gestures.mjs';
import { probeLiveReload, settleAfterProbe } from '../livereload.mjs';
import { waitServer } from '../server.mjs';
import { waitSynced } from '../session.mjs';
import { ROUTE, openMain } from './open.mjs';

const run = promisify(execFile);

async function waitReachable(timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const out = await adb.sh('ping -c 1 -W 1 192.168.1.19 2>&1 | grep -c "1 received"').catch(() => '0');
    if (out.trim() === '1') return Date.now() - start;
    await adb.sleep(500);
  }
  return null;
}

export default async function network(ctx) {
  const { report } = ctx;
  const g = 'network';
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  const base = (await roundState()).marks;

  await adb.setWifi(false);
  await adb.sleep(5000);
  const a = await drawStroke(ctx, { n: ctx.strokeNo, server: false });
  ctx.offlineMarks = base;
  const b = await drawStroke(ctx, { n: ctx.strokeNo, server: false });
  report.check(g, 'offline strokes render locally', (a.framesAfter ?? 0) > (a.framesBefore ?? 0) && (b.framesAfter ?? 0) > (b.framesBefore ?? 0) && a.changed.length >= 300 && b.changed.length >= 300, {
    measured: `frames ${a.framesBefore}->${a.framesAfter}, ${b.framesBefore}->${b.framesAfter}; px ${a.changed.length}, ${b.changed.length}`,
    threshold: 'frames rise, >= 300 px each',
  });
  const t0 = Date.now();
  await adb.setWifi(true);
  const reachable = await waitReachable(30000);
  const wifiMs = reachable === null ? null : Date.now() - t0;
  const got = await waitMarks(base + 2, 60000);
  const total = Date.now() - t0;
  report.check(g, 'two offline marks reach the server after Wi-Fi on', got.marks >= base + 2 && total <= 15000, {
    measured: `${base} -> ${got.marks} marks, ${total} ms after Wi-Fi enable (PC reachable from the tablet after ${wifiMs} ms)`,
    threshold: '+2 within 15 s',
    evidence: `Wi-Fi link took ${wifiMs} ms; the app took ${total - (wifiMs ?? 0)} ms more to deliver`,
  });
  ctx.offlineMarks = undefined;
  await waitSynced(30000);

  const marks = (await roundState()).marks;
  await run('systemctl', ['--user', 'restart', 'penwise']);
  const tUp = Date.now();
  const up = await waitServer(60000);
  report.check(g, 'server back after restart', up, { measured: `${Date.now() - tUp} ms`, threshold: '< 60 s' });
  const synced = await waitSynced(40000);
  report.check(g, 'app reconnects after server restart', synced !== null, { measured: synced === null ? 'never showed Synced in 40 s' : `Synced after ${synced} ms`, threshold: '< 40 s' });
  const c = await drawStroke(ctx, { n: ctx.strokeNo });
  report.check(g, 'new stroke reaches server after restart', c.marks === marks + 1, {
    measured: `${marks} -> ${c.marks} in ${c.markMs} ms`,
    threshold: '+1 within 12 s',
  });
  strokeVerdicts(report, g, 'after restart', { ...c, marks: c.marks, marksBefore: c.marksBefore });
  const live = await probeLiveReload(ROUTE, 20000);
  report.check(g, 'live reload still works after the outage and restart', live.reloaded, {
    measured: live.reloaded ? `reloaded in ${live.ms} ms` : 'the page already open did not reload within 20 s',
    threshold: 'reload within 20 s',
    evidence: live.reloaded ? '' : 'bridge.js liveReload() gives up after 5 reconnect attempts (about 15 s of 1..5 s steps); a longer outage leaves the loaded page without live reload until it is reloaded',
  });
  await settleAfterProbe(ROUTE);
}
