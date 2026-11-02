// Glue over a run of scrolls (owner, 2026-11-02: "when u scroll down / past it thats when the drawings start moving
// down aswell with it"): a stroke kept in view through several finger scrolls and flings, both ways, must stay on the
// page after each one, and so must a stroke bound to a column that pins (position: sticky) while its section scrolls.
import * as adb from '../adb.mjs';
import { evalLive } from '../cdp.mjs';
import { darkPixels } from '../png.mjs';
import { drawStroke, scrollToTop, settle, shot } from '../gestures.mjs';
import { ROUTE } from './open.mjs';

const MAX_DIFF = 4;
/** Dark px a stroke must leave to be tracked: a portrait screen draws the pen's line about 4 px wide. */
const MIN_STROKE_PX = 150;
/** The stroke is searched this far either side of where the page put it: a stroke left behind by a pinned column is
 * still found, so the diff is a number and not just "not found". */
const SEARCH = 420;

/**
 * Where did the stroke's dark `points` go in `img`: the dy in [lo, hi] with the most of them dark again, counting only
 * points that land between `y0` and `y1` (the page's dark sticky header passes for ink everywhere above y0) and at
 * least half of them. Ties go to the dy nearest `near`. {dy, score}; dy null when nothing scores.
 */
function shiftIn(img, points, { lo, hi, y0, y1, near }) {
  const dark = new Set(darkPixels(img, [0, y0, img.w, y1], 60).map(([x, y]) => y * img.w + x));
  let best = { dy: null, score: 0 };
  for (let dy = lo; dy <= hi; dy++) {
    let n = 0;
    let hit = 0;
    for (const [x, y] of points) {
      if (y + dy < y0 || y + dy >= y1) continue;
      n++;
      if (dark.has((y + dy) * img.w + x)) hit++;
    }
    if (n < points.length / 2) continue;
    const score = hit / n;
    if (score > best.score + 1e-9 || (Math.abs(score - best.score) < 1e-9 && Math.abs(dy - near) < Math.abs(best.dy - near))) best = { dy, score };
  }
  return best;
}

/** One finger scroll inside the live window, right of the strokes; `frac` of the live height, positive = down. */
async function scroll(ctx, { frac, ms }) {
  const live = ctx.screen.live;
  const x = live.w * 0.85;
  const [a, b] = frac > 0 ? [live.h * 0.8, live.h * (0.8 - frac)] : [live.h * 0.2, live.h * (0.2 - frac)];
  await adb.finger(x, a, x, b, ms);
}

const SLOW = 1400;
const FLING = 90;

/** Finger-scrolls down in slow drags until scrollY reaches `css` (or the steps run out); returns scrollY. */
async function scrollTo(ctx, css) {
  let y = await settle(ROUTE);
  for (let i = 0; i < 12 && y !== null && y < css; i++) {
    const frac = Math.min(0.3, ((css - y) * ctx.scale) / ctx.screen.live.h);
    await scroll(ctx, { frac: Math.max(0.05, frac), ms: SLOW });
    y = await settle(ROUTE);
  }
  return y;
}

/**
 * Draws `line` (screen px), then runs `steps` and measures after each one how far the stroke moved on screen against
 * how far the page did (scrollY). Steps that would carry the stroke out of the live window are measured but not
 * judged. Returns the per-step rows.
 */
