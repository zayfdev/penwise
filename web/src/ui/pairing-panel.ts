/** Pairing: the QR and short code another device scans/types (`/pair.png`, `/pair/code`). */

import type { Api } from "../net/api";

/** Panel with the QR image, the code and a "new code" button. */
export class PairingPanel {
  private readonly image: HTMLImageElement;
  private readonly code: HTMLElement;

  constructor(
    root: HTMLElement,
    private readonly api: Pick<Api, "pairCode" | "pairQrUrl">,
  ) {
    this.image = root.querySelector<HTMLImageElement>("img") ?? document.createElement("img");
    this.code = root.querySelector<HTMLElement>("[data-pair-code]") ?? root;
    root
      .querySelector<HTMLButtonElement>("[data-pair-new]")
      ?.addEventListener("click", () => void this.load(true));
  }

  /** Fetches the current (or a fresh) code and reloads the QR. */
  async load(refresh: boolean): Promise<void> {
    try {
      const { code } = await this.api.pairCode(refresh);
      this.code.textContent = code;
      this.image.src = this.api.pairQrUrl(code);
    } catch (error) {
      this.code.textContent = `unavailable (${String(error)})`;
    }
  }
}
