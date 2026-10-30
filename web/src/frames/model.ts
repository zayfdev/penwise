/**
 * Frames on the canvas (design harness, contracts C1/C2): the frame item shape, the project → iframe URL rule and
 * the page → CSS placement math. Pure — the DOM lives in `layer.ts`, the wasm calls in `app/app.ts`.
 *
 * A frame covers `css_w × css_h` page units at scale 1. Its iframe sits in a wrapper carrying the canvas view as
 * a CSS transform, so the iframe itself keeps its preset size and the site's breakpoints see real CSS px.
 */

import type { Viewport, ViewTransform } from "../view";

/** A size preset of the frame chip (`custom` keeps whatever size the frame has). */
export interface Preset {
  readonly name: string;
  readonly label: string;
  readonly cssW: number;
  readonly cssH: number;
}

/** The fixed presets (design-harness-spec §4, [default] 4). */
export const PRESETS: readonly Preset[] = [
  { name: "phone", label: "Phone", cssW: 390, cssH: 844 },
  { name: "tablet", label: "Tablet", cssW: 834, cssH: 1194 },
  { name: "desktop", label: "Desktop", cssW: 1440, cssH: 900 },
];

/** Choices of a preset switch: the fixed presets plus `custom`. */
export const PRESET_CHOICES: readonly { readonly name: string; readonly label: string }[] = [
  ...PRESETS.map((preset) => ({ name: preset.name, label: preset.label })),
  { name: "custom", label: "Custom" },
];

/** CSS size of a fixed preset (`null` for `custom` or an unknown name). */
export function presetSize(name: string): { readonly cssW: number; readonly cssH: number } | null {
  const preset = PRESETS.find((p) => p.name === name);
  return preset ? { cssW: preset.cssW, cssH: preset.cssH } : null;
}

/** How much of the document a frame's box shows (core `FrameExtent`). */
export type FrameExtent = "viewport" | "full";

/** One live frame item of the shown page. `x`/`y` are the page-unit top-left of the rect. */
export interface Frame {
  readonly id: string;
  readonly project: string;
  readonly route: string;
  readonly preset: string;
  readonly cssW: number;
  readonly cssH: number;
  /** `full`: the box is the whole document (V1/V2 contract §1); `viewport`: the box is the preset size. */
  readonly extent: FrameExtent;
  /** The document height the server last measured (CSS px, `0` = unknown). */
  readonly docH: number;
  /** Box height on the board, as the core computes it from the frame's current layout (`box_size`). */
  readonly boxH: number;
  /** The 32-hex id the server's `/frames/{hex}/…` routes take (the core reports it). */
  readonly hex: string;
  readonly x: number;
  readonly y: number;
  /** P1 patch JSON text applied over the page (ghost/variant); `null` = the page as served. */
  readonly patch: string | null;
  /** Label the owner sees on the frame chip (`""` for a plain frame). */
  readonly label: string;
  /** Variant group id, `null` for a plain frame. */
  readonly group: string | null;
  /** The owner's pick within its group (one chosen frame dims the group's others). */
  readonly chosen: boolean;
}

/** A linked project (`GET /projects`). */
export interface Project {
  readonly id: string;
  readonly name: string;
  readonly root: string;
  readonly kind: string;
  readonly origin: string;
  readonly port: number;
  readonly notebook: string | null;
}

function finite(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

/** The patch as text: the core stores text, a JSON object is stringified so the bridge gets one form. */
function readPatch(value: unknown): string | null {
  if (typeof value === "string") return value === "" ? null : value;
  if (typeof value === "object" && value !== null) return JSON.stringify(value);
  return null;
}

function readFrame(entry: unknown): Frame | null {
  if (typeof entry !== "object" || entry === null) return null;
  const row = entry as Record<string, unknown>;
  if (typeof row.id !== "string" || row.id === "") return null;
  if (typeof row.project !== "string" || row.project === "") return null;
  if (typeof row.route !== "string") return null;
  const rect = row.rect;
  if (!Array.isArray(rect) || rect.length < 4) return null;
  const x = finite(rect[0], Number.NaN);
  const y = finite(rect[1], Number.NaN);
  const rectW = finite(rect[2], Number.NaN);
  const rectH = finite(rect[3], Number.NaN);
  const cssW = finite(row.cssW, rectW);
  const cssH = finite(row.cssH, rectH);
  const extent: FrameExtent = row.extent === "viewport" ? "viewport" : "full";
  const docH = Math.max(0, finite(row.docH, 0));
  const boxH = Math.max(cssH, finite(row.boxH, cssH));
  if (!Number.isFinite(x) || !Number.isFinite(y) || cssW <= 0 || cssH <= 0) return null;
  return {
    id: row.id,
    project: row.project,
    route: row.route,
    preset: typeof row.preset === "string" ? row.preset : "custom",
    cssW,
    cssH,
    extent,
    docH,
    boxH,
    hex: typeof row.hex === "string" ? row.hex : "",
    x,
    y,
    patch: readPatch(row.patch),
    label: typeof row.label === "string" ? row.label : "",
    group: typeof row.group === "string" && row.group !== "" ? row.group : null,
    chosen: row.chosen === true,
  };
}

/** Parses `Viewer.frames` JSON; malformed entries are dropped rather than breaking the page. */
export function parseFrames(json: string): Frame[] {
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return [];
  }
  if (!Array.isArray(value)) return [];
  const frames: Frame[] = [];
  for (const entry of value) {
    const frame = readFrame(entry);
    if (frame !== null) frames.push(frame);
  }
  return frames;
}

