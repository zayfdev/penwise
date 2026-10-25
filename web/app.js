// Live mirror of the tablet page. Geometry comes from the same Rust engine the tablet uses (wasm), so strokes
// look identical. Pages come from the server's library: the sidebar lists notebooks and pages, and the canvas
// shows the active one with its paper background.

import init, { geometry } from "./pkg/penwise_wasm.js";

const PAGE_WIDTH = 2560;
const PAGE_HEIGHT = 1600;
const DAB_STRIDE = 9;
const BACKGROUND_SPACING = 93.7;
const BACKGROUND_COLORS = { ruled: "#C9D6E8", grid: "#DCE3EC", dots: "#C6CEDA" };
const MIN_INFINITE = { width: 1200, height: 900 };
const INFINITE_PADDING = 200;

const canvas = document.getElementById("page");
const status = document.getElementById("status");
const sidebar = document.getElementById("sidebar");
const library = document.getElementById("library");
const newPageForm = document.getElementById("new-page");
const ctx = canvas.getContext("2d");

/** @type {Array<{json: object, argb: number}>} */
let strokes = [];
/** @type {{version: number, active: string, notebooks: Array<{id: string, title: string, pages: Array<object>}>} | null} */
let meta = null;
let activePage = null;

/** In-progress strokes from the tablet, keyed `${session}:${id}`; dropped when the stroke commits. */
const live = new Map();
const LIVE_REBUILD_MS = 50;
/** Eraser positions relayed from the tablet, shown briefly so an erase sweep is visible live. */
const eraseMarks = [];
const ERASE_MARK_MS = 700;
/** Longest a shape preview waits for the next animation frame. */
const SHAPE_FLUSH_FALLBACK_MS = 50;

await init();
resize();

window.addEventListener("resize", () => {
  resize();
  render();
});

/** Access token from the page URL (`/?token=…`); empty when the server runs without one. */
const TOKEN = new URLSearchParams(location.search).get("token") ?? "";

// ---------------------------------------------------------------- library state

/** Applies an index snapshot from the server and repaints the sidebar. */
function applyMeta(next) {
  meta = next;
  if (!activePage || !pageById(activePage.id)) activePage = pageById(next.active);
  renderSidebar();
  resize();
  render();
}

/** A page record anywhere in the library. */
function pageById(id) {
  if (!meta) return null;
  for (const notebook of meta.notebooks) {
    for (const page of notebook.pages) if (page.id === id) return page;
  }
  return null;
}

const GLYPHS = { blank: "▢", ruled: "☰", grid: "▦", dots: "⋯" };

/** Small inline icons; the sidebar stayed text-heavy otherwise. */
const ICONS = {
  rename:
    '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M4 20h4l10-10-4-4L4 16v4Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="m14 6 4 4" fill="none" stroke="currentColor" stroke-width="1.7"/></svg>',
  remove:
    '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M5 7h14M10 5h4M7 9v10h10V9M10 12v4M14 12v4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>',
  page:
    '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path d="M6 4h9l3 3v13H6z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>',
};

