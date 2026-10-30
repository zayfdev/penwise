/**
 * The strips of one frame's still, as `<img>` elements laid into the frame's box (document coordinates, so a
 * strip at document y sits at `top: y`). Only strips that are on screen and not hidden under the live window are
 * images, requested no wider than the screen shows them; a new version or width keeps the old images until the
 * new ones have loaded, so the frame never flashes empty. A box far off screen releases all of them.
 */

import { el } from "../dom";
import { type DocRange, type Still, type Strip, stripsToShow, withToken, withWidth } from "./still";

/** Document px beyond the visible range that are loaded already, so a small pan shows no gap. */
export const STRIP_MARGIN_PX = 1_200;

/** What a render shows of a still. */
export interface StillShow {
  readonly cssW: number;
  readonly visible: DocRange | null;
  readonly covered: DocRange | null;
  /** The `w` strips are requested at (`null` = full size). */
  readonly width: number | null;
}

/** The image layer of one frame. */
export class StillView {
  readonly root = el("div", { class: "stills" });
  private readonly images = new Map<string, HTMLImageElement>();
  private wanted: ReadonlySet<string> = new Set();

  constructor(private readonly token: () => string) {}

  /** The urls of the images in the box now (tests and e2e read this). */
  get urls(): readonly string[] {
    return [...this.images.keys()];
  }

  /** Shows `still`'s strips that overlap `show.visible` and are not entirely under `show.covered`. */
  render(still: Still | null, show: StillShow): void {
    const strips =
      still === null ? [] : stripsToShow(still.strips, show.visible, show.covered, STRIP_MARGIN_PX);
    const keyed = strips.map((strip) => [withWidth(strip.url, show.width), strip] as const);
    this.wanted = new Set(keyed.map(([key]) => key));
    for (const [key, strip] of keyed) {
      const image = this.images.get(key) ?? this.add(key, strip, show.cssW);
      image.style.width = `${show.cssW}px`;
    }
    this.sweep();
  }

  /** Drops every image (the box is far off screen). */
  release(): void {
    this.wanted = new Set();
    for (const image of this.images.values()) image.remove();
    this.images.clear();
  }

  private add(key: string, strip: Strip, cssW: number): HTMLImageElement {
    const image = el("img", {
      attrs: {
        alt: "",
        draggable: "false",
        referrerpolicy: "no-referrer",
        "data-y": String(strip.y),
      },
    });
    image.style.top = `${strip.y}px`;
    image.style.width = `${cssW}px`;
    image.style.height = `${strip.h}px`;
    image.addEventListener("load", () => this.sweep());
    image.addEventListener("error", () => {
      // Dropped, so the next render asks for it again.
      image.remove();
      if (this.images.get(key) === image) this.images.delete(key);
    });
    image.src = withToken(key, this.token());
    this.images.set(key, image);
    this.root.append(image);
    return image;
  }

  /** Removes images nothing wants any more, once everything wanted has loaded. */
  private sweep(): void {
    for (const key of this.wanted) {
      const image = this.images.get(key);
      if (image && !(image.complete && image.naturalWidth > 0)) return;
    }
    for (const [key, image] of this.images) {
      if (this.wanted.has(key)) continue;
      image.remove();
      this.images.delete(key);
    }
  }
}
