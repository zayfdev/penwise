// Check 11: the crash buffer stays empty and logcat has no FATAL for the app pids, over the whole run.
import { crashBuffer, fatalLines, logbook } from '../probes.mjs';
import { appPids, notePid } from '../session.mjs';

export async function finalVerdicts(ctx) {
  await notePid();
  const text = (await logbook.drain()) + logbook.text;
  const crash = await crashBuffer();
  const crashSeen = logbook.text.split('\n').filter((l) => /^\S+\s+\S+\s+\d+\s+\d+\s+[FE]\s+(DEBUG|AndroidRuntime)/.test(l) && /dev\.penwise/.test(l));
  const fatal = fatalLines(text, appPids());
  ctx.report.check('crash', 'crash buffer empty', crash.length === 0 && crashSeen.length === 0, {
    measured: `${crash.length + crashSeen.length} line(s)`,
    threshold: '0',
    evidence: [...crash, ...crashSeen].slice(0, 5).join('\n'),
  });
  ctx.report.check('crash', 'no FATAL for app pids', fatal.length === 0, {
    measured: `${fatal.length} line(s) over pids ${appPids().join(',')}`,
    threshold: '0',
    evidence: fatal.slice(0, 5).join('\n'),
  });
}
