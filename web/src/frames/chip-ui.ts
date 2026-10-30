/** The controls of one frame chip: label, preset, route, reload, ghost toggle and the variant pick button. */

import { el } from "../dom";
import { PRESET_CHOICES } from "./model";

/** The chip's elements, wired by the layer. */
export interface ChipParts {
  readonly chip: HTMLDivElement;
  readonly label: HTMLSpanElement;
  readonly preset: HTMLSelectElement;
  readonly route: HTMLInputElement;
  readonly reload: HTMLButtonElement;
  readonly ghostLabel: HTMLLabelElement;
  readonly ghostBox: HTMLInputElement;
  readonly useButton: HTMLButtonElement;
}

/** Builds the chip of frame `id` (no listeners). */
export function buildChip(id: string): ChipParts {
  const label = el("span", { class: "name" });
  const preset = el(
    "select",
    { attrs: { "aria-label": "Frame preset" } },
    PRESET_CHOICES.map((choice) =>
      el("option", { text: choice.label, attrs: { value: choice.name } }),
    ),
  );
  const route = el("input", { attrs: { type: "text", "aria-label": "Frame route" } });
  const reload = el("button", { text: "Reload", attrs: { type: "button", class: "reload" } });
  const ghostBox = el("input", {
    attrs: { type: "checkbox", "aria-label": "Show the ghost patch" },
  });
  const ghostLabel = el("label", { class: "ghost" }, [ghostBox, "Ghost"]);
  const useButton = el("button", { text: "Use this", attrs: { type: "button", class: "use" } });
  const chip = el("div", { class: "frame-chip", attrs: { "data-frame": id } }, [
    label,
    preset,
    route,
    reload,
    ghostLabel,
    useButton,
  ]);
  return { chip, label, preset, route, reload, ghostLabel, ghostBox, useButton };
}
