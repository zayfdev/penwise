/** The chip that offers a deferred camera move ("Agent: open <label> →"); a newer move replaces it. */

import { el } from "../dom";

/** A button over the stage that performs one move when pressed. */
export class AgentChip {
  private readonly button = el("button", {
    class: "agent-chip",
    attrs: { type: "button", id: "agent-chip" },
  });
  private action: (() => void) | null = null;

  constructor(stage: HTMLElement) {
    this.button.hidden = true;
    this.button.addEventListener("click", () => {
      const action = this.action;
      this.hide();
      action?.();
    });
    // Pressing it must not start a stroke on the canvas below.
    this.button.addEventListener("pointerdown", (event) => event.stopPropagation());
    stage.append(this.button);
  }

  /** Shows `text`; pressing the chip runs `action`. */
  show(text: string, action: () => void): void {
    this.button.textContent = text;
    this.action = action;
    this.button.hidden = false;
  }

  /** Drops the offer. */
  hide(): void {
    this.action = null;
    this.button.hidden = true;
  }
}
