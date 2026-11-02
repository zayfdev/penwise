/** Brush, undo/redo, follow mode and view buttons. */

import type { FollowMode } from "../app/follow";
import { el } from "../dom";
import type { BindingMode } from "../frames/bridge-messages";
import type { BrushChoice } from "../input/pen";

/** User actions from the toolbar. */
export interface ToolbarEvents {
  undo(): void;
  redo(): void;
  /** Fit the current frame's width (readable page). */
  fit(): void;
  /** See the whole board. */
  board(): void;
  mode(mode: FollowMode): void;
  /** "Draw with mouse": the mouse inks over frames instead of clicking through them. */
  mouseDraw(on: boolean): void;
  /** How a stroke started over a frame follows it (contract K3). */
  binding(mode: BindingMode): void;
  /** "Show done": done marks stay on screen while on. */
  showDone(on: boolean): void;
}

/** Hex `#rrggbb` → opaque `0xAARRGGBB`; black for anything else. */
export function argbOf(hex: string): number {
  const match = /^#([0-9a-fA-F]{6})$/.exec(hex);
  return match?.[1] ? (0xff000000 | Number.parseInt(match[1], 16)) >>> 0 : 0xff000000;
}

/** The tool strip over the page. */
export class Toolbar {
  private readonly preset: HTMLSelectElement;
  private readonly color: HTMLInputElement;
  private readonly size: HTMLInputElement;
  private readonly undoButton: HTMLButtonElement;
  private readonly redoButton: HTMLButtonElement;

  constructor(root: HTMLElement, presets: readonly string[], events: ToolbarEvents) {
    this.preset = el(
      "select",
      { attrs: { "aria-label": "Brush", id: "tool-preset" } },
      presets.map((name, i) => el("option", { text: name, attrs: { value: String(i) } })),
    );
    this.color = el("input", {
      attrs: { type: "color", value: "#000000", "aria-label": "Color", id: "tool-color" },
    });
    this.size = el("input", {
      attrs: {
        type: "range",
        min: "0.5",
        max: "4",
        step: "0.1",
        value: "1",
        "aria-label": "Size",
        id: "tool-size",
      },
    });
    this.undoButton = this.button("Undo", "tool-undo", events.undo);
    this.redoButton = this.button("Redo", "tool-redo", events.redo);
    const mode = el("select", { attrs: { "aria-label": "Follow", id: "tool-mode" } }, [
      el("option", { text: "Follow tablet", attrs: { value: "follow" } }),
      el("option", { text: "Free", attrs: { value: "free" } }),
      el("option", { text: "Lead tablet", attrs: { value: "lead" } }),
    ]);
    mode.addEventListener("change", () => {
      if (mode.value === "follow" || mode.value === "free" || mode.value === "lead") {
        events.mode(mode.value);
      }
    });
    const mouseDraw = el("input", { attrs: { type: "checkbox", id: "tool-draw-mouse" } });
    mouseDraw.addEventListener("change", () => events.mouseDraw(mouseDraw.checked));
    const binding = el("select", { attrs: { "aria-label": "Binding", id: "tool-bind" } }, [
      el("option", { text: "Bind to element", attrs: { value: "bound" } }),
      el("option", { text: "Pin to area", attrs: { value: "area" } }),
    ]);
    binding.addEventListener("change", () => {
      if (binding.value === "bound" || binding.value === "area") events.binding(binding.value);
    });
    const showDone = el("input", { attrs: { type: "checkbox", id: "tool-show-done" } });
    showDone.addEventListener("change", () => events.showDone(showDone.checked));
    root.prepend(
      this.preset,
      this.color,
      this.size,
      this.undoButton,
      this.redoButton,
      this.button("Fit", "tool-fit", events.fit),
      this.button("Board", "tool-board", events.board),
      mode,
      binding,
      el("label", { class: "toggle" }, [mouseDraw, "Draw with mouse"]),
      el("label", { class: "toggle" }, [showDone, "Show done"]),
    );
  }

  /** The current brush. */
  brush(): BrushChoice {
    return {
      preset: Number.parseInt(this.preset.value, 10) || 0,
      argb: argbOf(this.color.value),
      scale: Number.parseFloat(this.size.value) || 1,
    };
  }

  /** Enables/disables undo and redo. */
  setHistory(canUndo: boolean, canRedo: boolean): void {
    this.undoButton.disabled = !canUndo;
    this.redoButton.disabled = !canRedo;
  }

  private button(label: string, id: string, action: () => void): HTMLButtonElement {
    const button = el("button", { text: label, attrs: { type: "button", id } });
    button.addEventListener("click", action);
    return button;
  }
}
