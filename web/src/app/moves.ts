/**
 * Camera moves another device asks for (`SwitchPage`, `Focus`): done at once, or — while the owner is
 * drawing here —
 * offered on a chip; and a `Focus` that waits for its page and that page's frames before it fits the frame.
 */

import { AgentChip } from "../ui/agent-chip";
import { MessageKind, type Received } from "../wasm";
import { chipText, moveLabel, OwnerDevices, shouldDefer } from "./agent-move";
import type { MoveKind } from "./follow";

/** What the moves need from the app. */
export interface MoveDeps {
  /** A stroke is open. */
  drawing(): boolean;
  /** The page shown. */
  current(): string | null;
  /** The shown page's frames (for the chip's name). */
  frames(): readonly { readonly id: string; readonly label: string; readonly route: string }[];
  pageTitle(page: string): string | null;
  /** Asks to show `page` (done when the catalog has it and no stroke is open). */
  show(page: string): void;
  /** Fits frame `item` of the shown page; false when it is not a frame there. */
  focusItem(item: string): boolean;
  /** Fits the shown page's content. */
  fitContent(): void;
  /** A snapshot is half applied: the shown page's frames are not all known. */
  snapshotBusy(): boolean;
  now(): number;
}

interface PendingFocus {
  readonly page: string;
  readonly item: string | null;
}

/** Offers, defers and performs moves. */
export class Moves {
  private readonly chip: AgentChip;
  /** When the owner last finished a stroke here. */
  private lastInkAt: number | null = null;
  private readonly hands = new OwnerDevices();
  private pending: PendingFocus | null = null;
  /** The page whose snapshot has not ended yet. */
  private loading: string | null = null;

  constructor(
    stage: HTMLElement,
    private readonly deps: MoveDeps,
  ) {
    this.chip = new AgentChip(stage);
  }

  /** Every received frame: a device that inks live is one of the owner's hands. */
  observe(received: Pick<Received, "kind" | "device">): void {
    const inking =
      received.kind === MessageKind.StrokeDelta || received.kind === MessageKind.StrokeCancel;
    if (inking && received.device) this.hands.inked(received.device);
  }

  /** The owner finished a stroke. */
  inked(): void {
    this.lastInkAt = this.deps.now();
  }

  /** `page` was just opened: its frames arrive with its snapshot. */
  opened(page: string): void {
    this.loading = page;
  }

  /** A snapshot of `scope` ended. */
  snapshotEnded(scope: string): void {
    if (scope === this.loading || scope === "all") this.loading = null;
  }

  /** The owner chose a page themself: a waiting move is moot. */
  cancel(): void {
    this.pending = null;
    this.chip.hide();
  }

  /** Another device (`by`) asked to show `page` (and fit `item`). */
  offer(page: string, move: MoveKind, item: string | null, by: string | null): void {
    const focus = move === "focus" ? { page, item } : null;
    const nothing = page === this.deps.current() && focus === null;
    const defer = shouldDefer(
      move,
      this.hands.has(by),
      this.lastInkAt,
      this.deps.now(),
      this.deps.drawing(),
    );
    if (nothing || !defer) {
      this.perform(page, focus);
      return;
    }
    const frames = page === this.deps.current() ? this.deps.frames() : [];
    const label = moveLabel(item, frames, this.deps.pageTitle(page));
    this.chip.show(chipText(label), () => this.perform(page, focus));
  }

  private perform(page: string, focus: PendingFocus | null): void {
    this.chip.hide();
    this.pending = focus;
    this.deps.show(page);
    this.apply();
  }

  /** Fits the focused frame once its page and frames are shown; an unknown item fits the content. */
  apply(): void {
    const focus = this.pending;
    if (focus === null || focus.page !== this.deps.current()) return;
    if (this.loading === focus.page || this.deps.snapshotBusy()) return;
    this.pending = null;
    if (focus.item !== null && this.deps.focusItem(focus.item)) return;
    this.deps.fitContent();
  }
}