/** Rebuilds the sidebar list, keeping it in sync with `meta`. */
function renderSidebar() {
  library.textContent = "";
  if (!meta) return;
  for (const notebook of meta.notebooks) {
    const heading = document.createElement("div");
    heading.className = "notebook";
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = notebook.title;
    const addHere = document.createElement("button");
    addHere.innerHTML = ICONS.page;
    addHere.title = `New page in ${notebook.title}`;
    addHere.addEventListener("click", () => openNewPageForm(notebook.id));
    const rename = document.createElement("button");
    rename.innerHTML = ICONS.rename;
    rename.title = "Rename notebook";
    rename.addEventListener("click", () => renameNotebook(notebook));
    const remove = document.createElement("button");
    remove.innerHTML = ICONS.remove;
    remove.title = "Delete notebook";
    remove.disabled = notebook.pages.length > 0 || meta.notebooks.length <= 1;
    remove.addEventListener("click", () => deleteNotebook(notebook));
    heading.append(name, addHere, rename, remove);
    library.append(heading);
    for (const page of notebook.pages) {
      const row = document.createElement("div");
      row.className = `page${activePage && page.id === activePage.id ? " active" : ""}`;
      row.dataset.page = page.id;

      const glyph = document.createElement("span");
      glyph.className = "glyph";
      glyph.textContent = page.kind?.t === "infinite" ? "∞" : (GLYPHS[page.background] ?? "▢");
      const title = document.createElement("span");
      title.className = "title";
      title.textContent = page.title;
      row.append(glyph, title);

      const tools = document.createElement("span");
      tools.className = "tools";
      const rename = document.createElement("button");
      rename.innerHTML = ICONS.rename;
      rename.title = "Rename page";
      rename.addEventListener("click", (event) => {
        event.stopPropagation();
        renamePage(page);
      });
      const remove = document.createElement("button");
      remove.innerHTML = ICONS.remove;
      remove.title = "Delete page";
      remove.disabled = totalPages() <= 1;
      remove.addEventListener("click", (event) => {
        event.stopPropagation();
        deletePage(page);
      });
      tools.append(rename, remove);
      row.append(tools);
      row.addEventListener("click", () => selectPage(page.id));
      library.append(row);
    }
  }
}

/** Opens the new-page form, remembered against a notebook so the page lands in it. */
function openNewPageForm(notebookId) {
  newPageForm.querySelector('input[name="notebook"]').value = notebookId;
  newPageForm.classList.add("open");
  newPageForm.querySelector('input[name="title"]').focus();
}

function renameNotebook(notebook) {
  const name = window.prompt("Notebook name", notebook.title);
  if (!name || name === notebook.title) return;
  void mutate(`/notebooks/${notebook.id}`, "PATCH", { title: name });
}

function deleteNotebook(notebook) {
  if (notebook.pages.length > 0) {
    status.textContent = "notebook still has pages";
    return;
  }
  if (!window.confirm(`Delete notebook "${notebook.title}"?`)) return;
  void mutate(`/notebooks/${notebook.id}`, "DELETE");
}

function totalPages() {
  if (!meta) return 0;
  return meta.notebooks.reduce((sum, notebook) => sum + notebook.pages.length, 0);
}

/** Asks the server to make `id` the active page; the server then sends the snapshot. */
function selectPage(id) {
  if (activePage && activePage.id === id) return;
  client.send({ t: "select", page: id });
}

function renamePage(page) {
  const name = window.prompt("Page title", page.title);
  if (!name || name === page.title) return;
  void mutate(`/pages/${page.id}`, "PATCH", { title: name });
}

function deletePage(page) {
  if (!window.confirm(`Delete "${page.title}"?`)) return;
  void mutate(`/pages/${page.id}`, "DELETE");
}

