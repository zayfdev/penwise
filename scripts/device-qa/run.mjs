// device-qa runner: node scripts/device-qa/run.mjs [group ...]. Exits non-zero on any failing or erroring check.
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import * as adb from './adb.mjs';
import { logbook } from './probes.mjs';
import { Report } from './report.mjs';
import { QA_DIR, ensureProject, prepareSites, waitServer } from './server.mjs';
import { holdOrientation, measureScreen, orientationName, restoreState, saveState } from './session.mjs';

const GROUPS = ['open', 'scroll', 'ink', 'glue', 'rotate', 'lifecycle', 'network', 'reload', 'fixtures', 'round', 'memory'];
// Geometry groups depend on the orientation (view size, live window); the others run once, in the first orientation.
const GEOMETRY = ['open', 'scroll', 'glue'];
const orient = (process.argv.find((a) => a.startsWith('--orient=')) ?? '--orient=current').slice(9);
if (!['current', 'landscape', 'portrait', 'both'].includes(orient)) {
  process.stderr.write('--orient= must be current, landscape, portrait or both\n');
  process.exit(2);
}
const wanted = process.argv.slice(2).filter((a) => !a.startsWith('-'));
const groups = wanted.length ? wanted : GROUPS;
for (const g of groups) {
  if (!GROUPS.includes(g)) {
    process.stderr.write(`unknown group ${g}; groups: ${GROUPS.join(' ')}\n`);
    process.exit(2);
  }
}

const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
const dir = join(QA_DIR, stamp);
await mkdir(join(dir, 'shots'), { recursive: true });
const report = new Report(dir, { startedAt: new Date().toISOString(), groups, orient });
const ctx = { dir, report, strokeNo: 0, screen: null, frame: null };

let restored = false;
async function finish(code) {
  if (restored) return;
  restored = true;
  await restoreState().catch((e) => process.stderr.write(`restore failed: ${e.message}\n`));
  process.exit(code);
}
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => finish(130));

try {
  if (!(await adb.sh('echo ok').catch(() => ''))) throw new Error('no adb device');
  if (!(await waitServer(5000))) throw new Error('penwise server not reachable on :7420');
  await saveState();
  // A running app keeps the pen's colour and tool from the last session; the checks read ink as dark pixels, so start
  // from the app's defaults (the lifecycle group restarts it later anyway).
  await adb.forceStop();
  const pkg = await adb.sh(`dumpsys package ${adb.PKG}`);
  report.meta.device = (await adb.sh('getprop ro.product.model')).trim();
  report.meta.appVersion = /versionName=(\S+)/.exec(pkg)?.[1];
  report.meta.appInstalled = /lastUpdateTime=([^\n]+)/.exec(pkg)?.[1];
  process.stderr.write(`app ${report.meta.appVersion} installed ${report.meta.appInstalled} on ${report.meta.device}\n`);
  await adb.sleep(100);
  await prepareSites();
  await ensureProject();
  await logbook.drain();
  const { prepareBoard } = await import('./board.mjs');
  await prepareBoard(ctx);
  // Rotation under test: the owner's current one (locked for the run), a named one, or both (current first).
  const start = (await adb.screen()).rot % 2;
  const wantRot = orient === 'landscape' ? 1 : orient === 'portrait' ? 0 : start;
  const plan = [{ rot: wantRot, list: groups }];
  if (orient === 'both') plan.push({ rot: 1 - wantRot, list: groups.filter((g) => GEOMETRY.includes(g)) });
  const labelled = orient === 'both';
  for (const step of plan) {
    await holdOrientation(step.rot);
    ctx.screen = await measureScreen();
    ctx.frame = null;
    report.suffix = labelled ? ` [${orientationName(step.rot)}]` : '';
    report.meta.orientations = [...(report.meta.orientations ?? []), { orientation: orientationName(step.rot), screen: `${ctx.screen.w}x${ctx.screen.h}`, density: ctx.screen.density, expectedView: ctx.screen.pixels, live: ctx.screen.live }];
    process.stderr.write(`-- ${orientationName(step.rot)} ${ctx.screen.w}x${ctx.screen.h}, view ${ctx.screen.pixels.width}x${ctx.screen.pixels.height}, live ${ctx.screen.live.w}x${ctx.screen.live.h}\n`);
    for (const g of step.list) {
      process.stderr.write(`== ${g}\n`);
      try {
        const mod = await import(`./checks/${g}.mjs`);
        await mod.default(ctx);
      } catch (error) {
        report.add(g, 'harness', 'error', { measured: error.message, evidence: error.stack });
      }
    }
  }
  report.suffix = '';
  const { finalVerdicts } = await import('./checks/crash.mjs');
  await finalVerdicts(ctx);
} catch (error) {
  report.add('setup', 'harness', 'error', { measured: error.message, evidence: error.stack });
}
const path = await report.write();
process.stdout.write(`\n${report.table()}\n\nreport: ${path}\n`);
await finish(report.failures.length ? 1 : 0);