/** True when `b` reports the same frame state as `a` (id ignored). */
export function sameFrame(a: Frame, b: Frame): boolean {
  return (
    a.project === b.project &&
    a.route === b.route &&
    a.preset === b.preset &&
    a.cssW === b.cssW &&
    a.cssH === b.cssH &&
    a.extent === b.extent &&
    a.docH === b.docH &&
    a.boxH === b.boxH &&
    a.hex === b.hex &&
    a.x === b.x &&
    a.y === b.y &&
    a.patch === b.patch &&
    a.label === b.label &&
    a.group === b.group &&
    a.chosen === b.chosen
  );
}

/** What changed between two `frames()` readings. */
export interface FrameDiff {
  readonly added: readonly Frame[];
  readonly updated: readonly Frame[];
  readonly removed: readonly string[];
}

/** Diff by id: only frames whose state changed are `updated`, so unchanged iframes are never touched. */
export function diffFrames(previous: readonly Frame[], next: readonly Frame[]): FrameDiff {
  const before = new Map(previous.map((frame) => [frame.id, frame] as const));
  const ids = new Set(next.map((frame) => frame.id));
  const added: Frame[] = [];
  const updated: Frame[] = [];
  for (const frame of next) {
    const old = before.get(frame.id);
    if (old === undefined) added.push(frame);
    else if (!sameFrame(old, frame)) updated.push(frame);
  }
  return {
    added,
    updated,
    removed: previous.filter((frame) => !ids.has(frame.id)).map((frame) => frame.id),
  };
}

/** The frame under a page point (the last in paint order wins), or `null` on bare canvas. */
export function frameAt(frames: readonly Frame[], x: number, y: number): Frame | null {
  for (let i = frames.length - 1; i >= 0; i -= 1) {
    const frame = frames[i];
    if (
      frame !== undefined &&
      x >= frame.x &&
      x < frame.x + frame.cssW &&
      y >= frame.y &&
      y < frame.y + frame.boxH
    ) {
      return frame;
    }
  }
  return null;
}

/** A frame's box on the stage, CSS px. */
export interface Placement {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

/** Where `frame` lands on the stage for `view`: page units scaled, shifted by the view origin. */
export function placeFrame(
  frame: Pick<Frame, "x" | "y" | "cssW" | "boxH">,
  view: ViewTransform,
): Placement {
  return {
    left: (frame.x - view.originX) * view.scale,
    top: (frame.y - view.originY) * view.scale,
    width: frame.cssW * view.scale,
    height: frame.boxH * view.scale,
  };
}

/** Transform of the frames wrapper: page units → stage CSS px (`translate` after `scale`). */
export function frameTransform(view: ViewTransform): string {
  const tx = -view.originX * view.scale;
  const ty = -view.originY * view.scale;
  return `translate(${tx}px, ${ty}px) scale(${view.scale})`;
}

/** True when any of `box` is inside `viewport` (a viewport of unknown size counts as visible). */
export function intersects(box: Placement, viewport: Viewport): boolean {
  if (viewport.width <= 0 || viewport.height <= 0) return true;
  return (
    box.left + box.width > 0 &&
    box.top + box.height > 0 &&
    box.left < viewport.width &&
    box.top < viewport.height
  );
}

/**
 * Whether frame iframes take mouse/touch input. A pen draws over them, "Draw with mouse" makes the mouse ink,
 * and Ctrl/Meta must reach the stage to zoom the canvas instead.
 */
export function framesInteractive(
  lastPointer: string,
  drawMouse: boolean,
  modifier: boolean,
): boolean {
  return !drawMouse && !modifier && lastPointer !== "pen";
}

/** Loopback hosts: the server's own machine, where project ports need no enter step. */
export function isLoopback(host: string): boolean {
  const name = host.replace(/^\[|\]$/g, "").toLowerCase();
  return name === "localhost" || name === "::1" || name === "0.0.0.0" || /^127\./.test(name);
}

/** A frame route always starts with `/`. */
export function normalizeRoute(route: string): string {
  const trimmed = route.trim();
  if (trimmed === "") return "/";
  return trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
}

/**
 * URL an iframe loads for `route` of the project on `port`, on the host the client used for sync (C2). Off
 * loopback the first load goes through the enter route, which sets the token cookie and redirects to `route`.
 */
export function frameUrl(host: string, port: number, route: string, token: string): string {
  const path = normalizeRoute(route);
  const authority = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  const base = `http://${authority}:${port}`;
  if (isLoopback(host)) return `${base}${path}`;
  return `${base}/__penwise/enter?${new URLSearchParams({ token, to: path })}`;
}

function readProject(entry: unknown): Project | null {
  if (typeof entry !== "object" || entry === null) return null;
  const row = entry as Record<string, unknown>;
  if (typeof row.id !== "string" || row.id === "") return null;
  const port = finite(row.port, 0);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return null;
  return {
    id: row.id,
    name: typeof row.name === "string" ? row.name : row.id,
    root: typeof row.root === "string" ? row.root : "",
    kind: typeof row.kind === "string" ? row.kind : "static",
    origin: typeof row.origin === "string" ? row.origin : "",
    port,
    notebook: typeof row.notebook === "string" ? row.notebook : null,
  };
}

/** Validates `GET /projects`; entries without an id or a port are dropped. */
export function readProjects(value: unknown): Project[] {
  if (!Array.isArray(value)) return [];
  const projects: Project[] = [];
  for (const entry of value) {
    const project = readProject(entry);
    if (project !== null) projects.push(project);
  }
  return projects;
}
