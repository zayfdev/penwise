/**
 * Ops the server refused: count, list, export and dismiss. The data comes from the wasm `Viewer`
 * (`dead_letters`, `dead_letter_bytes`, `dismiss_dead_letter`); this file only lays it out.
 */

import { el, setChildren } from "../dom";
import { toBase64 } from "../net/outbox-store";
import type { DeadLetter, Viewer } from "../wasm";

/** The slice of the `Viewer` the panel needs. */
export type DeadLetterSource = Pick<Viewer, "dead_letters" | "dead_letter_bytes">;

/** One export entry: the refusal and the op's stored envelope (what `decode_op` reads). */
export interface ExportedLetter {
  readonly op_id: string;
  readonly reason: string;
  readonly summary: string;
  readonly envelope_base64: string;
}

/** The JSON text of an export file for every dead letter of `source`. */
export function exportDeadLetters(source: DeadLetterSource): string {
  const letters: ExportedLetter[] = source.dead_letters().map((letter) => ({
    op_id: letter.op_id,
    reason: letter.reason,
    summary: letter.summary,
    envelope_base64: toBase64(source.dead_letter_bytes(letter.op_id) ?? new Uint8Array()),
  }));
  return JSON.stringify(letters, null, 2);
}

/** User actions from the panel. */
export interface DeadLetterEvents {
  dismiss(opId: string): void;
  exportAll(): void;
}

/** The refused-ops section of the sidebar (hidden while there are none). */
export class DeadLetterPanel {
  private readonly list: HTMLElement;

  constructor(
    private readonly root: HTMLElement,
    private readonly events: DeadLetterEvents,
  ) {
    this.list = root.querySelector<HTMLElement>("[data-dead-list]") ?? root;
    root.querySelector("[data-dead-export]")?.addEventListener("click", () => events.exportAll());
  }

  /** Re-renders from the viewer's current dead letters. */
  render(letters: readonly DeadLetter[]): void {
    this.root.hidden = letters.length === 0;
    const rows = letters.map((letter) => {
      const dismiss = el("button", {
        text: "Dismiss",
        attrs: { type: "button", "data-dismiss": letter.op_id },
      });
      dismiss.addEventListener("click", () => this.events.dismiss(letter.op_id));
      return el("div", { class: "dead", attrs: { "data-dead": letter.op_id } }, [
        el("span", { text: `${letter.summary}: ${letter.reason}` }),
        dismiss,
      ]);
    });
    setChildren(this.list, rows);
  }
}
