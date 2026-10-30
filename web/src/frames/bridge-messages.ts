/**
 * The bridge protocol of the PC window (design-harness-spec §6.2, contract K2): decoding what the injected
 * `bridge.js` posts, the messages it takes, and the binding handed to the core on pen-up.
 *
 * The bridge lives in another origin (a project port), so nothing here trusts it: every field is checked and a
 * malformed message is dropped instead of reaching the core. The frame host is identified by `event.source`
 * (see {@link isFromFrame}), never by `event.origin` alone.
 */

/** A point in viewport CSS px of the framed page. */
export type Point = readonly [number, number];

/** `[x, y, w, h]` of an element's border box, viewport CSS px. `null` = the element is gone. */
export type Rect = readonly [number, number, number, number];

/** Why the page scrolled (`pw:scroll`/`pw:layout` `cause`, contract §5): the owner, a `pw:scrollTo` request, or the page. */
export type ScrollCause = "user" | "request" | "page";

/** One `pw:layout` reading; `rects` entries are only the ones that changed after the first message. */
export interface Layout {
  readonly scroll: Point;
  readonly doc: Point;
  readonly viewport: Point;
  /** The state recipe active in the page (`""` = initial state). */
  readonly state: string;
  /** What moved the scroll since the last message. */
  readonly cause: ScrollCause;
  /** For `cause: "request"`: the `id` of the `pw:scrollTo` that caused it. */
  readonly requestId: number | null;
  /** Element key → viewport rect; `null` when the key no longer resolves. */
  readonly rects: Readonly<Record<string, Rect | null>>;
}

/** The bridge's answer to a hit request: the element's stable key and its rect (viewport CSS px). */
export interface Hit {
  readonly key: string | null;
  readonly rect: Rect | null;
}

/** One DOM op of a P1 patch the bridge could not apply. */
export interface PatchFailure {
  readonly index: number;
  readonly reason: string;
}

/** A decoded bridge message. Unknown or malformed messages decode to `null`. */
export type BridgeMessage =
  | { readonly t: "layout"; readonly layout: Layout }
  | {
      readonly t: "hit";
      readonly id: number;
      readonly key: string | null;
      readonly rect: Rect | null;
    }
  | { readonly t: "replayed"; readonly state: string; readonly ok: boolean }
  /** A (re)loaded document: the host re-sends the watch list and the frame's patch. */
  | { readonly t: "hello" }
  /** Result of applying/reverting a patch (`ok` false lists the DOM ops that failed). */
  | { readonly t: "patched"; readonly ok: boolean; readonly failed: readonly PatchFailure[] };

/** How a stroke drawn over a frame follows it (contract K1 `mode`). */
export type BindingMode = "bound" | "area";

/** A stroke's frame binding as the client knows it while drawing (the core fills preset/state/anchor). */
export interface StrokeBinding {
  readonly frame: string;
  /** The mode chosen on the toolbar; a bound stroke without a key still commits as an area binding. */
  readonly mode: BindingMode;
  /** The element key, once the bridge answered the pen-down hit request (bound mode only). */
  key(): string | null;
}

/** True when `source` is this frame's own window: a page inside the frame must not drive the viewer. */
export function isFromFrame(source: unknown, contentWindow: unknown): boolean {
  return contentWindow !== null && contentWindow !== undefined && source === contentWindow;
}

function finite(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function pair(value: unknown): Point | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const x = finite(value[0]);
  const y = finite(value[1]);
  return x === null || y === null ? null : [x, y];
}

function rect(value: unknown): Rect | null {
  if (!Array.isArray(value) || value.length !== 4) return null;
  const x = finite(value[0]);
  const y = finite(value[1]);
  const w = finite(value[2]);
  const h = finite(value[3]);
  return x === null || y === null || w === null || h === null ? null : [x, y, w, h];
}

/** Decodes `pw:layout`; a missing or malformed required field drops the message (the core keeps its layout). */
function decodeLayout(row: Record<string, unknown>): Layout | null {
  const scroll = pair(row.scroll);
  const doc = pair(row.doc);
  const viewport = pair(row.viewport);
  const state = typeof row.state === "string" ? row.state : "";
  if (scroll === null || doc === null || viewport === null) return null;
  const rects: Record<string, Rect | null> = {};
  if (typeof row.rects === "object" && row.rects !== null) {
    for (const [key, value] of Object.entries(row.rects)) {
      // `null` is the bridge saying "the element is gone"; anything else malformed is ignored.
      if (value === null) rects[key] = null;
      else {
        const box = rect(value);
        if (box !== null) rects[key] = box;
      }
    }
  }
  const cause: ScrollCause = row.cause === "user" || row.cause === "request" ? row.cause : "page";
  return { scroll, doc, viewport, state, cause, requestId: finite(row.id), rects };
}