async function strokeThrough(ctx, label, line, steps, extra = async () => ({})) {
  const g = 'glue';
  const stroke = await drawStroke(ctx, { n: ctx.strokeNo, line });
  const points = stroke.dark ?? [];
  if (points.length < MIN_STROKE_PX) {
    ctx.report.add(g, `${label}: stroke stays on the page`, 'error', {
      measured: `only ${points.length} stroke px`,
      threshold: `>= ${MIN_STROKE_PX} stroke px`,
      evidence: 'inconclusive; see stroke-*-after.png',
    });
    return [];
  }
  const sy0 = await evalLive('scrollY', ROUTE);
  const top = Math.min(line[1], line[3]);
  const bottom = Math.max(line[1], line[3]);
  const rows = [];
  // `steps` is a list, or a function of the rows so far that returns the next step (null: done).
  const next = typeof steps === 'function' ? steps : (done) => steps[done.length] ?? null;
  for (let i = 0, step = next(rows); step; i++, step = next(rows)) {
    await scroll(ctx, step);
    const sy = await settle(ROUTE);
    await adb.sleep(1000);
    const img = await shot(ctx, `glue-${label.replace(/\W+/g, '-')}-${i + 1}`);
    const page = -((sy ?? sy0) - sy0) * ctx.scale;
    // Below the page's dark sticky header (80 css px), whose pixels would pass for ink, and above the window's end.
    const inView = top + page > 90 * ctx.scale && bottom + page < ctx.screen.live.h;
    const band = { y0: Math.round(90 * ctx.scale), y1: ctx.screen.live.h, near: Math.round(page) };
    const found = shiftIn(img, points, { lo: Math.round(page) - SEARCH, hi: Math.round(page) + SEARCH, ...band });
    rows.push({ step: `${step.frac > 0 ? 'down' : 'up'} ${step.ms === FLING ? 'fling' : 'drag'}`, scrollY: +(sy ?? sy0).toFixed(1), page: +page.toFixed(1), stroke: found.dy, score: +found.score.toFixed(2), diff: found.dy === null ? null : +(found.dy - page).toFixed(1), inView, ...(await extra()) });
  }
  const judged = rows.filter((r) => r.inView);
  const worst = judged.reduce((m, r) => Math.max(m, r.score >= 0.5 && r.diff !== null ? Math.abs(r.diff) : Infinity), 0);
  ctx.report.check(g, `${label}: stroke stays on the page (${judged.length} of ${rows.length} steps in view)`, judged.length >= 3 && worst <= MAX_DIFF, {
    measured: `worst |diff| ${worst} px; ${judged.map((r) => `${r.step} ${r.diff}`).join(', ')}`,
    threshold: `|diff| <= ${MAX_DIFF} px after every step in view, >= 3 steps`,
    evidence: JSON.stringify(rows),
  });
  return rows;
}

/** A stroke on plain page content through drags and flings down and up the 5000 px page. */
async function longPage(ctx) {
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
  const { w, live } = { w: ctx.screen.w, live: ctx.screen.live };
  const y = live.h * 0.62;
  const line = [w * 0.32, y, w * 0.62, y + live.h * 0.03];
  await strokeThrough(ctx, 'scrolls and flings', line, [
    { frac: 0.08, ms: SLOW },
    { frac: 0.08, ms: SLOW },
    { frac: 0.05, ms: FLING },
    { frac: -0.08, ms: SLOW },
    { frac: -0.05, ms: FLING },
    { frac: 0.1, ms: SLOW },
  ]);
}

/** The pinned column's viewport top and its document top (CSS px). */
async function pinRect() {
  return evalLive('(() => { const r = document.getElementById("pin").getBoundingClientRect(); return { left: r.left, top: r.top, bottom: r.bottom, doc: r.top + scrollY }; })()', ROUTE);
}

/**
 * A stroke whose pen-down is on #pin (position: sticky; top 120px; it pins from scrollY 626 to 1070), scrolled down
 * until the column has pinned at least 70 css px past its place: the stroke must move with the page, not ride with
 * the column.
 */
