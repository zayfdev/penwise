// Chrome DevTools Protocol client for the app's WebView (adb forward + Node's global WebSocket).
import { PKG, pid, run, sh } from './adb.mjs';

const PORT = 9333;

/** Forwards the app's WebView DevTools socket; null when the app is not running. */
export async function forward() {
  const p = await pid();
  if (!p) return null;
  await run(['forward', `tcp:${PORT}`, `localabstract:webview_devtools_remote_${p}`]);
  return PORT;
}

/** The page targets of the app's WebViews: [{id, url, title, view:{width,height,screenX,screenY,visible,...}, ws}]. */
export async function targets() {
  if (!(await forward())) return [];
  let list;
  try {
    const res = await fetch(`http://127.0.0.1:${PORT}/json`, { signal: AbortSignal.timeout(5000) });
    list = await res.json();
  } catch {
    return [];
  }
  return list
    .filter((t) => t.type === 'page')
    .map((t) => {
      let view = {};
      try {
        view = JSON.parse(t.description || '{}');
      } catch {}
      return { id: t.id, url: t.url, title: t.title, view, ws: t.webSocketDebuggerUrl };
    });
}

/** The visible, attached page target (the live frame), optionally matching a url fragment. */
export async function livePage(match) {
  const all = await targets();
  return all.find((t) => t.view.attached && t.view.visible !== false && (!match || t.url.includes(match))) ?? null;
}

export async function waitForPage(match, timeoutMs = 45000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    const t = await livePage(match);
    if (t) return t;
    await new Promise((r) => setTimeout(r, 800));
  }
  return null;
}

export class Cdp {
  static async open(target) {
    const cdp = new Cdp();
    cdp.ws = new WebSocket(target.ws);
    cdp.next = 1;
    cdp.waiting = new Map();
    await new Promise((resolve, reject) => {
      cdp.ws.addEventListener('open', resolve, { once: true });
      cdp.ws.addEventListener('error', () => reject(new Error('cdp connect failed')), { once: true });
    });
    cdp.ws.addEventListener('message', (ev) => {
      const msg = JSON.parse(ev.data);
      cdp.waiting.get(msg.id)?.(msg);
      cdp.waiting.delete(msg.id);
    });
    return cdp;
  }

  send(method, params = {}) {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`cdp ${method} timed out`)), 15000);
      this.waiting.set(id, (msg) => {
        clearTimeout(timer);
        msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async eval(expression) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(`page error: ${r.exceptionDetails.text}`);
    return r.result.value;
  }

  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

const METRICS = `(() => { const v = window.visualViewport; const d = document.documentElement; return {
  innerWidth: innerWidth, innerHeight: innerHeight, scrollY: Math.round(scrollY * 100) / 100,
  scrollHeight: d.scrollHeight, vvHeight: v.height, vvOffsetTop: v.offsetTop, vvScale: v.scale,
  vvOffsetLeft: v.offsetLeft, vvWidth: v.width, url: location.href }; })()`;

/** One metrics sample from the live page: viewport, scroll, visual viewport, plus the WebView's view box. */
export async function sample(match) {
  const t = await livePage(match);
  if (!t) return null;
  const cdp = await Cdp.open(t);
  try {
    return { ...(await cdp.eval(METRICS)), view: t.view };
  } finally {
    cdp.close();
  }
}

/** Evaluates an expression in the live page. */
export async function evalLive(expression, match) {
  const t = await livePage(match);
  if (!t) return null;
  const cdp = await Cdp.open(t);
  try {
    return await cdp.eval(expression);
  } finally {
    cdp.close();
  }
}

/** Polls the live page every `everyMs` while something else runs (a swipe); stop() returns the samples. */
export class Sampler {
  constructor(match, everyMs = 80) {
    this.match = match;
    this.everyMs = everyMs;
    this.samples = [];
    this.running = false;
  }

  start() {
    this.running = true;
    this.loop = (async () => {
      let cdp = null;
      let view = {};
      while (this.running) {
        try {
          if (!cdp) {
            const t = await livePage(this.match);
            if (!t) {
              await new Promise((r) => setTimeout(r, 200));
              continue;
            }
            view = t.view;
            cdp = await Cdp.open(t);
          }
          this.samples.push({ ...(await cdp.eval(METRICS)), view, at: Date.now() });
        } catch {
          cdp?.close();
          cdp = null;
        }
        await new Promise((r) => setTimeout(r, this.everyMs));
      }
      cdp?.close();
    })();
    return this;
  }

  async stop() {
    this.running = false;
    await this.loop;
    return this.samples;
  }
}

export { PKG, sh };
