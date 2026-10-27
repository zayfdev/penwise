/**
 * Paper grain height: a line-for-line port of core `stamp::texture::paper_height` (procedural two-octave value
 * noise in page space), so dab grain lands on the same page pixels as the server and tablet renders. No wasm and
 * no baked tile: the lattice (a `SplitMix64` hash per cell) is computed once, lazily, in 32-bit halves.
 */

/** Lattice cells per tile edge; the noise repeats every `PAPER_PERIOD · PAPER_CELL_PX · scale` page px. */
const PAPER_PERIOD = 256;
/** Lattice cell size at scale 1, px. */
const PAPER_CELL_PX = 2;
/** `PAPER_SEED` of core, as `[hi, lo]` 32-bit halves. */
const SEED_HI = 0x70617065;
const SEED_LO = 0x725f7631;

/** `a · b` as `[hi, lo]` 32-bit halves of the 64-bit product of two u32. */
function mulWide(a: number, b: number): [number, number] {
  const a0 = a & 0xffff;
  const a1 = a >>> 16;
  const b0 = b & 0xffff;
  const b1 = b >>> 16;
  const mid = a0 * b1 + a1 * b0 + ((a0 * b0) >>> 16);
  const hi = (a1 * b1 + Math.floor(mid / 65536)) >>> 0;
  return [hi, Math.imul(a, b) >>> 0];
}

/** 64-bit wrapping `z · (chi:clo)` on `[hi, lo]` halves. */
function mul64(hi: number, lo: number, chi: number, clo: number): [number, number] {
  const [ph, pl] = mulWide(lo, clo);
  return [(ph + Math.imul(lo, chi) + Math.imul(hi, clo)) >>> 0, pl];
}

/** Top 24 bits of `mix64` (core `prng::mix64`, the `SplitMix64` finalizer) of the 64-bit key `hi:lo`. */
function mixTop24(hi: number, lo: number): number {
  // z ^= z >> 30
  let l = (lo ^ ((lo >>> 30) | (hi << 2))) >>> 0;
  let h = (hi ^ (hi >>> 30)) >>> 0;
  [h, l] = mul64(h, l, 0xbf58476d, 0x1ce4e5b9);
  // z ^= z >> 27
  l = (l ^ ((l >>> 27) | (h << 5))) >>> 0;
  h = (h ^ (h >>> 27)) >>> 0;
  [h, l] = mul64(h, l, 0x94d049bb, 0x133111eb);
  // z ^ (z >> 31), then >> 40 keeps the top 24 bits of the high half
  h = (h ^ (h >>> 31)) >>> 0;
  return h >>> 8;
}

/** Lattice of octave `o` (`period = 256 << o` cells per side): value `(mix64(key) >> 40) · 2⁻²⁴` per cell. */
function bakeLattice(o: number): Float32Array {
  const period = PAPER_PERIOD << o;
  const table = new Float32Array(period * period);
  for (let i = 0; i < period; i++) {
    for (let j = 0; j < period; j++) {
      // key = SEED ^ (o << 62) ^ (i << 31) ^ j
      const hi = (SEED_HI ^ (o << 30) ^ (i >>> 1)) >>> 0;
      const lo = (SEED_LO ^ ((i & 1) << 31) ^ j) >>> 0;
      table[i * period + j] = mixTop24(hi, lo) / 16777216;
    }
  }
  return table;
}

const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));
const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;

/** Paper height field shared by every dab stroke of a view. */
export class PaperGrain {
  private lattices: [Float32Array, Float32Array] | null = null;

  /** Core `paper_height(x, y, scale)` at page position `(x, y)`, `∈ [0, 1]`; 0.5 for a degenerate input. */
  height(x: number, y: number, scale: number): number {
    if (!(Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(scale)) || scale <= 0) {
      return 0.5;
    }
    this.lattices ??= [bakeLattice(0), bakeLattice(1)];
    const tables = this.lattices;
    const cell = PAPER_CELL_PX * scale;
    const coarse = octave(tables[0], x, y, cell, 0);
    const fine = octave(tables[1], x, y, cell * 0.5, 1);
    if (coarse === null || fine === null) return 0.5;
    return clamp01((coarse + 0.5 * fine) / 1.5);
  }
}

/** One value-noise octave with lattice period `256 << o` cells (smoothstep-weighted bilinear); `null` on overflow. */
function octave(table: Float32Array, x: number, y: number, cell: number, o: number): number | null {
  const gx = x / cell;
  const gy = y / cell;
  if (!(Number.isFinite(gx) && Number.isFinite(gy))) return null;
  const fx = Math.floor(gx);
  const fy = Math.floor(gy);
  const tx = smoothWeight(gx - fx);
  const ty = smoothWeight(gy - fy);
  const period = PAPER_PERIOD << o;
  const i0 = ((fx % period) + period) % period;
  const j0 = ((fy % period) + period) % period;
  const i1 = (i0 + 1) % period;
  const j1 = (j0 + 1) % period;
  const at = (i: number, j: number): number => table[i * period + j] ?? 0;
  const top = lerp(at(i0, j0), at(i1, j0), tx);
  const bottom = lerp(at(i0, j1), at(i1, j1), tx);
  return lerp(top, bottom, ty);
}

function smoothWeight(t: number): number {
  return t * t * (3 - 2 * t);
}
