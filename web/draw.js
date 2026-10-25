// Pen drawing in the browser: the same page, the same engine (wasm), the same wire protocol as the tablet.
//
// Pointer Events give pressure and tilt on Apple Pencil, Surface Pen and most USI pens; a mouse or finger draws
// too, at constant pressure. Strokes go out as ordinary `stroke` frames, so the tablet, the mirror and every
// agent see them exactly as if the tablet had drawn them.

import init, { geometry } from "./pkg/penwise_wasm.js";

const PAGE_WIDTH = 2560;
const PAGE_HEIGHT = 1600;
const TOKEN = new URLSearchParams(location.search).get("token") ?? "";
const MAX_SAMPLES_PER_STROKE = 4096;
/** The stroke is committed even if the browser never sends pointerup (tab hidden, pen lost). */
const IDLE_COMMIT_MS = 4000;

const canvas = document.getElementById("page");
const status = document.getElementById("status");
const ctx = canvas.getContext("2d");

let meta = null;
let activePage = null;
/** Committed strokes of the page: `{seed, preset, argb, scale, samples, times, json, argbInt}`. */
let strokes = [];
let images = [];
let view = { scale: 1, offsetX: 0, offsetY: 0 };
let dpr = 1;

// `argb` must be a *signed* 32-bit int on the wire: the server stores it as i32, and 0xFF… is out of range.
const tool = { preset: 0, argb: 0xff111827 | 0, size: 4 };
let drawing = null;
let idleTimer = 0;

await init();
resize();
window.addEventListener("resize", () => {
  resize();
  render();
});

function resize() {
  dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(innerWidth * dpr);
  canvas.height = Math.floor(innerHeight * dpr);
  // Fit the page into the window, with a margin, so the whole sheet is visible.
  const pageWidth = activePage?.kind?.t === "fixed" ? activePage.kind.width : PAGE_WIDTH;
  const pageHeight = activePage?.kind?.t === "fixed" ? activePage.kind.height : PAGE_HEIGHT;
  const scale = Math.min(innerWidth / pageWidth, innerHeight / pageHeight, 1) * 0.94;
  view = {
    scale,
    offsetX: (innerWidth - pageWidth * scale) / 2,
    offsetY: (innerHeight - pageHeight * scale) / 2,
  };
  applyTransform();
}

/** Canvas coordinates: draw in page px, with the page centred. */
function applyTransform() {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

function toPage(event) {
  return {
    x: (event.clientX - view.offsetX) / view.scale,
    y: (event.clientY - view.offsetY) / view.scale,
  };
}

/** Pressure from a Pointer Event; pens report 0–1, a mouse reports 0.5 while down. */
function pressureOf(event) {
  if (event.pointerType === "pen") return event.pressure > 0 ? event.pressure : 0.5;
  return 0.5;
}

/** Tilt in radians from the pen axis, and its direction, from `tiltX`/`tiltY`. */
function tiltOf(event) {
  const tiltX = ((event.tiltX ?? 0) * Math.PI) / 180;
  const tiltY = ((event.tiltY ?? 0) * Math.PI) / 180;
  const tilt = Math.min(Math.PI / 2, Math.hypot(tiltX, tiltY));
  const azimuth = tilt === 0 ? 0 : Math.atan2(tiltY, tiltX) || 0;
  return { tilt, azimuth };
}

function buildJson(preset, seed, argb, scale, samples, times) {
  try {
    return JSON.parse(geometry(preset, BigInt(seed), argb, scale, new Float32Array(samples), new Int32Array(times)));
  } catch {
    return null;
  }
}

let seedCounter = 0n;

function newSeed() {
  // Unique per client: millisecond clock plus a counter, still a decimal string on the wire.
  const base = BigInt(Date.now()) * 1000n + (seedCounter++ % 1000n);
  return (base % (2n ** 63n)).toString();
}

function startStroke(event) {
  if (drawing) return;
  try {
    canvas.setPointerCapture(event.pointerId);
  } catch {
    // Synthetic or already-released pointers cannot be captured; drawing still works without it.
  }
  const page = toPage(event);
  const { tilt, azimuth } = tiltOf(event);
  drawing = {
    pointerId: event.pointerId,
    seed: newSeed(),
    preset: tool.preset,
    argb: tool.argb | 0,
    scale: tool.size / 4,
    samples: [page.x, page.y, pressureOf(event), tilt, azimuth],
    times: [0],
    started: performance.now(),
    json: null,
    builtAt: 0,
  };
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => commitStroke(), IDLE_COMMIT_MS);
  event.preventDefault();
}

