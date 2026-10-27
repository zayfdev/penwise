/** Full-text search over recognized handwriting (`/search`). */

import { el, setChildren } from "../dom";
import type { Api } from "../net/api";

const DEBOUNCE_MS = 250;

/** Search box and hit list; clicking a hit opens its page. */
export class SearchPanel {
  private readonly results: HTMLElement;
  private timer = 0;

  constructor(
    root: HTMLElement,
    private readonly api: Pick<Api, "search">,
    private readonly open: (page: string) => void,
  ) {
    const input = root.querySelector<HTMLInputElement>("input");
    this.results = root.querySelector<HTMLElement>("[data-results]") ?? root;
    input?.addEventListener("input", () => {
      window.clearTimeout(this.timer);
      this.timer = window.setTimeout(() => void this.run(input.value), DEBOUNCE_MS);
    });
  }

  /** Runs `query` and lists the hits. */
  async run(query: string): Promise<void> {
    if (query.trim() === "") {
      setChildren(this.results, []);
      return;
    }
    try {
      const hits = await this.api.search(query);
      setChildren(
        this.results,
        hits.length === 0
          ? [el("div", { class: "hint", text: "No matches" })]
          : hits.map((hit) => {
              const row = el("button", { class: "hit", attrs: { type: "button" } }, [
                el("strong", { text: hit.title }),
                el("span", { text: hit.snippet }),
              ]);
              row.addEventListener("click", () => this.open(hit.page));
              return row;
            }),
      );
    } catch (error) {
      setChildren(this.results, [
        el("div", { class: "hint", text: `Search failed: ${String(error)}` }),
      ]);
    }
  }
}
