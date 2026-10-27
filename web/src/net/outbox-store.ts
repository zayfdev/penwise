/**
 * Keeps the viewer's unsent edits across a page reload: `Viewer.export_state()` (device + outbox, a wasm blob
 * this file never parses) base64-encoded in `localStorage` under the tab's device id. The viewer says when the
 * blob changed (`state_revision`), so a save only happens after an edit, an ack or a dismissed dead letter.
 */

import type { Viewer } from "../wasm";

/** Storage key prefix; the device id follows. */
export const OUTBOX_KEY_PREFIX = "penwise.outbox.v1.";

/** The slice of the wasm `Viewer` the store reads. */
export type PersistedViewer = Pick<
  Viewer,
  "export_state" | "state_revision" | "pending_ops" | "dead_letters"
>;

/** Bytes → base64 (chunked: `String.fromCharCode(...big)` overflows the call stack). */
export function toBase64(bytes: Uint8Array): string {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000)
    text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
}

/** Inverse of {@link toBase64}; `null` for text that is not base64. */
export function fromBase64(text: string): Uint8Array | null {
  try {
    return Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
  } catch {
    return null;
  }
}

/** Saves and loads the outbox blob of one device. */
export class OutboxStore {
  private saved = 0;
  private readonly key: string;

  constructor(
    private readonly storage: Pick<Storage, "getItem" | "setItem" | "removeItem">,
    device: string,
    private readonly problem: (message: string) => void,
  ) {
    this.key = `${OUTBOX_KEY_PREFIX}${device}`;
  }

  /** The saved blob, if any. */
  load(): Uint8Array | null {
    const text = this.storage.getItem(this.key);
    return text === null ? null : fromBase64(text);
  }

  /** Drops a blob the viewer could not restore (kept under another key so nothing is lost silently). */
  quarantine(): void {
    const text = this.storage.getItem(this.key);
    if (text === null) return;
    this.storage.setItem(`${this.key}.unreadable`, text);
    this.storage.removeItem(this.key);
  }

  /** Writes the blob when the viewer's state moved since the last save. Returns false on a failed write. */
  save(viewer: PersistedViewer): boolean {
    const revision = viewer.state_revision();
    if (revision === this.saved) return true;
    try {
      // Nothing unsent or refused: drop the blob so no tab ever adopts an empty outbox.
      if (viewer.pending_ops() === 0 && viewer.dead_letters().length === 0)
        this.storage.removeItem(this.key);
      else this.storage.setItem(this.key, toBase64(viewer.export_state()));
      this.saved = revision;
      return true;
    } catch (error) {
      this.problem(`could not keep unsent drawing across a reload: ${String(error)}`);
      return false;
    }
  }
}
