// Check 5 (lifecycle): HOME + relaunch, rotate to portrait and back, force-stop + relaunch. After each, a new
// stroke still renders (ink frames, no stall, pixels, server mark) and the earlier marks are still there.
import * as adb from '../adb.mjs';
import { waitForPage } from '../cdp.mjs';
import { drawStroke, roundState, scrollToTop, settle, strokeVerdicts } from '../gestures.mjs';
import { measureScreen, notePid, openFrame, orientationName, refocus } from '../session.mjs';
import { ROUTE, openMain } from './open.mjs';

async function afterStep(ctx, label, expectMarks) {
  const g = 'lifecycle';
  const page = await waitForPage(ROUTE, 40000);
  if (!page) return ctx.report.check(g, `${label}: page back`, false, { measured: 'no live page after 40 s', threshold: 'page attached' });
  await adb.sleep(1500);
  const have = (await roundState()).marks;
  ctx.report.check(g, `${label}: earlier marks still on server`, have >= expectMarks, { measured: `${have} marks`, threshold: `>= ${expectMarks}` });
  const stroke = await drawStroke(ctx, { n: ctx.strokeNo });
  strokeVerdicts(ctx.report, g, `${label}: new stroke`, stroke);
  return stroke.marks;
}

export default async function lifecycle(ctx) {
  if (!ctx.frame) await openMain(ctx);
  await scrollToTop(ctx, ROUTE);
  let marks = (await roundState()).marks;

  await adb.home();
  await adb.sleep(2500);
  await adb.sh(`am start -n ${adb.PKG}/.MainActivity`);
  await adb.sleep(2500);
  await notePid();
  marks = (await afterStep(ctx, 'home + relaunch', marks)) ?? marks;

  const here = (await adb.screen()).rot % 2;
  const other = 1 - here;
  await adb.setRotation(other);
  await adb.sleep(3500);
  const turned = await adb.screen();
  ctx.report.check('lifecycle', `rotated to ${orientationName(other)}`, (turned.rot % 2) === other, { measured: `${turned.w}x${turned.h}`, threshold: orientationName(other) });
  await adb.setRotation(here);
  await adb.sleep(3500);
  ctx.screen = await measureScreen();
  await refocus(ctx.frame.frame, ROUTE);
  marks = (await afterStep(ctx, 'rotate away and back', marks)) ?? marks;

  await adb.forceStop();
  await adb.sleep(1000);
  ctx.frame = await openFrame(ROUTE);
  await notePid();
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  await afterStep(ctx, 'force-stop + relaunch', marks);
}
