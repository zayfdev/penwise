/**
 * The sidebar's "Add frame" form: a linked project's route at a preset, placed on the shown page. The form only
 * collects the choice; the app builds the edit (`add_frame`).
 */

import { el } from "../dom";
import { PRESET_CHOICES, type Project } from "./model";

/** What the form asks for. */
export interface AddFrameEvents {
  add(project: string, route: string, preset: string): void;
}

/** Project · route · preset, mounted into `#frames-panel`. */
export class AddFrameForm {
  private readonly project = el("select", {
    attrs: { "aria-label": "Frame project", id: "frame-project" },
  });
  private readonly route = el("input", {
    attrs: {
      type: "text",
      placeholder: "/",
      "aria-label": "Frame route",
      value: "/",
      id: "frame-route",
    },
  });
  private readonly preset = el(
    "select",
    { attrs: { "aria-label": "Frame preset", id: "frame-preset" } },
    PRESET_CHOICES.map((choice) =>
      el("option", { text: choice.label, attrs: { value: choice.name } }),
    ),
  );
  private readonly addButton = el("button", {
    text: "Add frame",
    attrs: { type: "button", id: "add-frame" },
  });

  constructor(root: HTMLElement, events: AddFrameEvents) {
    this.addButton.disabled = true;
    this.addButton.addEventListener("click", () => {
      if (this.project.value === "") return;
      events.add(this.project.value, this.route.value, this.preset.value);
    });
    root.append(
      el("div", { class: "row" }, [this.project, this.preset]),
      el("div", { class: "row" }, [this.route, this.addButton]),
    );
  }

  /** The linked projects; adding is impossible while there are none. */
  render(projects: readonly Project[]): void {
    const previous = this.project.value;
    this.project.replaceChildren(
      ...projects.map((project) =>
        el("option", { text: project.name, attrs: { value: project.id } }),
      ),
    );
    if (projects.some((project) => project.id === previous)) this.project.value = previous;
    this.addButton.disabled = projects.length === 0;
  }
}