/** Sends one library mutation and applies the index it returns. */
async function mutate(path, method, body) {
  const response = await fetch(`${path}?token=${encodeURIComponent(TOKEN)}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    status.textContent = `library: ${await response.text()}`;
    return;
  }
  const result = await response.json();
  if (result.meta) applyMeta(result.meta);
  return result;
}

newPageForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const data = new FormData(newPageForm);
  const title = String(data.get("title") ?? "").trim();
  const template = String(data.get("template") ?? "blank");
  const notebook = String(data.get("notebook") ?? "");
  const payload = {
    background: template === "infinite" ? "blank" : template,
    kind: template === "infinite" ? { t: "infinite" } : undefined,
    notebook: notebook || undefined,
  };
  if (title) payload.title = title;
  const result = await mutate("/pages", "POST", payload);
  newPageForm.reset();
  newPageForm.classList.remove("open");
  if (result?.page) selectPage(result.page.id);
});

document.getElementById("add-page").addEventListener("click", () => {
  openNewPageForm(activePage ? (meta?.notebooks.find((n) => n.pages.some((p) => p.id === activePage.id))?.id ?? "") : "");
});

const textPanel = document.getElementById("text-panel");
const textBody = document.getElementById("text-body");
const textHint = document.getElementById("text-hint");
const pairPanel = document.getElementById("pair-panel");
const pairCode = document.getElementById("pair-code");

/** Shows (or hides) the recognised text of the active page; reading it can take a while on CPU. */
async function toggleText(refresh = false) {
  if (!textPanel.classList.contains("open")) {
    textPanel.classList.add("open");
    refresh = false;
  } else if (!refresh) {
    textPanel.classList.remove("open");
    return;
  }
  if (!activePage) return;
  textBody.textContent = "reading…";
  textHint.textContent = "the handwriting model runs locally; the first read of a page takes ~30 s";
  const query = `page=${encodeURIComponent(activePage.id)}${refresh ? "&refresh=true" : ""}`;
  const response = await fetch(`/text?${query}&token=${encodeURIComponent(TOKEN)}`);
  if (!response.ok) {
    textBody.textContent = "";
    textHint.textContent = `not read: ${await response.text()}`;
    return;
  }
  const result = await response.json();
  textBody.textContent = result.text?.trim() ? result.text : "(no handwriting recognised)";
  textHint.textContent = result.cached ? "cached" : "just read";
}

document.getElementById("read-text").addEventListener("click", () => void toggleText());
document.getElementById("text-refresh").addEventListener("click", () => void toggleText(true));

/** Shows (or hides) the pairing code; the code is single-use, so a dead one is simply refreshed. */
async function togglePairing() {
  if (pairPanel.classList.contains("open")) {
    pairPanel.classList.remove("open");
    return;
  }
  pairPanel.classList.add("open");
  await loadPairCode("/pair/code");
  // The QR carries the same code; it is refreshed whenever the code is.
  document.getElementById("pair-qr").src = `/pair.png?token=${encodeURIComponent(TOKEN)}`;
}

async function loadPairCode(path, method = "GET") {
  pairCode.textContent = "······";
  const response = await fetch(`${path}?token=${encodeURIComponent(TOKEN)}`, { method });
  if (!response.ok) {
    pairCode.textContent = "unavailable";
    return;
  }
  const result = await response.json();
  pairCode.textContent = result.code ?? "·····";
  document.getElementById("pair-qr").src = `/pair.png?token=${encodeURIComponent(TOKEN)}&t=${Date.now()}`;
}

document.getElementById("pair-device").addEventListener("click", () => void togglePairing());
document.getElementById("pair-new").addEventListener("click", () => void loadPairCode("/pair/code", "POST"));

document.getElementById("add-notebook").addEventListener("click", async () => {
  const title = window.prompt("Notebook name", `Notebook ${(meta?.notebooks.length ?? 0) + 1}`);
  if (!title) return;
  await mutate("/notebooks", "POST", { title });
});

// ---------------------------------------------------------------- page geometry

/** Page origin and size in page px; infinite pages wrap their content. */
function pageExtent() {
  const kind = activePage?.kind ?? { t: "fixed", width: PAGE_WIDTH, height: PAGE_HEIGHT };
  if (kind.t !== "infinite") {
    return { originX: 0, originY: 0, width: kind.width, height: kind.height };
  }
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const stroke of strokes) extendFromJson(stroke.json);
  for (const entry of live.values()) if (entry.json) extendFromJson(entry.json);

  function extendFromJson(json) {
    const points = json?.outline ?? json?.dabs ?? [];
    const stride = json?.outline ? 2 : DAB_STRIDE;
    for (let i = 0; i + 1 < points.length; i += stride) {
      if (points[i] < minX) minX = points[i];
      if (points[i] > maxX) maxX = points[i];
      if (points[i + 1] < minY) minY = points[i + 1];
      if (points[i + 1] > maxY) maxY = points[i + 1];
    }
  }
  if (!Number.isFinite(minX)) return { originX: 0, originY: 0, width: MIN_INFINITE.width, height: MIN_INFINITE.height };
  const originX = minX - INFINITE_PADDING;
  const originY = minY - INFINITE_PADDING;
  return {
    originX,
    originY,
    width: Math.max(maxX - originX + INFINITE_PADDING, MIN_INFINITE.width),
    height: Math.max(maxY - originY + INFINITE_PADDING, MIN_INFINITE.height),
  };
}

/** Fits the page into the space beside the sidebar, keeping its aspect ratio. */
function resize() {
  const extent = pageExtent();
  const availableWidth = window.innerWidth - sidebar.offsetWidth - 32;
  const availableHeight = window.innerHeight - 32;
  const scale = Math.min(availableWidth / extent.width, availableHeight / extent.height);
  const dpr = window.devicePixelRatio || 1;
  canvas.style.width = `${Math.round(extent.width * scale)}px`;
  canvas.style.height = `${Math.round(extent.height * scale)}px`;
  canvas.width = Math.round(extent.width * scale * dpr);
  canvas.height = Math.round(extent.height * scale * dpr);
  ctx.setTransform(scale * dpr, 0, 0, scale * dpr, 0, 0);
  ctx.translate(-extent.originX, -extent.originY);
  canvas.dataset.scale = String(scale * dpr);
}

// ---------------------------------------------------------------- transport

class WebSocketClient {
  start() {
    const url = `ws://${location.host}/ws${TOKEN ? `?token=${encodeURIComponent(TOKEN)}` : ""}`;
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.addEventListener("open", () => this.setState("mirroring", false));
    socket.addEventListener("close", () => {
      this.setState("reconnecting…", true);
      setTimeout(() => this.start(), 1000);
    });
    socket.addEventListener("error", () => this.setState("connection error", true));
    socket.addEventListener("message", (event) => this.onFrame(event.data));
  }

  send(frame) {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(frame));
  }

  setState(text, offline) {
    status.textContent = text;
    status.classList.toggle("offline", offline);
  }

  onFrame(data) {
    let frame;
    try {
      frame = JSON.parse(data);
    } catch {
      return;
    }
    if (frame.t === "meta" && frame.meta) {
      applyMeta(frame.meta);
    } else if (frame.t === "active" && frame.page) {
      const page = pageById(frame.page);
      if (page && page.id !== activePage?.id) {
        activePage = page;
        // The previous page's strokes are not this page's: drop them and ask for the new snapshot.
        strokes = [];
        live.clear();
        eraseMarks.length = 0;
        client.send({ t: "select", page: page.id });
        renderSidebar();
        resize();
        render();
      }
    } else if (frame.t === "page" && Array.isArray(frame.strokes)) {
      if (textPanel.classList.contains("open")) void toggleText();
      const page = pageById(frame.page);
      if (!page) return;
      activePage = page;
      strokes = frame.strokes.map(rebuild).filter(Boolean);
      live.clear();
      renderSidebar();
      resize();
      render();
      this.setState(`mirroring · ${strokes.length} strokes`, false);
    } else if (frame.t === "op" && frame.op && frame.page === activePage?.id) {
      applyOp(frame.op);
      this.setState(`mirroring · ${strokes.length} strokes`, false);
      render();
    } else if (frame.t === "stroke" && frame.stroke && frame.page === activePage?.id) {
      const rebuilt = rebuild(frame.stroke);
      if (rebuilt) strokes.push(rebuilt);
      this.setState(`mirroring · ${strokes.length} strokes`, false);
      render();
    } else if (frame.t === "live" && frame.page === activePage?.id && frame.kind === "erase") {
      eraseMarks.push({ x: frame.x, y: frame.y, r: frame.r ?? 24, at: performance.now() });
      while (eraseMarks.length > 64) eraseMarks.shift();
      render();
    } else if (frame.t === "live" && frame.page === activePage?.id && frame.kind === "shape") {
      accumulateShape(frame);
      render();
    } else if (frame.t === "live" && Array.isArray(frame.samples) && frame.page === activePage?.id) {
      accumulateLive(frame);
      render();
    } else if (frame.t === "live_end") {
      for (const key of [...live.keys()]) {
        if (key.startsWith(`${frame.session}:`)) live.delete(key);
      }
      render();
    }
  }
}

