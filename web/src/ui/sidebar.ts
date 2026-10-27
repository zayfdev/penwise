/**
 * Notebook / page list with "new notebook" and "new page" forms. Rows come from the viewer's catalog;
 * titles are user data and only ever reach the DOM through `textContent`.
 */

import type { NotebookGroup } from "../app/catalog";
import { el, setChildren } from "../dom";
import type { PageTemplate } from "../net/api";

/** User actions from the sidebar. */
export interface SidebarEvents {
  select(pageId: string): void;
  newPage(template: PageTemplate, notebook: string | null, title: string): void;
  newNotebook(title: string): void;
}

const TEMPLATES: readonly [PageTemplate, string][] = [
  ["blank", "Blank"],
  ["ruled", "Ruled"],
  ["grid", "Grid"],
  ["dots", "Dots"],
  ["infinite", "Infinite"],
];

/** The page list on the left. */
export class Sidebar {
  private readonly list = el("div", { class: "library" });
  private readonly title = el("input", {
    attrs: { type: "text", placeholder: "Title (optional)" },
  });
  private readonly template = el(
    "select",
    { attrs: { "aria-label": "Page template" } },
    TEMPLATES.map(([value, label]) => el("option", { text: label, attrs: { value } })),
  );
  private notebook: string | null = null;

  constructor(
    root: HTMLElement,
    private readonly events: SidebarEvents,
  ) {
    const add = el("button", { text: "New page", attrs: { type: "button", id: "add-page" } });
    add.addEventListener("click", () => {
      this.events.newPage(this.templateValue(), this.notebook, this.title.value.trim());
      this.title.value = "";
    });
    const nbTitle = el("input", {
      attrs: { type: "text", placeholder: "Notebook name", id: "nb-title" },
    });
    const addNb = el("button", {
      text: "New notebook",
      attrs: { type: "button", id: "add-notebook" },
    });
    addNb.addEventListener("click", () => {
      const name = nbTitle.value.trim();
      if (name === "") return;
      this.events.newNotebook(name);
      nbTitle.value = "";
    });
    root.append(
      this.list,
      el("div", { class: "form" }, [this.title, this.template, add]),
      el("div", { class: "form" }, [nbTitle, addNb]),
    );
  }

  /** Re-renders the list; `current` is highlighted and is the notebook new pages go into. */
  render(groups: readonly NotebookGroup[], current: string | null): void {
    this.notebook = null;
    const rows: HTMLElement[] = [];
    for (const { notebook, pages } of groups) {
      rows.push(el("div", { class: "notebook", text: notebook.title }));
      for (const page of pages) {
        const active = page.id === current;
        if (active) this.notebook = notebook.id;
        const row = el("button", {
          class: active ? "page active" : "page",
          text: page.title === "" ? "Untitled" : page.title,
          attrs: { type: "button", "data-page": page.id },
        });
        row.addEventListener("click", () => this.events.select(page.id));
        rows.push(row);
      }
    }
    if (this.notebook === null) this.notebook = groups[0]?.notebook.id ?? null;
    setChildren(this.list, rows);
  }

  private templateValue(): PageTemplate {
    const found = TEMPLATES.find(([value]) => value === this.template.value);
    return found ? found[0] : "blank";
  }
}
