/**
 * The design-harness camera and moves of the viewer: glides to a frame or the content, follows the live page's
 * scroll, offers other devices' page and focus moves (or defers them), and tells the tablet about a focus
 * made here.
 */

import type { Frame } from "../frames/model";
import type { Viewport, ViewTransform } from "../view";
import { Camera } from "./camera-control";
import { Moves } from "./moves";

/** What the controls need from the app. */
export interface DesignDeps {
  readonly stage: HTMLElement;
  view(): ViewTransform;
  viewport(): Viewport;
  /** Shows `view` (both canvases, the frames and the mark chips follow). */
  apply(view: ViewTransform): void;
  /** The camera moved by itself: the automatic refit must leave the view alone. */
  moved(): void;
  /** A stroke is open (the camera never follows the page's scroll then). */
  drawing(): boolean;
  current(): string | null;
  frames(): readonly Frame[];
  pageTitle(page: string): string | null;
  /** Asks to show `page`. */
  show(page: string): void;
  /** Fits frame `item` of the shown page; false when it is not a frame there. */
  focusItem(item: string): boolean;
  /** The view that fits the shown page's content, if it can be computed. */
  contentView(): ViewTransform | null;
  snapshotBusy(): boolean;
  /** Lead mode: the tablet follows what is shown here. */
  leading(): boolean;
  /** The `Focus` frame for the server, `undefined` when it does not speak protocol 2. */
  focusFrame(page: string, item: string): Uint8Array | undefined;
  send(frame: Uint8Array): void;
  problem(message: string): void;
}

/** Camera glides and moves from other devices. */
export class DesignControls {
  readonly camera: Camera;
  readonly moves: Moves;

  constructor(private readonly deps: DesignDeps) {
    this.camera = new Camera({
      view: () => deps.view(),
      viewport: () => deps.viewport(),
      apply: (view) => deps.apply(view),
      moved: () => deps.moved(),
    });
    this.moves = new Moves(deps.stage, {
      drawing: () => deps.drawing(),
      current: () => deps.current(),
      frames: () => deps.frames(),
      pageTitle: (page) => deps.pageTitle(page),
      show: (page) => deps.show(page),
      focusItem: (item) => deps.focusItem(item),
      fitContent: () => this.fitContent(),
      snapshotBusy: () => deps.snapshotBusy(),
      now: () => Date.now(),
    });
  }

  /** The live page scrolled by itself for the owner: the camera follows, never under the pen. */
  follow(dy: number): void {
    if (!this.deps.drawing()) this.camera.follow(dy);
  }

  /** Glides to what the shown page's content needs (the board view). */
  fitContent(): void {
    const view = this.deps.contentView();
    if (view !== null) this.camera.glideTo(view);
  }

  /** Shows the whole board at once (the "Board" button): no glide, so what follows sees the final view. */
  showContent(): void {
    this.camera.cancel();
    const view = this.deps.contentView();
    if (view !== null) this.deps.apply(view);
  }

  /** In lead mode the tablet follows a frame focused here (only to a server that knows `Focus`). */
  announceFocus(frame: Frame): void {
    const page = this.deps.current();
    if (!this.deps.leading() || page === null) return;
    try {
      const bytes = this.deps.focusFrame(page, frame.id);
      if (bytes) this.deps.send(bytes);
    } catch (error) {
      this.deps.problem(`focus not sent: ${String(error)}`);
    }
  }
}