/** A hit answer in the bridge's shape (`pw:hit` fields, or the server's `GET /frames/{id}/hit` body). */
export function readHit(value: unknown): Hit {
  if (typeof value !== "object" || value === null) return { key: null, rect: null };
  const row = value as Record<string, unknown>;
  const key = typeof row.key === "string" && row.key !== "" ? row.key : null;
  return { key, rect: key === null ? null : rect(row.rect) };
}

/** `pw:scrollTo`: scroll the framed page to a document position; the bridge echoes `id` on the scroll it causes. */
export function scrollToMessage(
  x: number,
  y: number,
  id: number,
): { t: "pw:scrollTo"; x: number; y: number; id: number } {
  return { t: "pw:scrollTo", x, y, id };
}

/** Parses one bridge message (object, or its JSON string form for hosts that stringify). */
export function decodeBridge(data: unknown): BridgeMessage | null {
  if (typeof data === "string") {
    try {
      return decodeBridge(JSON.parse(data));
    } catch {
      return null;
    }
  }
  if (typeof data !== "object" || data === null) return null;
  const row = data as Record<string, unknown>;
  if (row.t === "pw:layout") {
    const layout = decodeLayout(row);
    return layout === null ? null : { t: "layout", layout };
  }
  if (row.t === "pw:hit") {
    const id = finite(row.id);
    if (id === null) return null;
    const key = typeof row.key === "string" && row.key !== "" ? row.key : null;
    return { t: "hit", id, key, rect: rect(row.rect) };
  }
  if (row.t === "pw:replayed") {
    return {
      t: "replayed",
      state: typeof row.state === "string" ? row.state : "",
      ok: row.ok === true,
    };
  }
  if (row.t === "pw:hello") return { t: "hello" };
  if (row.t === "pw:patched") {
    const failed: PatchFailure[] = [];
    if (Array.isArray(row.failed)) {
      for (const entry of row.failed) {
        if (typeof entry !== "object" || entry === null) continue;
        const failure = entry as Record<string, unknown>;
        const index = finite(failure.index);
        if (index === null) continue;
        failed.push({
          index,
          reason: typeof failure.reason === "string" ? failure.reason : "",
        });
      }
    }
    return { t: "patched", ok: row.ok === true, failed };
  }
  return null;
}

/** `pw:watch`: the element keys whose rects the bridge should report. */
export function watchMessage(keys: readonly string[]): { t: "pw:watch"; keys: string[] } {
  return { t: "pw:watch", keys: [...keys] };
}

/** `pw:hit?`: the element under a frame-local viewport point (answered as `pw:hit`). */
export function hitRequest(
  id: number,
  x: number,
  y: number,
): { t: "pw:hit?"; id: number; x: number; y: number } {
  return { t: "pw:hit?", id, x, y };
}

/** `pw:replay`: rebuild a recorded page state in the frame. */
export function replayMessage(state: string): { t: "pw:replay"; state: string } {
  return { t: "pw:replay", state };
}

/**
 * What to send a bridge for a frame's temporary patch: applied while the ghost is on, reverted while it is
 * off. `null` = a plain frame (nothing to send). The patch travels as its JSON text; the bridge parses it.
 */
export function patchFor(
  patch: string | null,
  ghost: boolean,
): { readonly patch: string; readonly on: boolean } | null {
  return patch === null ? null : { patch, on: ghost };
}

/** `pw:patch`: apply (`on`) or revert the frame's patch on the live page. */
export function patchMessage(
  patch: string,
  on: boolean,
): { t: "pw:patch"; patch: string; on: boolean } {
  return { t: "pw:patch", patch, on };
}

/** The core's layout JSON (`Viewer.set_frame_layout`): the bridge reading plus the frame's preset. */
export function layoutJson(layout: Layout, preset: string): string {
  return JSON.stringify({
    preset,
    scroll: layout.scroll,
    doc: layout.doc,
    viewport: layout.viewport,
    state: layout.state,
    rects: layout.rects,
  });
}

/**
 * Binding handed to the core when a stroke drawn over a frame is committed (contract K1).
 *
 * `bound` needs a key from the bridge; while it has none (area mode, or the hit never resolved) the stroke is
 * pinned to the area at its draw position instead. `null` = the stroke is not on a frame at all.
 */
export function bindingJson(
  frame: string | null,
  mode: BindingMode,
  key: string | null,
): string | null {
  if (frame === null || frame === "") return null;
  const bound = mode === "bound" && key !== null && key !== "";
  return JSON.stringify(bound ? { frame, mode: "bound", key } : { frame, mode: "area" });
}
