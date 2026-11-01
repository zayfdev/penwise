// The PC server's HTTP API (token read from the data dir, never printed) and the dedicated QA project.
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

export const ROOT = resolve(new URL('../..', import.meta.url).pathname);
export const QA_DIR = join(ROOT, 'target/device-qa');
export const SITES = join(QA_DIR, 'sites');
export const PROJECT_ID = 'prj_deviceqa';
export const BASE = process.env.PENWISE_URL || 'http://127.0.0.1:7420';
export const PORT = Number(new URL(BASE).port || 7420);

let token;
async function tokenValue() {
  token ??= (process.env.PENWISE_TOKEN || (await readFile(join(homedir(), '.local/share/penwise/token'), 'utf8'))).trim();
  return token;
}

/** JSON request; `auth` adds the token (needed outside the project route group). Throws on non-2xx. */
export async function api(method, path, body, { auth = false, raw = false } = {}) {
  const headers = { 'content-type': 'application/json' };
  if (auth) headers['x-penwise-token'] = await tokenValue();
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  const text = await res.text();
  if (!res.ok) throw Object.assign(new Error(`${method} ${path}: HTTP ${res.status} ${text.slice(0, 200)}`), { status: res.status });
  if (raw) return text;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export async function serverUp() {
  try {
    await api('GET', '/projects');
    return true;
  } catch {
    return false;
  }
}

export async function waitServer(timeoutMs = 60000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await serverUp()) return true;
    await new Promise((r) => setTimeout(r, 500));
  }
  return false;
}

/**
 * Copies the fixtures into target/device-qa/sites (a scratch copy; fixtures/ is never written). It overwrites in
 * place and never deletes the folder: the server's file watcher holds the folder, and a deleted-and-recreated
 * folder would silently stop the reload notifications.
 */
export async function prepareSites() {
  await mkdir(SITES, { recursive: true });
  await cp(join(ROOT, 'fixtures/sites'), SITES, { recursive: true, force: true });
}

/** Registers (idempotent by root) the QA project over the fixture copy; returns the project row. */
export async function ensureProject() {
  const project = await api('POST', '/projects', { id: PROJECT_ID, name: 'device-qa', root: SITES, static: SITES });
  await writeFile(join(QA_DIR, 'project-id'), `${project.id}\n`);
  return project;
}

export const projects = () => api('GET', '/projects');
export const still = (frame) => api('GET', `/frames/${frame}/still`);
export const rounds = () => api('GET', `/projects/${PROJECT_ID}/rounds`);

/** The doc height the server's still reports; waits while the server renders it for the first time. */
export async function docHeight(frame, timeoutMs = 90000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    try {
      const s = await still(frame);
      if (s?.doc) return { width: s.doc[0], height: s.doc[1], viewport: s.viewport };
    } catch {}
    await new Promise((r) => setTimeout(r, 1000));
  }
  return null;
}

export async function fileExists(p) {
  return existsSync(p);
}
