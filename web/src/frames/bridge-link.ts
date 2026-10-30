/**
 * One frame's bridge channel (contract K2): posts to the iframe and keeps what it answers.
 *
 * The bridge is cross-origin and untrusted; messages reach this class already decoded
 * ([`decodeBridge`](bridge-messages.ts)) and only because they came from the frame's own window. The channel
 * holds the last layout (the core's view of the page), the element keys to watch, and the hit answers a
 * stroke is waiting for.
 */

import {
  type BridgeMessage,
  type Hit,
  hitRequest,
  type Layout,
  type Point,
  type Rect,
  replayMessage,
  scrollToMessage,
  watchMessage,
} from "./bridge-messages";

/** The answer to a hit request when the frame (or its document) no longer exists. */
const MISSING: Hit = { key: null, rect: null };

/** The message channel of one frame iframe. */
export class BridgeChannel {
  private readonly hits = new Map<number, (hit: Hit) => void>();
  private nextHit = 1;
  private keys: string[] = [];
  private reported: Layout | null = null;
  /** Every rect reported since the document loaded, in document coordinates. */
  private docRects: Record<string, Rect | null> = {};

  /** `target` resolves the iframe's window each time (it changes identity only across process swaps). */
  constructor(private readonly target: () => Window | null) {}

  /** The last layout the bridge reported since this document loaded, or `null`. */
  layout(): Layout | null {
    return this.reported;
  }

  /** The page state the frame is in now (`""` before the first layout or on the initial state). */
  state(): string {
    return this.reported?.state ?? "";
  }

  /** Asks for the element under a frame-local viewport point; the answer lands while the stroke is drawn. */
  hit(x: number, y: number, answer: (hit: Hit) => void): void {
    const target = this.target();
    if (!target) {
      answer(MISSING);
      return;
    }
    const id = this.nextHit++;
    this.hits.set(id, answer);
    target.postMessage(hitRequest(id, x, y), "*");
  }

  /** Replaces the watched element keys; the bridge answers with a layout next frame. */
  watch(keys: readonly string[]): void {
    this.keys = [...keys];
    this.postWatch();
  }

  /** Asks the page to scroll to a document position (`pw:scrollTo`). */
  scrollTo(x: number, y: number, id: number): void {
    this.target()?.postMessage(scrollToMessage(x, y, id), "*");
  }

  /** Rebuilds a recorded page state (`pw:replay`). */
  replay(state: string): void {
    this.target()?.postMessage(replayMessage(state), "*");
  }

  /**
   * Records a layout message and keeps it as the one later rect merges apply to. Messages after the first carry
   * only the rects that changed, and each is relative to the scroll of its own message, so the channel keeps
   * every rect seen since the document loaded in document coordinates.
   */
  store(layout: Layout): void {
    this.keepRects(layout.rects, layout.scroll);
    this.reported = { ...layout, rects: this.rectsAt(layout.scroll) };
  }

  /** Rect the bridge sent outside a layout (the hit answer, viewport coords) in the stored layout. */
  mergeRects(rects: Readonly<Record<string, Rect | null>>): Layout | null {
    if (!this.reported) return null;
    this.keepRects(rects, this.reported.scroll);
    this.reported = { ...this.reported, rects: this.rectsAt(this.reported.scroll) };
    return this.reported;
  }

  private keepRects(rects: Readonly<Record<string, Rect | null>>, scroll: Point): void {
    for (const [key, rect] of Object.entries(rects)) {
      this.docRects[key] = rect && [rect[0] + scroll[0], rect[1] + scroll[1], rect[2], rect[3]];
    }
  }

  private rectsAt(scroll: Point): Record<string, Rect | null> {
    const out: Record<string, Rect | null> = {};
    for (const [key, rect] of Object.entries(this.docRects)) {
      out[key] = rect && [rect[0] - scroll[0], rect[1] - scroll[1], rect[2], rect[3]];
    }
    return out;
  }

  /** A hit answer: resolves the stroke's binding; a stale id is ignored. */
  answer(message: Extract<BridgeMessage, { t: "hit" }>): void {
    const pending = this.hits.get(message.id);
    if (!pending) return;
    this.hits.delete(message.id);
    pending({ key: message.key, rect: message.rect });
  }

  /** A new document: no layout until it reports, and it needs the watch list again. */
  loaded(): void {
    this.reported = null;
    this.docRects = {};
    this.postWatch();
  }

  /** The frame went away: strokes waiting for a hit fall back to an area binding. */
  close(): void {
    const pending = [...this.hits.values()];
    this.hits.clear();
    for (const answer of pending) answer(MISSING);
  }

  private postWatch(): void {
    this.target()?.postMessage(watchMessage(this.keys), "*");
  }
}
