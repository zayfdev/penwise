/**
 * Mark chips over the ink (contract K3): one chip per mark of the shown page, at the mark's rect corner.
 *
 * The core decides what a mark is, where it lands and whether it is hidden (`Viewer.marks`); this layer only
 * draws the status dot, filters done marks while "show done" is off, greys a mark whose page state is not
 * active, and offers the small menu (done/reopen/replay).
 */

import { el } from "../dom";
import type { Viewport, ViewTransform } from "../view";
import type { Rect } from "./bridge-messages";
import { intersects, type Placement } from "./model";

/** The status a mark carries (contract K1). */
export type MarkStatus = "new" | "seen" | "done" | "reopened";

/** One mark as `Viewer.marks(page)` reports it. `rect` is effective page coords, `null` when hidden. */
export interface Mark {
  readonly id: string;
  readonly frame: string;
  readonly preset: string;
  readonly status: MarkStatus;
  readonly state: string;
  readonly strokes: readonly string[];
  readonly rect: Rect | null;
}

/** What the chip layer asks the app to do. */
export interface MarkEvents {
  /** The owner picked a status from a chip menu. */
  status(id: string, status: MarkStatus): void;
  /** The owner asked for the mark's recorded page state to be replayed. */
  replay(frame: string, state: string): void;
}

/** Where a mark's rect lands on the stage, CSS px. */
export function placeMark(rect: Rect, view: ViewTransform): Placement {
  return {
    left: (rect[0] - view.originX) * view.scale,
    top: (rect[1] - view.originY) * view.scale,
    width: rect[2] * view.scale,
    height: rect[3] * view.scale,
  };
}

/** Size of a mark chip, CSS px (it is centred on the rect's top-right corner). */
const CHIP_SIZE = 18;

function isStatus(value: unknown): value is MarkStatus {
  return value === "new" || value === "seen" || value === "done" || value === "reopened";
}

function finite(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readRect(value: unknown): Rect | null {
  if (!Array.isArray(value) || value.length !== 4) return null;
  const x = finite(value[0]);
  const y = finite(value[1]);
  const w = finite(value[2]);
  const h = finite(value[3]);
  return x === null || y === null || w === null || h === null ? null : [x, y, w, h];
}

function readMark(entry: unknown): Mark | null {
  if (typeof entry !== "object" || entry === null) return null;
  const row = entry as Record<string, unknown>;
  const status = row.status;
  if (typeof row.id !== "string" || typeof row.frame !== "string" || !isStatus(status)) {
    return null;
  }
  return {
    id: row.id,
    frame: row.frame,
    preset: typeof row.preset === "string" ? row.preset : "",
    status,
    state: typeof row.state === "string" ? row.state : "",
    strokes: Array.isArray(row.strokes)
      ? row.strokes.filter((id): id is string => typeof id === "string")
      : [],
    rect: readRect(row.rect),
  };
}

/** Parses `Viewer.marks(page)`; malformed entries are dropped rather than breaking the overlay. */
export function parseMarks(json: string): Mark[] {
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return [];
  }
  if (!Array.isArray(value)) return [];
  const marks: Mark[] = [];
  for (const entry of value) {
    const mark = readMark(entry);
    if (mark !== null) marks.push(mark);
  }
  return marks;
}

/** Chips drawn for the marks of the shown page. */
export class MarkLayer {
  private view: ViewTransform = { scale: 1, originX: 0, originY: 0 };
  private viewport: Viewport = { width: 0, height: 0 };
  private marks: readonly Mark[] = [];
  private showDone = false;
  private menu: HTMLDivElement | null = null;
  private menuFor: string | null = null;
  private readonly chips = new Map<string, HTMLButtonElement>();

  constructor(
    private readonly root: HTMLElement,
    private readonly events: MarkEvents,
    /** The page state currently active in a frame (a mark whose state differs is dimmed). */
    private readonly stateOf: (frame: string) => string,
  ) {
    window.addEventListener(
      "pointerdown",
      (event) => {
        if (event.target instanceof Node && this.root.contains(event.target)) return;
        this.closeMenu();
      },
      true,
    );
  }