/** Applies an incremental edit frame without a page resend. */
function applyOp(op) {
  if (op.k === "remove" && Array.isArray(op.seeds)) {
    const gone = new Set(op.seeds);
    strokes = strokes.filter((stroke) => !gone.has(stroke.seed));
  } else if (op.k === "clear") {
    strokes = [];
    live.clear();
  } else if (op.k === "replace" && Array.isArray(op.strokes)) {
    const gone = new Set(op.seeds ?? []);
    strokes = strokes.filter((stroke) => !gone.has(stroke.seed));
    for (const wire of op.strokes) {
      const rebuilt = rebuild(wire);
      if (rebuilt) strokes.push(rebuilt);
    }
  } else if (op.k === "add" && op.stroke) {
    const rebuilt = rebuild(op.stroke);
    if (rebuilt) strokes.push(rebuilt);
  }
}

/**
 * A shape drag replaces its whole preview each frame; rebuilds are coalesced to at most one per tick.
 *
 * `requestAnimationFrame` alone is not enough: browsers suppress it in a hidden tab, which would freeze the
 * preview of a page nobody is looking at until the tab is visible again. A timer backs it up.
 */
let shapeFlushScheduled = false;
let shapeFlushTimer = 0;

function accumulateShape(frame) {
  const key = `${frame.session}:${frame.id}`;
  live.set(key, { ...frame, key, json: null, pending: true });
  if (shapeFlushScheduled) return;
  shapeFlushScheduled = true;
  requestAnimationFrame(flushShapePreviews);
  shapeFlushTimer = setTimeout(flushShapePreviews, SHAPE_FLUSH_FALLBACK_MS);
}

