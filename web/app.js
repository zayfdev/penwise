// Live mirror of the tablet page. Read-only: the server sends the page on connect, then every new stroke.
// Geometry comes from the same Rust engine the tablet uses (wasm), so strokes look identical.

import init, { geometry } from "./pkg/penwise_wasm.js";

const PAGE_WIDTH = 1600;
const PAGE_HEIGHT = 2560;
const DAB_STRIDE = 9;

const canvas = document.getElementById("page");
const status = document.getElementById("status");
const ctx = canvas.getContext("2d");

/** @type {Array<{json: object, argb: number}>} */
let strokes = [];

/** In-progress strokes from the tablet, keyed `${session}:${id}`; dropped when the stroke commits. */
const live = new Map();
const LIVE_REBUILD_MS = 50;

await init();
resize();

window.addEventListener("resize", () => {
  resize();
  render();
});

/** Fits the page into the window, keeping its aspect ratio and wasting no pixels. */
function resize() {
  const scale = Math.min(
    (window.innerWidth - 32) / PAGE_WIDTH,
    (window.innerHeight - 32) / PAGE_HEIGHT,
  );
  const dpr = window.devicePixelRatio || 1;
  canvas.style.width = `${Math.round(PAGE_WIDTH * scale)}px`;
  canvas.style.height = `${Math.round(PAGE_HEIGHT * scale)}px`;
  canvas.width = Math.round(PAGE_WIDTH * scale * dpr);
  canvas.height = Math.round(PAGE_HEIGHT * scale * dpr);
  ctx.setTransform(scale * dpr, 0, 0, scale * dpr, 0, 0);
}

/** Access token from the page URL (`/?token=…`); empty when the server runs without one. */
const TOKEN = new URLSearchParams(location.search).get("token") ?? "";

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
    if (frame.t === "page" && Array.isArray(frame.strokes)) {
      strokes = frame.strokes.map(rebuild);
      this.setState(`mirroring · ${strokes.length} strokes`, false);
      render();
    } else if (frame.t === "stroke" && frame.stroke) {
      strokes.push(rebuild(frame.stroke));
      this.setState(`mirroring · ${strokes.length} strokes`, false);
      render();
    } else if (frame.t === "live" && Array.isArray(frame.samples)) {
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

/** Rebuilds one stroke's geometry through the wasm engine; malformed strokes are dropped. */
function rebuild(wire) {
  const json = buildJson(wire.preset, wire.seed, wire.argb, wire.scale, wire.samples, wire.times);
  return { json, argb: wire.argb };
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
    entry = { preset: frame.preset, argb: frame.argb, scale: frame.scale, seed: frame.seed, samples: [], times: [], json: null, builtAt: 0 };
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

function rgba(argb, alpha) {
  const a = ((argb >>> 24) & 255) / 255;
  const r = (argb >>> 16) & 255;
  const g = (argb >>> 8) & 255;
  const b = argb & 255;
  return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a * alpha))})`;
}

function render() {
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.restore();

  // Under-ink band first: multiply strokes (highlighter) then everything else.
  for (const pass of [true, false]) {
    for (const stroke of strokes) {
      if (Boolean(stroke.json.multiply) === pass) drawStroke(stroke);
    }
  }
  for (const entry of live.values()) {
    if (entry.json) drawStroke({ json: entry.json, argb: entry.argb });
  }
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

// Startup last: class declarations are not initialised before this point (TDZ).
new WebSocketClient().start();
