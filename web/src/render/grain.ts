/**
 * Paper grain patterns for dab strokes (see `crates/penwise-wasm/src/grain.rs`): one height tile per grain
 * scale (`bake_grain_tile`), mapped to alpha by `grain_coverage_lut(level, softness, strength)`, used as a
 * repeating `CanvasPattern` in page space. Baking is expensive, so both stages are cached.
 */

/** The wasm functions grain needs. */
export interface GrainSource {
  grain_period_px(scale: number): number;
  bake_grain_tile(scale: number, size: number): Uint8Array;
  grain_coverage_lut(level: number, softness: number, strength: number): Uint8Array;
}

const MAX_PATTERNS = 64;
const MAX_TILES = 8;

/** Rounds to 1/32 so near-equal strokes share a pattern. */
function bucket(v: number): number {
  return Math.round(v * 32) / 32;
}

/** Cache of grain tiles and patterns. */
export class GrainCache {
  private readonly tiles = new Map<number, { size: number; heights: Uint8Array }>();
  private readonly patterns = new Map<string, CanvasPattern>();

  constructor(
    private readonly source: GrainSource,
    private readonly makeCanvas: (
      width: number,
      height: number,
    ) => HTMLCanvasElement = createCanvas,
  ) {}

  /** Cache key of one stroke's grain (levels/strengths bucketed). */
  static key(scale: number, level: number, softness: number, strength: number): string {
    return `${scale.toFixed(3)}|${bucket(level)}|${bucket(softness)}|${bucket(strength)}`;
  }

  /** Pattern for one stroke's grain, or `null` when the context cannot make one (or the tile is empty). */
  pattern(
    ctx: CanvasRenderingContext2D,
    scale: number,
    level: number,
    softness: number,
    strength: number,
  ): CanvasPattern | null {
    const key = GrainCache.key(scale, level, softness, strength);
    const cached = this.patterns.get(key);
    if (cached) return cached;
    const tile = this.tile(scale);
    if (!tile) return null;
    const lut = this.source.grain_coverage_lut(bucket(level), bucket(softness), bucket(strength));
    const canvas = this.makeCanvas(tile.size, tile.size);
    const tctx = canvas.getContext("2d");
    if (!tctx) return null;
    const image = tctx.createImageData(tile.size, tile.size);
    for (let i = 0; i < tile.heights.length; i++) {
      const o = i * 4;
      image.data[o] = 255;
      image.data[o + 1] = 255;
      image.data[o + 2] = 255;
      image.data[o + 3] = lut[tile.heights[i] ?? 0] ?? 255;
    }
    tctx.putImageData(image, 0, 0);
    const pattern = ctx.createPattern(canvas, "repeat");
    if (!pattern) return null;
    if (this.patterns.size >= MAX_PATTERNS) this.evictOldest(this.patterns);
    this.patterns.set(key, pattern);
    return pattern;
  }

  /** Drops every cached tile and pattern. */
  clear(): void {
    this.tiles.clear();
    this.patterns.clear();
  }

  private tile(scale: number): { size: number; heights: Uint8Array } | null {
    const k = Math.round(scale * 1000);
    const cached = this.tiles.get(k);
    if (cached) return cached;
    const size = this.source.grain_period_px(scale);
    const heights = this.source.bake_grain_tile(scale, size);
    if (heights.length !== size * size) return null;
    if (this.tiles.size >= MAX_TILES) this.evictOldest(this.tiles);
    const tile = { size, heights };
    this.tiles.set(k, tile);
    return tile;
  }

  private evictOldest<K>(map: Map<K, unknown>): void {
    const first = map.keys().next();
    if (!first.done) map.delete(first.value);
  }
}

function createCanvas(width: number, height: number): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}
