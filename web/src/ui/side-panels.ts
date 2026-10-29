/** The sidebar panels that only talk to the HTTP API: page text, search and pairing, and the marks guide. */

import { byId } from "../dom";
import type { Api } from "../net/api";
import { attachMarkGuide } from "./mark-guide";
import { PairingPanel } from "./pairing-panel";
import { SearchPanel } from "./search-panel";
import { TextPanel } from "./text-panel";

/** Wires the text, search and pairing panels; `select` opens a page chosen in a search result. */
export function attachSidePanels(
  api: Api,
  current: () => string | null,
  select: (page: string) => void,
): void {
  new TextPanel(byId("text-panel", HTMLElement), api, current);
  new SearchPanel(byId("search-panel", HTMLElement), api, select);
  attachMarkGuide();
  const pairing = new PairingPanel(byId("pair-panel", HTMLElement), api);
  byId("pair-toggle", HTMLButtonElement).addEventListener("click", () => {
    const panel = byId("pair-panel", HTMLElement);
    panel.hidden = !panel.hidden;
    if (!panel.hidden) void pairing.load(false);
  });
}
