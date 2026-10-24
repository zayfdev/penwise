/**
 * Pairing panel: shows the server's QR for the tablet and a code entry for remote browsers.
 *
 * Owner: slice d.
 *
 * - `GET /pair` answers only loopback requests (404 otherwise). On the server PC the viewer first self-pairs
 *   with one fetched code (`Connection.pair`), then shows a QR of a **fresh** code for the tablet (codes are
 *   single-use, `expires_ms` later; refresh the QR on expiry or after a successful pairing).
 * - On any other machine `/pair` is 404: the panel asks for a code typed from the PC's pairing page.
 * - The QR SVG is shown through `URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }))` in an
 *   `<img>` (never `innerHTML`; the CSP allows `blob:` images). The code text is shown too.
 */

/** `GET /pair` body. */
export interface PairingInfo {
  /** `penwise://pair?...` URI encoded in the QR. */
  readonly uri: string;
  /** The pairing code (secret until used). */
  readonly code: string;
  /** Standalone SVG document of the QR. */
  readonly svg: string;
  /** Unix ms. */
  readonly expires_ms: number;
}

/** User actions from the panel. */
export interface PairingEvents {
  /** User submitted a code and a device name. */
  submit(code: string, name: string): void;
}

/** Fetches a fresh code from `/pair`; `null` when not local (404) or on network failure. */
export async function fetchPairing(fetchFn: typeof fetch): Promise<PairingInfo | null> {
  throw new Error("unimplemented: fetchPairing");
}

/** Narrows an unknown JSON value to {@link PairingInfo}. */
export function parsePairingInfo(value: unknown): PairingInfo | null {
  throw new Error("unimplemented: parsePairingInfo");
}

/** True for `localhost`, `127.0.0.0/8` and `[::1]` page hosts. */
export function isLoopbackHost(hostname: string): boolean {
  throw new Error("unimplemented: isLoopbackHost");
}

/** The pairing section. */
export class PairingPanel {
  constructor(root: HTMLElement, events: PairingEvents) {
    throw new Error("unimplemented: PairingPanel.constructor");
  }

  /** Shows the QR + code + expiry countdown. */
  showQr(info: PairingInfo): void {
    throw new Error("unimplemented: PairingPanel.showQr");
  }

  /** Shows the code + name entry form. */
  showCodeEntry(): void {
    throw new Error("unimplemented: PairingPanel.showCodeEntry");
  }

  /** Shows an error line (e.g. `bad_pairing_code`). */
  showError(message: string): void {
    throw new Error("unimplemented: PairingPanel.showError");
  }

  /** Hides the panel and revokes the QR object URL. */
  hide(): void {
    throw new Error("unimplemented: PairingPanel.hide");
  }
}