async function pinnedColumn(ctx) {
  const g = 'glue';
  await scrollToTop(ctx, ROUTE);
  await scrollTo(ctx, 380);
  const pin = await pinRect();
  if (!pin || pin.top * ctx.scale < 140 || pin.bottom * ctx.scale > ctx.screen.live.h - 40) {
    ctx.report.add(g, 'pinned column: stroke stays on the page', 'error', { measured: `column at ${JSON.stringify(pin)}`, threshold: 'column on screen, not yet pinned' });
    return;
  }
  const s = ctx.scale;
  const y = (pin.top + 95) * s;
  const line = [(pin.left + 60) * s, y, (pin.left + 760) * s, y + 40 * s];
  const natural = pin.doc;
  // Drags of about 60 css px down until the column has pinned 70 px past its place (at most 7), then one back up.
  const down = { frac: (60 * s) / ctx.screen.live.h, ms: SLOW };
  const steps = (rows) => {
    const last = rows.at(-1);
    if (last?.step.startsWith('up')) return null;
    if (rows.length >= 7 || (last?.pinnedBy ?? 0) >= 70) return { frac: -down.frac, ms: SLOW };
    return down;
  };
  const rows = await strokeThrough(ctx, 'pinned column', line, steps, async () => {
    const now = await pinRect();
    return { pinnedBy: now ? +(now.doc - natural).toFixed(1) : null };
  });
  if (!rows.length) return;
  const most = rows.reduce((m, r) => Math.max(m, r.pinnedBy ?? 0), 0);
  ctx.report.check(g, 'pinned column: the column pinned while the stroke was in view', most >= 60, {
    measured: `column held up to ${most} css px below its place`,
    threshold: '>= 60 css px (else the stroke check above proves nothing about sticky columns)',
  });
}

/**
 * A stroke that scrolls into view is drawn whole while the page still moves (owner: the drawings "overlap move and
 * break"). The ink layer shows its last render through the moving camera; before, every background render finished
 * for a view the scroll had left and was dropped, so a stroke that came into view mid-scroll stayed cut at the old
 * screen edge (or missing) until the scroll stopped. Screenshots taken mid-drag must show all of it.
 */
async function entersWhole(ctx) {
  const g = 'glue';
  const name = 'stroke scrolling into view: drawn whole mid-scroll';
  await scrollToTop(ctx, ROUTE);
  await scrollTo(ctx, 300);
  const { w, live } = { w: ctx.screen.w, live: ctx.screen.live };
  const y = live.h * 0.45;
  const line = [w * 0.35, y, w * 0.6, y + live.h * 0.12];
  const stroke = await drawStroke(ctx, { n: ctx.strokeNo, line });
  const points = stroke.dark ?? [];
  const sy0 = await evalLive('scrollY', ROUTE);
  await scroll(ctx, { frac: 0.62, ms: SLOW });
  const sy1 = await settle(ROUTE);
  const gone = line[3] - (sy1 - sy0) * ctx.scale < 90 * ctx.scale;
  if (points.length < MIN_STROKE_PX || !gone) {
    ctx.report.add(g, name, 'error', { measured: `${points.length} stroke px; off screen after the scroll: ${gone}`, threshold: `>= ${MIN_STROKE_PX} px, scrolled off the top first` });
    return;
  }
  const drag = scroll(ctx, { frac: -0.75, ms: 3200 });
  await adb.sleep(1300);
  const rows = [];
  for (let i = 0; i < 2; i++) {
    const before = await evalLive('scrollY', ROUTE);
    const img = await shot(ctx, `glue-enters-${i + 1}`);
    const after = await evalLive('scrollY', ROUTE);
    const lo = Math.round(90 * ctx.scale - Math.min(line[1], line[3]));
    const hi = Math.round(live.h - Math.max(line[1], line[3]));
    const found = shiftIn(img, points, { lo, hi, y0: Math.round(90 * ctx.scale), y1: live.h, near: 0 });
    rows.push({ scrollY: [+before.toFixed(1), +after.toFixed(1)], moving: before !== after, at: found.dy, score: +found.score.toFixed(2) });
  }
  await drag;
  await settle(ROUTE);
  const judged = rows.filter((r) => r.moving);
  const worst = judged.reduce((m, r) => Math.min(m, r.score), 1);
  ctx.report.check(g, name, judged.length >= 1 && worst >= 0.7, {
    measured: `${judged.length} mid-scroll shot(s), worst ${(worst * 100).toFixed(0)}% of ${points.length} stroke px drawn`,
    threshold: '>= 70% of the stroke in every shot taken while scrollY changed, >= 1 such shot',
    evidence: JSON.stringify(rows),
  });
}

export default async function glueSteps(ctx) {
  ctx.scale = ctx.screen.live.w / 1440;
  await longPage(ctx);
  await pinnedColumn(ctx);
  await entersWhole(ctx);
  await scrollToTop(ctx, ROUTE);
  await settle(ROUTE);
}