function moveStroke(event) {
  if (!drawing || event.pointerId !== drawing.pointerId) return;
  // Coalesced events carry the samples the browser batched between frames: the difference between a smooth
  // line and a chain of straight segments.
  const coalesced = event.getCoalescedEvents?.() ?? [];
  const points = coalesced.length > 0 ? coalesced : [event];
  for (const point of points) {
    if (drawing.times.length >= MAX_SAMPLES_PER_STROKE) break;
    const page = toPage(point);
    const { tilt, azimuth } = tiltOf(point);
    const elapsed = Math.round((performance.now() - drawing.started) * 1000);
    drawing.samples.push(page.x, page.y, pressureOf(point), tilt, azimuth);
    drawing.times.push(elapsed);
  }
  rebuildPreview();
  render();
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => commitStroke(), IDLE_COMMIT_MS);
  event.preventDefault();
}

/** Rebuilds the in-progress geometry, at most once per display frame. */
function rebuildPreview() {
  const now = performance.now();
  if (now - drawing.builtAt < 16) return;
  drawing.builtAt = now;
  drawing.json = buildJson(drawing.preset, drawing.seed, drawing.argb, drawing.scale, drawing.samples, drawing.times);
}

let lastSent = null;

function commitStroke(event) {
  clearTimeout(idleTimer);
  if (!drawing) return;
  if (event && event.pointerId !== undefined && event.pointerId !== drawing.pointerId) return;
  const finished = drawing;
  drawing = null;
  if (finished.times.length < 2) {
    render();
    return;
  }
  const json = buildJson(finished.preset, finished.seed, finished.argb, finished.scale, finished.samples, finished.times);
  const stroke = {
    seed: finished.seed,
    preset: finished.preset,
    argb: finished.argb,
    scale: finished.scale,
    samples: finished.samples,
    times: finished.times,
    json,
  };
  strokes.push(stroke);
  lastSent = { seed: stroke.seed, samples: stroke.samples.length, times: stroke.times.length, raw: null };
  const frame = {
    t: "stroke",
    page: activePage?.id,
    stroke: {
      layer: "ink",
      preset: stroke.preset,
      argb: stroke.argb,
      scale: stroke.scale,
      seed: stroke.seed,
      samples: stroke.samples,
      times: stroke.times,
    },
  };
  lastSent.raw = JSON.stringify(frame);
  socket.send(frame);
  render();
  event?.preventDefault?.();
}

// ---------------------------------------------------------------- wire

const socket = {
  ws: null,
  send(frame) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(frame));
  },
  start() {
    const ws = new WebSocket(`ws://${location.host}/ws${TOKEN ? `?token=${encodeURIComponent(TOKEN)}` : ""}`);
    this.ws = ws;
    ws.addEventListener("open", () => {
      setStatus(`connected · ${activePage?.title ?? ""}`, false);
      if (activePage) ws.send(JSON.stringify({ t: "select", page: activePage.id }));
    });
    ws.addEventListener("close", () => {
      setStatus("reconnecting…", true);
      setTimeout(() => this.start(), 1000);
    });
    ws.addEventListener("message", (event) => onFrame(event.data));
  },
};

function setStatus(text, offline) {
  status.textContent = text;
  status.classList.toggle("offline", offline);
}

function pageById(id) {
  for (const notebook of meta?.notebooks ?? []) {
    for (const page of notebook.pages) if (page.id === id) return page;
  }
  return null;
}

function onFrame(text) {
  let frame;
  try {
    frame = JSON.parse(text);
  } catch {
    return;
  }
  if (frame.t === "meta" && frame.meta) {
    meta = frame.meta;
  } else if (frame.t === "page" && frame.page === activePage?.id) {
    strokes = (frame.strokes ?? []).map(fromWire).filter(Boolean);
    images = frame.images ?? [];
    for (const image of images) imageFor(image.asset);
    render();
  } else if (frame.t === "image" && frame.page === activePage?.id) {
    images = [...images.filter((image) => image.id !== frame.image.id), frame.image];
    imageFor(frame.image.asset);
    render();
  } else if (frame.t === "stroke" && frame.page === activePage?.id) {
    const stroke = fromWire(frame.stroke);
    if (stroke && !strokes.some((existing) => existing.seed === stroke.seed)) strokes.push(stroke);
    render();
  } else if (frame.t === "op" && frame.page === activePage?.id) {
    applyOp(frame.op);
    render();
  } else if (frame.t === "error") {
    setStatus(`server: ${frame.error}`, true);
    console.warn("penwise server error", frame);
  } else if (frame.t === "active" && frame.page) {
    const page = pageById(frame.page);
    if (page && page.id !== activePage?.id) {
      activePage = page;
      strokes = [];
      images = [];
      socket.send({ t: "select", page: page.id });
      resize();
    }
  }
}

function fromWire(wire) {
  const json = buildJson(wire.preset, wire.seed, wire.argb, wire.scale, wire.samples, wire.times);
  return json
    ? { ...wire, json }
    : null;
}

function applyOp(op) {
  if (op.k === "remove") {
    strokes = strokes.filter((stroke) => !op.seeds.includes(stroke.seed));
  } else if (op.k === "clear") {
    strokes = [];
    images = [];
  } else if (op.k === "replace") {
    strokes = strokes.filter((stroke) => !(op.seeds ?? []).includes(stroke.seed));
    for (const wire of op.strokes ?? []) {
      const stroke = fromWire(wire);
      if (stroke) strokes.push(stroke);
    }
  }
}

