/**
 * What the browser keeps about edits the server has not taken: the saved outbox (reload survival), the
 * `beforeunload` warning, clock-skew notices and the refused-ops panel. Everything here reads the wasm `Viewer`.
 */

import { OutboxStore } from "../net/outbox-store";
import { DeadLetterPanel, exportDeadLetters } from "../ui/dead-letters";
import type { Viewer } from "../wasm";

/** Offers `text` as a file download. */
function downloadText(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

/** Saved outbox, unload warning and dead letters of one tab. */
export class Unsent {
  private readonly store: OutboxStore;
  private readonly panel: DeadLetterPanel;

  constructor(
    private readonly viewer: Viewer,
    storage: Storage,
    device: string,
    panelRoot: HTMLElement,
    private readonly problem: (message: string) => void,
    private readonly changed: () => void,
  ) {
    this.store = new OutboxStore(storage, device, problem);
    this.panel = new DeadLetterPanel(panelRoot, {
      dismiss: (opId) => {
        this.viewer.dismiss_dead_letter(opId);
        this.changed();
      },
      exportAll: () => downloadText("penwise-refused-ops.json", exportDeadLetters(this.viewer)),
    });
  }

  /** Reloads this device's saved unsent edits when a blob exists; otherwise starts a fresh identity. */
  restoreOrIdentify(device: string, entropy: bigint): void {
    const saved = this.store.load();
    if (saved !== null) {
      try {
        this.viewer.restore_state(saved, entropy);
        return;
      } catch (error) {
        this.store.quarantine();
        this.problem(`saved unsent drawing could not be restored: ${String(error)}`);
      }
    }
    this.viewer.identify(device, entropy);
  }

  /** Saves what changed, reports clock skew, refreshes the refused-ops list; returns how many are refused. */
  sync(): number {
    this.store.save(this.viewer);
    const skew = this.viewer.take_clock_skew_ms();
    if (skew > 0) {
      this.problem(
        `a stamp from the server is ${Math.round(skew / 1000)}s ahead of this device's clock; new edits may be refused`,
      );
    }
    const letters = this.viewer.dead_letters();
    this.panel.render(letters);
    return letters.length;
  }

  /** Leaving with edits the server never acknowledged: ask first (the outbox is also kept in storage). */
  warn(event: BeforeUnloadEvent): void {
    this.store.save(this.viewer);
    if (this.viewer.pending_ops() === 0) return;
    event.preventDefault();
    event.returnValue = "";
  }
}
