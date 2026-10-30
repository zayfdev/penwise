/**
 * The sidebar's "Add frame" form: a linked project's route at a preset, placed on the shown page. The form only
 * collects the choice; the app builds the edit (`add_frame`).
 */

import { el } from "../dom";
import { PRESET_CHOICES, type Project } from "./model";

/** What the form asks for. */
export interface AddFrameEvents {
  add(project: string, route: string, preset: string): void;
  /** "Send": closes the project's open round (spec v2 §4). */
  send(project: string): void;
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

  private readonly sendButton = el("button", {
    text: "Send",
    attrs: { type: "button", id: "send-round", title: "Send the marks drawn since the last send" },
  });
  private readonly sendStatus = el("span", { attrs: { id: "send-status", role: "status" } });

  constructor(root: HTMLElement, events: AddFrameEvents) {
    this.addButton.disabled = true;
    this.sendButton.disabled = true;
    this.sendButton.addEventListener("click", () => {
      if (this.project.value !== "") events.send(this.project.value);
    });
    this.addButton.addEventListener("click", () => {
      if (this.project.value === "") return;
      events.add(this.project.value, this.route.value, this.preset.value);
    });
    root.append(
      el("div", { class: "row" }, [this.project, this.preset]),
      el("div", { class: "row" }, [this.route, this.addButton]),
      el("div", { class: "row" }, [this.sendButton, this.sendStatus]),
    );
  }

  /** The result of the last Send ("Sent", "No new marks", or a problem); empty clears it. */
  setSendStatus(text: string): void {
    this.sendStatus.textContent = text;
  }

  /** Enables Send while a send is not in flight. */
  setSending(sending: boolean): void {
    this.sendButton.disabled = sending || this.project.value === "";
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
    this.sendButton.disabled = projects.length === 0;
  }
}
