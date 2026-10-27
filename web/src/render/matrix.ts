/** 2D affine matrices in `CanvasRenderingContext2D.setTransform` order (`a b c d e f`). */
export interface Matrix {
  readonly a: number;
  readonly b: number;
  readonly c: number;
  readonly d: number;
  readonly e: number;
  readonly f: number;
}

export const IDENTITY: Matrix = { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 };

/** `m · n`: apply `n` first, then `m`. */
export function mul(m: Matrix, n: Matrix): Matrix {
  return {
    a: m.a * n.a + m.c * n.b,
    b: m.b * n.a + m.d * n.b,
    c: m.a * n.c + m.c * n.d,
    d: m.b * n.c + m.d * n.d,
    e: m.a * n.e + m.c * n.f + m.e,
    f: m.b * n.e + m.d * n.f + m.f,
  };
}

/** Page → device matrix of a view at `dpr`: `device = (page − origin) · scale · dpr`. */
export function viewMatrix(scale: number, originX: number, originY: number, dpr: number): Matrix {
  const s = scale * dpr;
  return { a: s, b: 0, c: 0, d: s, e: -originX * s, f: -originY * s };
}

/** Applies `m` to a point. */
export function apply(m: Matrix, x: number, y: number): [number, number] {
  return [m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f];
}

/** Uniform scale factor of `m` (length of its x axis). */
export function scaleOf(m: Matrix): number {
  return Math.hypot(m.a, m.b);
}

/** Sets `ctx`'s transform to `m`. */
export function setMatrix(ctx: CanvasRenderingContext2D, m: Matrix): void {
  ctx.setTransform(m.a, m.b, m.c, m.d, m.e, m.f);
}
