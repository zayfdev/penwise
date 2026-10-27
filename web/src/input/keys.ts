/** Undo/redo keyboard shortcuts. */

/** True while the user edits text (undo then belongs to the field, not the page). */
export function isTyping(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && ["text", "search", "number"].includes(target.type);
}

/** Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z and Ctrl/Cmd+Y redo (`step(redo)`). */
export function attachHistoryKeys(target: Window, step: (redo: boolean) => void): void {
  target.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey) || isTyping(event.target)) return;
    const key = event.key.toLowerCase();
    if (key === "z") step(event.shiftKey);
    else if (key === "y") step(true);
    else return;
    event.preventDefault();
  });
}
