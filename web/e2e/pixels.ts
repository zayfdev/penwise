/** Pixel probes run inside Chromium (canvas readback), so the test needs no image libraries. */

import type { Page } from "playwright-core";

/** Which pixels count. */
export type Tint = "red" | "blue" | "ink";

/** Bounding box (`[minX, minY, maxX, maxY]`) and count of the tinted pixels of an image. */
export interface PixelStats {
  readonly count: number;
  readonly bbox: [number, number, number, number] | null;
}

/** Stats of a PNG (the server's `/page.png`) in a scratch page. */
export async function pngStats(scratch: Page, png: Buffer, tint: Tint): Promise<PixelStats> {
  return scratch.evaluate(
    async ({ base64, tint }) => {
      const match = (r: number, g: number, b: number, a: number): boolean =>
        tint === "red"
          ? a > 128 && r > 140 && g < 90 && b < 90
          : tint === "blue"
            ? a > 128 && b > 140 && r < 90 && g < 120
            : a > 0 && r + g + b < 600;
      const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
      const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("no 2d context");
      ctx.drawImage(bitmap, 0, 0);
      const { data, width, height } = ctx.getImageData(0, 0, canvas.width, canvas.height);
      let count = 0;
      let box: [number, number, number, number] | null = null;
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const o = (y * width + x) * 4;
          if (!match(data[o] ?? 0, data[o + 1] ?? 0, data[o + 2] ?? 0, data[o + 3] ?? 0)) continue;
          count++;
          box = box
            ? [Math.min(box[0], x), Math.min(box[1], y), Math.max(box[2], x), Math.max(box[3], y)]
            : [x, y, x, y];
        }
      }
      return { count, bbox: box };
    },
    { base64: png.toString("base64"), tint },
  );
}

/** Stats of a canvas of the viewer (`committed` or `live`). */
export async function canvasStats(
  viewer: Page,
  id: "committed" | "live",
  tint: Tint,
): Promise<PixelStats> {
  return viewer.evaluate(
    ({ id, tint }) => {
      const match = (r: number, g: number, b: number, a: number): boolean =>
        tint === "red"
          ? a > 128 && r > 140 && g < 90 && b < 90
          : tint === "blue"
            ? a > 128 && b > 140 && r < 90 && g < 120
            : a > 0 && r + g + b < 600;
      const canvas = document.getElementById(id) as HTMLCanvasElement;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("no 2d context");
      const { data, width, height } = ctx.getImageData(0, 0, canvas.width, canvas.height);
      let count = 0;
      let box: [number, number, number, number] | null = null;
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const o = (y * width + x) * 4;
          if (!match(data[o] ?? 0, data[o + 1] ?? 0, data[o + 2] ?? 0, data[o + 3] ?? 0)) continue;
          count++;
          box = box
            ? [Math.min(box[0], x), Math.min(box[1], y), Math.max(box[2], x), Math.max(box[3], y)]
            : [x, y, x, y];
        }
      }
      return { count, bbox: box };
    },
    { id, tint },
  );
}
