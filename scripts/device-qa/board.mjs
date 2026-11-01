// The QA board's clean state: every run rolls the board back to the seq of its creation (compensating ops, so it is
// an ordinary logged change) and closes any round a killed run left sent. Nothing else of the owner's data is touched.
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { PROJECT_ID, QA_DIR, api, projects } from './server.mjs';

const BASE_FILE = join(QA_DIR, 'board-base-seq.json');

export async function boardPage() {
  const list = await projects();
  const project = list.find((p) => p.id === PROJECT_ID);
  const pages = await api('GET', '/pages', undefined, { auth: true });
  const notebook = pages.notebooks.find((n) => n.id === project?.notebook);
  return notebook?.pages[0]?.id ?? null;
}

async function baseSeq(page) {
  try {
    const saved = JSON.parse(await readFile(BASE_FILE, 'utf8'));
    if (saved.page === page) return saved.seq;
  } catch {}
  const h = await api('GET', `/pages/${page}/history?limit=500`, undefined, { auth: true });
  const oldest = h.batches.at(-1);
  if (!oldest) return null;
  await writeFile(BASE_FILE, JSON.stringify({ page, seq: oldest.last_seq }));
  return oldest.last_seq;
}

export async function prepareBoard(ctx) {
  const page = await boardPage();
  ctx.boardPage = page;
  if (!page) return ctx.report.note('QA board does not exist yet; the first frame creates it');
  const seq = await baseSeq(page);
  if (seq === null) return;
  try {
    const res = await api('POST', `/pages/${page}/restore?to_seq=${seq}`, undefined, { auth: true });
    ctx.report.note(`board rolled back to seq ${seq}${res.skipped?.length ? ` (${res.skipped.length} items could not be rolled back)` : ''}`);
  } catch (error) {
    ctx.report.note(`board rollback failed (${error.message}); marks from earlier runs may remain`);
  }
  const list = await api('GET', `/projects/${PROJECT_ID}/rounds`).catch(() => []);
  for (const r of Array.isArray(list) ? list : (list.rounds ?? [])) {
    if (r.status === 'sent') await api('POST', `/projects/${PROJECT_ID}/rounds/${r.n}/resolve`).catch(() => {});
  }
}
