/** The page's handwriting as text (`/text`). */

import type { Api } from "../net/api";

/** Panel with a "read" button and the recognized text. */
export class TextPanel {
  private readonly body: HTMLElement;
  private readonly refresh: HTMLButtonElement;

  constructor(
    root: HTMLElement,
    private readonly api: Pick<Api, "pageText">,
    private readonly page: () => string | null,
  ) {
    this.body = root.querySelector<HTMLElement>("[data-text-body]") ?? root;
    this.refresh =
      root.querySelector<HTMLButtonElement>("[data-text-read]") ?? document.createElement("button");
    this.refresh.addEventListener("click", () => void this.read(true));
  }

  /** Loads the cached (or first) reading of the shown page. */
  async read(refresh: boolean): Promise<void> {
    const page = this.page();
    if (page === null) return;
    this.body.textContent = "Reading…";
    try {
      const text = await this.api.pageText(page, refresh);
      this.body.textContent = text === "" ? "(no handwriting found)" : text;
    } catch (error) {
      this.body.textContent = `Could not read the page: ${String(error)}`;
    }
  }
}
