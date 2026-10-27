/** Reusable offscreen canvases (dab coverage, erase groups) so painting does not allocate per command. */
export class LayerPool {
  private readonly free: HTMLCanvasElement[] = [];

  /** A cleared canvas of exactly `width × height` device px. */
  acquire(width: number, height: number): HTMLCanvasElement {
    const canvas = this.free.pop() ?? document.createElement("canvas");
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    } else {
      canvas.getContext("2d")?.clearRect(0, 0, width, height);
    }
    return canvas;
  }

  /** Returns a canvas for reuse. */
  release(canvas: HTMLCanvasElement): void {
    if (this.free.length < 6) this.free.push(canvas);
  }
}

/** The context of `canvas` or an error (2D contexts only fail when the canvas is already another kind). */
export function context2d(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("2D canvas context unavailable");
  return ctx;
}
