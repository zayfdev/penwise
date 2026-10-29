/**
 * Frame chip placement: chips sit just above their frame and are unscaled, so frames side by side at a low
 * canvas zoom would otherwise cover each other's controls (variant frames are 40 page px apart). A chip that
 * would overlap one already placed is lifted by one chip height until it clears all of them.
 */

/** A placed chip's box, CSS px (unscaled: `#frame-chips` is not transformed). */
export interface ChipBox {
  readonly left: number;
  readonly top: number;
  readonly width: number;
}

/** True when two chips would cover each other (their vertical spans are one chip height). */
function overlaps(a: ChipBox, b: ChipBox, height: number): boolean {
  return (
    a.left < b.left + b.width &&
    b.left < a.left + a.width &&
    a.top < b.top + height &&
    b.top < a.top + height
  );
}

/** `box` lifted above every already-placed chip until it overlaps none. */
export function liftAbovePlaced(box: ChipBox, height: number, placed: readonly ChipBox[]): ChipBox {
  let top = box.top;
  let moved = true;
  while (moved) {
    moved = false;
    for (const other of placed) {
      if (!overlaps({ ...box, top }, other, height)) continue;
      top -= height;
      moved = true;
    }
  }
  return { ...box, top };
}