function flushShapePreviews() {
  if (!shapeFlushScheduled) return;
  shapeFlushScheduled = false;
  clearTimeout(shapeFlushTimer);
  for (const entry of live.values()) {
    if (!entry.pending) continue;
    entry.pending = false;
    entry.json = buildJson(entry.preset, entry.seed, entry.argb, entry.scale, entry.samples, entry.times);
    if (!entry.json) live.delete(entry.key);
  }
  render();
}

/** Rebuilds one stroke's geometry through the wasm engine; malformed strokes are dropped. */
function rebuild(wire) {
  const json = buildJson(wire.preset, wire.seed, wire.argb, wire.scale, wire.samples, wire.times);
  return json ? { json, argb: wire.argb, seed: wire.seed } : null;
}

/** Runs a stroke through the engine; returns parsed geometry or null when it is not renderable. */
function buildJson(preset, seed, argb, scale, samples, times) {
  try {
    return JSON.parse(geometry(preset, BigInt(seed), argb, scale, new Float32Array(samples), new Int32Array(times)));
  } catch {
    return null;
  }
}

/** Appends new samples to a live preview and rebuilds its geometry at most every LIVE_REBUILD_MS. */
function accumulateLive(frame) {
  const key = `${frame.session}:${frame.id}`;
  let entry = live.get(key);
  if (!entry) {
    if (!frame.first) return; // missed the opening frame: wait for the next stroke
    entry = {
      preset: frame.preset,
      argb: frame.argb,
      scale: frame.scale,
      seed: frame.seed,
      samples: [],
      times: [],
      json: null,
      builtAt: 0,
    };
    live.set(key, entry);
  }
  entry.samples.push(...frame.samples);
  entry.times.push(...frame.times);
  const now = performance.now();
  if (now - entry.builtAt < LIVE_REBUILD_MS) return;
  entry.builtAt = now;
  entry.json = buildJson(entry.preset, entry.seed, entry.argb, entry.scale, entry.samples, entry.times);
  if (!entry.json) live.delete(key);
}

// ---------------------------------------------------------------- drawing

