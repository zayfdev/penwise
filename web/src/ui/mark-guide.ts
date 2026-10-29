/**
 * The mark cheat sheet (spec §5): what each gesture means and what the ink colours say. Static and
 * read-only; the agent interprets anything not listed freely, so the note says that too.
 */

import { el } from "../dom";

/** Shape gestures the agent recognizes first (`MarkEntry` kind hints in core). */
const SHAPES: readonly (readonly [string, string])[] = [
  ["Circle / loop", "Look here (focus)"],
  ["Arrow", "Move this there — tail = what, head = where"],
  ["Cross (X)", "Remove"],
  ["Wavy line under", "Feels off; the agent asks or proposes"],
  ["Box in empty space", "New thing here (the contents are the sketch)"],
  ["Line through text", "Change this text (handwriting = the new text)"],
];

/** Ink colours carry meaning (§5). */
const COLOURS: readonly (readonly [string, string])[] = [
  ["Red ink", "Problem"],
  ["Blue ink", "Idea"],
  ["Green ink", "Good, keep"],
];

function item(label: string, meaning: string): HTMLLIElement {
  return el("li", {}, [el("strong", { text: label }), ` — ${meaning}`]);
}

/** Mounts the guide after the frames panel (the sidebar already exists; a missing anchor is ignored). */
export function attachMarkGuide(): void {
  const anchor = document.getElementById("frames-panel");
  if (!anchor) return;
  const section = el("section", { attrs: { id: "mark-guide" } }, [
    el("h2", { text: "Marks guide" }),
    el(
      "ul",
      {},
      SHAPES.map(([label, meaning]) => item(label, meaning)),
    ),
    el(
      "ul",
      {},
      COLOURS.map(([label, meaning]) => item(label, meaning)),
    ),
    el("p", {
      class: "note",
      text: "Anything else, and handwriting, the agent reads freely.",
    }),
  ]);
  anchor.after(section);
}