  /** Moves the chips with the canvas view. */
  setView(view: ViewTransform, viewport: Viewport): void {
    this.view = view;
    this.viewport = viewport;
    this.place();
  }

  /** Renders the marks; `showDone` keeps done marks on screen. */
  render(marks: readonly Mark[], showDone: boolean): void {
    this.marks = marks;
    this.showDone = showDone;
    const live = new Set<string>();
    for (const mark of marks) {
      if (!this.shown(mark)) continue;
      live.add(mark.id);
      const chip = this.chips.get(mark.id) ?? this.addChip(mark);
      this.paintChip(chip, mark);
    }
    for (const [id, chip] of this.chips) {
      if (live.has(id)) continue;
      chip.remove();
      this.chips.delete(id);
      if (this.menuFor === id) this.closeMenu();
    }
    this.place();
  }

  /** Whether a mark appears at all: not done (unless shown) and placed by the core. */
  private shown(mark: Mark): boolean {
    return mark.rect !== null && (this.showDone || mark.status !== "done");
  }

  /** The recorded state of the mark is not the one active now: greyed with a replay option. */
  private dimmed(mark: Mark): boolean {
    return mark.state !== "" && mark.state !== this.stateOf(mark.frame);
  }

  private addChip(mark: Mark): HTMLButtonElement {
    const chip = el("button", {
      class: "mark-chip",
      attrs: { type: "button", "data-mark": mark.id },
    });
    chip.append(el("span", { class: "dot" }));
    chip.addEventListener("click", () => this.toggleMenu(mark.id));
    this.root.append(chip);
    this.chips.set(mark.id, chip);
    return chip;
  }

  private paintChip(chip: HTMLButtonElement, mark: Mark): void {
    const dimmed = this.dimmed(mark);
    const label = `Mark ${mark.status}${dimmed ? " (page state not active)" : ""}`;
    chip.dataset.status = mark.status;
    chip.classList.toggle("dimmed", dimmed);
    chip.title = label;
    chip.setAttribute("aria-label", label);
  }

  /** Places every visible chip on its rect's top-right corner; chips off the stage are hidden. */
  private place(): void {
    for (const mark of this.marks) {
      const chip = this.chips.get(mark.id);
      if (!chip || mark.rect === null) continue;
      const box = placeMark(mark.rect, this.view);
      chip.hidden = !intersects(box, this.viewport);
      chip.style.left = `${Math.round(box.left + box.width)}px`;
      chip.style.top = `${Math.round(box.top)}px`;
    }
    this.placeMenu();
  }

  private toggleMenu(id: string): void {
    if (this.menuFor === id) {
      this.closeMenu();
      return;
    }
    const mark = this.marks.find((entry) => entry.id === id);
    if (!mark) return;
    this.closeMenu();
    this.menuFor = id;
    const menu = el("div", { class: "mark-menu", attrs: { role: "menu", "data-mark": id } });
    if (mark.status === "done") {
      menu.append(this.item("Reopen", () => this.events.status(id, "reopened")));
    } else {
      menu.append(this.item("Done", () => this.events.status(id, "done")));
    }
    if (this.dimmed(mark)) {
      menu.append(this.item("Replay state", () => this.events.replay(mark.frame, mark.state)));
    }
    this.menu = menu;
    this.root.append(menu);
    this.placeMenu();
  }

  private item(label: string, action: () => void): HTMLButtonElement {
    const button = el("button", { text: label, attrs: { type: "button", role: "menuitem" } });
    button.addEventListener("click", () => {
      this.closeMenu();
      action();
    });
    return button;
  }

  private placeMenu(): void {
    const menu = this.menu;
    const id = this.menuFor;
    if (!menu || id === null) return;
    const chip = this.chips.get(id);
    if (!chip) {
      this.closeMenu();
      return;
    }
    menu.style.left = chip.style.left;
    menu.style.top = `${Number.parseInt(chip.style.top || "0", 10) + CHIP_SIZE}px`;
  }

  private closeMenu(): void {
    this.menu?.remove();
    this.menu = null;
    this.menuFor = null;
  }
}
