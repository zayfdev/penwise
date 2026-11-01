// device-qa runner: node scripts/device-qa/run.mjs [group ...]. Exits non-zero on any failing or erroring check.
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import * as adb from './adb.mjs';
import { logbook } from './probes.mjs';
import { Report } from './report.mjs';
import { QA_DIR, ensureProject, prepareSites, waitServer } from './server.mjs';
import { restoreState, saveState } from './session.mjs';

const GROUPS = ['open', 'scroll', 'ink', 'glue', 'lifecycle', 'network', 'reload', 'fixtures', 'round', 'memory'];
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
const report = new Report(dir, { startedAt: new Date().toISOString(), groups });
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
  ctx.screen = await adb.screen();
  for (const g of groups) {
    process.stderr.write(`== ${g}\n`);
    try {
      const mod = await import(`./checks/${g}.mjs`);
      await mod.default(ctx);
    } catch (error) {
      report.add(g, 'harness', 'error', { measured: error.message, evidence: error.stack });
    }
  }
  const { finalVerdicts } = await import('./checks/crash.mjs');
  await finalVerdicts(ctx);
} catch (error) {
  report.add('setup', 'harness', 'error', { measured: error.message, evidence: error.stack });
}
const path = await report.write();
process.stdout.write(`\n${report.table()}\n\nreport: ${path}\n`);
await finish(report.failures.length ? 1 : 0);