// ---------------------------------------------------------------- drawing

const imageCache = new Map();

function imageFor(asset) {
  let entry = imageCache.get(asset);
  if (!entry) {
    entry = new Image();
    entry.decoding = "async";
    entry.addEventListener("load", () => render());
    entry.src = `/assets/${asset}?token=${encodeURIComponent(TOKEN)}`;
    imageCache.set(asset, entry);
  }
  return entry;
}

function render() {
  const pageWidth = activePage?.kind?.t === "fixed" ? activePage.kind.width : PAGE_WIDTH;
  const pageHeight = activePage?.kind?.t === "fixed" ? activePage.kind.height : PAGE_HEIGHT;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#e9ecf1";
  ctx.fillRect(0, 0, innerWidth, innerHeight);
  ctx.save();
  ctx.translate(view.offsetX, view.offsetY);
  ctx.scale(view.scale, view.scale);
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, pageWidth, pageHeight);
  for (const image of images) {
    const entry = imageCache.get(image.asset);
    if (entry?.complete) ctx.drawImage(entry, image.x, image.y, image.w, image.h);
  }
  for (const stroke of strokes) drawStroke(stroke.json, stroke.argb);
  if (drawing?.json) drawStroke(drawing.json, drawing.argb);
  ctx.restore();
}

function rgba(argb, alpha) {
  const a = ((argb >>> 24) & 255) / 255;
  const r = (argb >>> 16) & 255;
  const g = (argb >>> 8) & 255;
  const b = argb & 255;
  return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a * alpha))})`;
}

/** Outline brushes paint a filled polygon; stamp brushes paint dabs (same shapes the tablet renders). */
function drawStroke(json, argb) {
  if (!json) return;
  ctx.fillStyle = rgba(argb, 1);
  if (json.outline) {
    const points = json.outline;
    ctx.beginPath();
    ctx.moveTo(points[0], points[1]);
    for (let index = 2; index + 1 < points.length; index += 2) ctx.lineTo(points[index], points[index + 1]);
    ctx.closePath();
    ctx.fill();
    return;
  }
  const dabs = json.dabs ?? [];
  for (let index = 0; index + 8 < dabs.length; index += 9) {
    const [x, y, radius, alpha] = dabs.slice(index, index + 4);
    if (radius <= 0) continue;
    ctx.fillStyle = rgba(argb, alpha);
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    ctx.fill();
  }
}

// ---------------------------------------------------------------- input

canvas.addEventListener("pointerdown", startStroke);
canvas.addEventListener("pointermove", moveStroke);
canvas.addEventListener("pointerup", commitStroke);
canvas.addEventListener("pointercancel", commitStroke);
canvas.addEventListener("pointerleave", commitStroke);

/** Two-finger tap undoes the last stroke this client drew. */
let twoFinger = { at: 0, count: 0 };
canvas.addEventListener("pointerdown", (event) => {
  if (event.pointerType === "touch") twoFinger.count++;
  if (twoFinger.count === 2) {
    undo();
    twoFinger.count = 0;
  }
});
for (const name of ["pointerup", "pointercancel"]) {
  canvas.addEventListener(name, () => {
    twoFinger.count = 0;
  });
}

function undo() {
  const last = strokes.at(-1);
  if (!last) return;
  const batch = strokes.slice(-1).map((stroke) => stroke.seed);
  strokes = strokes.filter((stroke) => !batch.includes(stroke.seed));
  socket.send({ t: "op", page: activePage?.id, op: { k: "remove", seeds: batch } });
  render();
}

document.getElementById("undo").addEventListener("click", undo);
for (const button of document.querySelectorAll("#bar button[data-preset]")) {
  button.addEventListener("click", () => {
    tool.preset = Number(button.dataset.preset);
    for (const other of document.querySelectorAll("#bar button[data-preset]")) {
      other.classList.toggle("on", other === button);
    }
  });
}
document.getElementById("color").addEventListener("input", (event) => {
  tool.argb = (0xff000000 | Number.parseInt(event.target.value.slice(1), 16)) | 0;
});
document.getElementById("size").addEventListener("input", (event) => {
  tool.size = Number(event.target.value);
});

// ---------------------------------------------------------------- boot

/** Fetches the index, picks the active page, then syncs. */
async function boot() {
  const response = await fetch(`/pages?token=${encodeURIComponent(TOKEN)}`);
  meta = await response.json();
  activePage = pageById(meta.active);
  resize();
  socket.start();
  if (activePage) socket.send({ t: "select", page: activePage.id });
}

boot().catch((error) => setStatus(String(error?.message ?? error), true));

window.__penwiseDraw = {
  get strokes() {
    return strokes.length;
  },
  get drawing() {
    return Boolean(drawing);
  },
  get page() {
    return activePage?.id ?? null;
  },
  get socket() {
    return socket.ws?.readyState ?? -1;
  },
  get lastFrame() {
    return lastSent;
  },
  undo,
};
