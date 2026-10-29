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

/** One `pw:layout` reading; `rects` entries are only the ones that changed after the first message. */
export interface Layout {
  readonly scroll: Point;
  readonly doc: Point;
  readonly viewport: Point;
  /** The state recipe active in the page (`""` = initial state). */
  readonly state: string;
  /** Element key → viewport rect; `null` when the key no longer resolves. */
  readonly rects: Readonly<Record<string, Rect | null>>;
}

/** The bridge's answer to a hit request: the element's stable key and its rect (viewport CSS px). */
export interface Hit {
  readonly key: string | null;
  readonly rect: Rect | null;
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
  | { readonly t: "replayed"; readonly state: string; readonly ok: boolean };

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
  return { scroll, doc, viewport, state, rects };
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