function rgba(argb, alpha) {
  const a = ((argb >>> 24) & 255) / 255;
  const r = (argb >>> 16) & 255;
  const g = (argb >>> 8) & 255;
  const b = argb & 255;
  return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a * alpha))})`;
}

/** Horizontal rules, a square grid or a dot grid, in page pixels. */
function drawBackground(extent, background) {
  const color = BACKGROUND_COLORS[background];
  if (!color) return;
  ctx.fillStyle = color;
  if (background === "ruled" || background === "grid") {
    for (let y = extent.originY + BACKGROUND_SPACING; y < extent.originY + extent.height; y += BACKGROUND_SPACING) {
      ctx.fillRect(extent.originX, y, extent.width, 1);
    }
  }
  if (background === "grid" || background === "dots") {
    for (let x = extent.originX + BACKGROUND_SPACING; x < extent.originX + extent.width; x += BACKGROUND_SPACING) {
      if (background === "grid") ctx.fillRect(x, extent.originY, 1, extent.height);
      else {
        for (let y = extent.originY + BACKGROUND_SPACING; y < extent.originY + extent.height; y += BACKGROUND_SPACING) {
          ctx.beginPath();
          ctx.arc(x, y, 1.5, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }
  }
}

function render() {
  const extent = pageExtent();
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.restore();

  ctx.fillStyle = "#fff";
  ctx.fillRect(extent.originX, extent.originY, extent.width, extent.height);
  drawBackground(extent, activePage?.background ?? "blank");

  // Under-ink band first: multiply strokes (highlighter) then everything else.
  for (const pass of [true, false]) {
    for (const stroke of strokes) {
      if (Boolean(stroke.json.multiply) === pass) drawStroke(stroke);
    }
  }
  for (const entry of live.values()) {
    if (entry.json) drawStroke({ json: entry.json, argb: entry.argb });
  }
  drawEraseMarks();
}

/** The eraser sweep, fading out so it reads as a gesture rather than ink. */
function drawEraseMarks() {
  const now = performance.now();
  while (eraseMarks.length > 0 && now - eraseMarks[0].at > ERASE_MARK_MS) eraseMarks.shift();
  for (const mark of eraseMarks) {
    const age = (now - mark.at) / ERASE_MARK_MS;
    ctx.beginPath();
    ctx.arc(mark.x, mark.y, mark.r, 0, Math.PI * 2);
    ctx.fillStyle = `rgba(80, 90, 110, ${0.18 * (1 - age)})`;
    ctx.fill();
    ctx.strokeStyle = `rgba(60, 70, 90, ${0.35 * (1 - age)})`;
    ctx.lineWidth = 1;
    ctx.stroke();
  }
  if (eraseMarks.length > 0) requestAnimationFrame(render);
}

function drawStroke({ json, argb }) {
  ctx.globalCompositeOperation = json.multiply ? "multiply" : "source-over";
  if (json.stamps) drawDabs(json, argb);
  else drawOutline(json, argb);
  ctx.globalCompositeOperation = "source-over";
}

function drawOutline(json, argb) {
  const points = json.outline;
  if (points.length < 2) return;
  const path = new Path2D();
  let start = 0;
  const ends = json.ends.length > 0 ? json.ends : [json.points];
  for (const end of ends) {
    if (end > start && end * 2 <= points.length) {
      path.moveTo(points[start * 2], points[start * 2 + 1]);
      for (let i = start + 1; i < end; i++) path.lineTo(points[i * 2], points[i * 2 + 1]);
      path.closePath();
    }
    start = end;
  }
  ctx.fillStyle = rgba(argb, json.opacity);
  ctx.fill(path, "nonzero");
}

function drawDabs(json, argb) {
  const dabs = json.dabs;
  for (let o = 0; o + DAB_STRIDE <= dabs.length; o += DAB_STRIDE) {
    const [x, y, rx, ry, angle, opacity, hardness] = dabs.slice(o, o + DAB_STRIDE);
    if (!(rx > 0) || !(ry > 0)) continue;
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.scale(rx, ry);
    const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, 1);
    const color = rgba(argb, opacity * json.opacity);
    gradient.addColorStop(0, color);
    gradient.addColorStop(Math.max(0, Math.min(0.999, hardness)), color);
    gradient.addColorStop(1, rgba(argb, 0));
    ctx.fillStyle = gradient;
    ctx.beginPath();
    ctx.arc(0, 0, 1, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }
}

// Read-only handle for tests and debugging (`window.__penwise.live.size`).
window.__penwise = {
  get strokes() {
    return strokes.length;
  },
  get previews() {
    return [...live.values()].filter((entry) => entry.json).length;
  },
  get debug() {
    return [...live.values()].map((entry) => ({
      kind: entry.kind ?? "stroke",
      built: Boolean(entry.json),
      pending: Boolean(entry.pending),
      samples: entry.samples ? entry.samples.length : 0,
    }));
  },
};

// Startup last: class declarations are not initialised before this point (TDZ).
const client = new WebSocketClient();
client.start();
