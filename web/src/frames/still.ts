/**
 * Stills (design-harness v2 contract §4): what a frame shows where it has no live window. The server captures
 * the whole document at the preset width and serves it as strips of at most 4096 CSS px; the client lays the
 * strips into the frame's box, and — when the frame has no live page — replaces the frame's layout in the core
 * with the still's (document rects of the watched keys). Pure: parsing, strip selection, strip sizes and the
 * layout JSON; the polling is `still-store.ts`.
 */

/** One horizontal strip of the document. */
export interface Strip {
  /** Document y of the strip's top edge, CSS px. */
  readonly y: number;
  readonly h: number;
  /** Server path of the PNG, as given (`/frames/{hex}/still/{version}/{y}.png`); only a query is added. */
  readonly url: string;
}

/** `GET /frames/{id}/still`. */
export interface Still {
  readonly version: string;
  /** Document size, CSS px. */
  readonly doc: readonly [number, number];
  /** The preset viewport the document was captured at. */
  readonly viewport: readonly [number, number];
  readonly strips: readonly Strip[];
  /** Document rects of the frame's watched keys (`null` = the element is gone). */
  readonly rects: Readonly<Record<string, readonly [number, number, number, number] | null>>;
  readonly state: string;
}

function finite(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function pair(value: unknown): [number, number] | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const a = finite(value[0]);
  const b = finite(value[1]);
  return a === null || b === null ? null : [a, b];
}

function quad(value: unknown): [number, number, number, number] | null {
  if (!Array.isArray(value) || value.length !== 4) return null;
  const nums = value.map(finite);
  const [x, y, w, h] = nums;
  if (x == null || y == null || w == null || h == null) return null;
  return [x, y, w, h];
}

function readStrip(entry: unknown): Strip | null {
  if (typeof entry !== "object" || entry === null) return null;
  const row = entry as Record<string, unknown>;
  const y = finite(row.y);
  const h = finite(row.h);
  if (y === null || h === null || h <= 0 || typeof row.url !== "string" || row.url === "") {
    return null;
  }
  return { y, h, url: row.url };
}

function readRects(value: unknown): Record<string, [number, number, number, number] | null> {
  const rects: Record<string, [number, number, number, number] | null> = {};
  if (typeof value !== "object" || value === null) return rects;
  for (const [key, rect] of Object.entries(value)) {
    if (rect === null) rects[key] = null;
    else {
      const box = quad(rect);
      if (box !== null) rects[key] = box;
    }
  }
  return rects;
}

/** Validates a still answer; `null` when it cannot be shown (no document size, no version). */
export function parseStill(json: unknown): Still | null {
  if (typeof json !== "object" || json === null) return null;
  const row = json as Record<string, unknown>;
  const version =
    typeof row.version === "string" || typeof row.version === "number" ? String(row.version) : null;
  const doc = pair(row.doc);
  const viewport = pair(row.viewport);
  if (version === null || doc === null || viewport === null) return null;
  const strips = Array.isArray(row.strips)
    ? row.strips.map(readStrip).filter((strip): strip is Strip => strip !== null)
    : [];
  const layout =
    typeof row.layout === "object" && row.layout !== null
      ? (row.layout as Record<string, unknown>)
      : {};
  return {
    version,
    doc,
    viewport,
    strips,
    rects: readRects(layout.rects),
    state: typeof layout.state === "string" ? layout.state : "",
  };
}

/** A range of document y, CSS px. */
export interface DocRange {
  readonly from: number;
  readonly to: number;
}

/**
 * The strips worth having as images: those that overlap `visible` (widened by `margin` so a small pan does not
 * show a gap) and are not entirely under the live window `covered` (the window is opaque).
 */
export function stripsToShow(
  strips: readonly Strip[],
  visible: DocRange | null,
  covered: DocRange | null,
  margin: number,
): readonly Strip[] {
  return strips.filter((strip) => {
    const top = strip.y;
    const bottom = strip.y + strip.h;
    if (visible !== null && (bottom <= visible.from - margin || top >= visible.to + margin)) {
      return false;
    }
    return !(covered !== null && top >= covered.from && bottom <= covered.to);
  });
}

/** Widths a strip is requested at, px: the smallest one that is not blurry on screen (fewer distinct images). */
const STRIP_WIDTHS = [256, 384, 512, 768, 1024, 1536, 2048];

/**
 * The `w` to ask the server for when a frame of CSS width `cssW` is shown at `scale` on a `dpr` screen, or
 * `null` for the full-size strip (a smaller strip would save under a fifth, or the screen shows it full size).
 */
export function stripWidth(cssW: number, scale: number, dpr: number): number | null {
  if (!(cssW > 0) || !(scale > 0)) return null;
  const needed = Math.ceil(cssW * scale * dpr);
  const bucket = STRIP_WIDTHS.find((width) => width >= needed);
  return bucket !== undefined && bucket < cssW * 0.8 ? bucket : null;
}

/** `url` asking for a strip `width` px wide (appended as given; the url is not parsed otherwise). */
export function withWidth(url: string, width: number | null): string {
  if (width === null) return url;
  return `${url}${url.includes("?") ? "&" : "?"}w=${width}`;
}

/**
 * Whether a box placed at `box` (stage CSS px) is within `margin` px of the stage: farther ones release their
 * images.
 */
export function boxNearScreen(
  box: {
    readonly left: number;
    readonly top: number;
    readonly width: number;
    readonly height: number;
  },
  viewport: { readonly width: number; readonly height: number },
  margin: number,
): boolean {
  if (viewport.width <= 0 || viewport.height <= 0) return true;
  return (
    box.left + box.width > -margin &&
    box.top + box.height > -margin &&
    box.left < viewport.width + margin &&
    box.top < viewport.height + margin
  );
}

/** `url` with the access token in its query, for an `<img>` (it cannot send a header). */
export function withToken(url: string, token: string): string {
  if (token === "") return url;
  return `${url}${url.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}`;
}

/**
 * The still as a core layout (`Viewer.set_frame_layout`). A frame that is not live has no scroll, so the
 * document rects are also the viewport rects the core expects (it adds the scroll, zero).
 */
export function stillLayoutJson(still: Still, preset: string): string {
  return JSON.stringify({
    preset,
    scroll: [0, 0],
    doc: still.doc,
    viewport: still.viewport,
    state: still.state,
    rects: still.rects,
  });
}
