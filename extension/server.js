/**
 * The Penwise server calls, kept free of Chrome APIs so they can run (and be tested) outside the extension.
 *
 * Everything is plain `fetch`, so the same functions work in the service worker, in a popup, or in Node.
 */

/**
 * @typedef {{url: string, token: string}} Settings
 * @typedef {{selector: string, rect: number[], role?: string, text?: string, url?: string}} Anchor
 */

/** A server URL without a trailing slash, and the query token appended when set. */
function endpoint(settings, path) {
  const base = settings.url.replace(/\/+$/, "");
  const separator = path.includes("?") ? "&" : "?";
  return settings.token ? `${base}${path}${separator}token=${encodeURIComponent(settings.token)}` : `${base}${path}`;
}

/** Throws with the server's message when a call is refused, so the popup can show something useful. */
async function request(settings, path, options = {}) {
  const response = await fetch(endpoint(settings, path), {
    ...options,
    headers: {
      ...(settings.token ? { "X-Penwise-Token": settings.token } : {}),
      ...(options.headers ?? {}),
    },
  });
  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(`${options.method ?? "GET"} ${path} failed: ${response.status} ${detail}`.trim());
  }
  const type = response.headers.get("content-type") ?? "";
  return type.includes("application/json") ? response.json() : response.text();
}

/** Uploads a PNG (a Blob or ArrayBuffer) and returns `{asset, width, height}`. */
export async function uploadPng(settings, png) {
  return request(settings, "/assets", {
    method: "POST",
    headers: { "Content-Type": "image/png" },
    body: png,
  });
}

/** Creates a page sized exactly to the picture, so one page pixel is one screenshot pixel. */
export async function createPage(settings, { title, width, height }) {
  const reply = await request(settings, "/pages", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      title,
      kind: { t: "fixed", width, height },
      background: "blank",
    }),
  });
  return reply.page;
}

/** Places `asset` across a whole page. */
export async function placePicture(settings, page, { asset, x = 0, y = 0, w, h, label }) {
  return request(settings, `/pages/${page}/images`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ asset, x, y, w, h, label, layer: "ink" }),
  });
}

/** Stores the element map for a captured page. */
export async function sendAnchors(settings, page, anchors) {
  return request(settings, `/pages/${page}/anchors`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ anchors }),
  });
}

/** Makes the captured page the one the tablet shows. */
export async function selectPage(settings, page) {
  return request(settings, `/pages/${page}/select`, { method: "POST" });
}

/**
 * Captures a whole visible tab: upload the PNG, create a page exactly its size, place it, select it.
 *
 * @param {Settings} settings
 * @param {{png: ArrayBuffer|Blob, width: number, height: number, url: string, title?: string, anchors?: Anchor[]}} capture
 */
export async function publishCapture(settings, capture) {
  const stored = await uploadPng(settings, capture.png);
  const page = await createPage(settings, {
    title: capture.title ?? capture.url,
    width: capture.width,
    height: capture.height,
  });
  await placePicture(settings, page.id, {
    asset: stored.asset,
    w: capture.width,
    h: capture.height,
    label: capture.url,
  });
  if (capture.anchors?.length) await sendAnchors(settings, page.id, capture.anchors);
  await selectPage(settings, page.id);
  return { page: page.id, asset: stored.asset, anchors: capture.anchors?.length ?? 0 };
}

/**
 * Crops a screenshot of the viewport down to one element.
 *
 * @param {{png: Blob, rect: number[], dpr: number}} crop  `rect` in CSS px of the viewport.
 * @returns {Promise<{png: Blob, width: number, height: number}>}
 */
export async function cropToElement({ png, rect, dpr }) {
  const bitmap = await createImageBitmap(png);
  const [x, y, width, height] = rect;
  const left = Math.max(0, Math.round(x * dpr));
  const top = Math.max(0, Math.round(y * dpr));
  const right = Math.min(bitmap.width, Math.round((x + width) * dpr));
  const bottom = Math.min(bitmap.height, Math.round((y + height) * dpr));
  const cropWidth = Math.max(1, right - left);
  const cropHeight = Math.max(1, bottom - top);
  const canvas = new OffscreenCanvas(cropWidth, cropHeight);
  const context = canvas.getContext("2d");
  context.drawImage(bitmap, left, top, cropWidth, cropHeight, 0, 0, cropWidth, cropHeight);
  bitmap.close();
  return { png: await canvas.convertToBlob({ type: "image/png" }), width: cropWidth, height: cropHeight };
}

/** Reads settings from `chrome.storage.local`, falling back to a local server. */
export async function loadSettings() {
  const stored = await chrome.storage.local.get(["url", "token"]);
  return {
    url: stored.url || "http://127.0.0.1:7420",
    token: stored.token || "",
  };
}

/** Saves settings. */
export async function saveSettings(settings) {
  await chrome.storage.local.set({ url: settings.url.trim(), token: settings.token.trim() });
}
