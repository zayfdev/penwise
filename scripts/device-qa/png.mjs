// PNG decode (8-bit RGB/RGBA, non-interlaced; what screencap emits) and pixel-region helpers, Node core only.
import { inflateSync } from 'node:zlib';

export function decode(buf) {
  let p = 8;
  let w = 0;
  let h = 0;
  let ct = 0;
  const idat = [];
  while (p < buf.length) {
    const len = buf.readUInt32BE(p);
    const type = buf.toString('ascii', p + 4, p + 8);
    const body = buf.subarray(p + 8, p + 8 + len);
    if (type === 'IHDR') {
      w = body.readUInt32BE(0);
      h = body.readUInt32BE(4);
      ct = body[9];
      if (body[8] !== 8 || body[12] !== 0 || (ct !== 2 && ct !== 6)) throw new Error('unsupported PNG');
    } else if (type === 'IDAT') idat.push(body);
    p += 12 + len;
  }
  const bpp = ct === 6 ? 4 : 3;
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * bpp;
  const out = Buffer.alloc(w * h * 4, 255);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? line[i - bpp] : 0;
      const b = prev[i];
      const c = i >= bpp ? prev[i - bpp] : 0;
      let add = 0;
      if (f === 1) add = a;
      else if (f === 2) add = b;
      else if (f === 3) add = (a + b) >> 1;
      else if (f === 4) {
        const pa = Math.abs(b - c);
        const pb = Math.abs(a - c);
        const pc = Math.abs(a + b - 2 * c);
        add = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      line[i] = (line[i] + add) & 255;
    }
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4;
      out[o] = line[x * bpp];
      out[o + 1] = line[x * bpp + 1];
      out[o + 2] = line[x * bpp + 2];
      if (bpp === 4) out[o + 3] = line[x * bpp + 3];
    }
    prev = line;
  }
  return { w, h, data: out };
}

const lum = (img, x, y) => {
  const o = (y * img.w + x) * 4;
  return img.data[o] * 0.299 + img.data[o + 1] * 0.587 + img.data[o + 2] * 0.114;
};

const inside = (img, x, y) => x >= 0 && y >= 0 && x < img.w && y < img.h;

/** Pixels whose colour differs by more than `tol` (sum of channels) between two same-size images, inside a rect. */
export function diffPixels(a, b, rect, tol = 90) {
  const [x0, y0, x1, y1] = rect ?? [0, 0, a.w, a.h];
  const hits = [];
  for (let y = Math.max(0, y0); y < Math.min(a.h, y1); y++) {
    for (let x = Math.max(0, x0); x < Math.min(a.w, x1); x++) {
      const o = (y * a.w + x) * 4;
      const d = Math.abs(a.data[o] - b.data[o]) + Math.abs(a.data[o + 1] - b.data[o + 1]) + Math.abs(a.data[o + 2] - b.data[o + 2]);
      if (d > tol) hits.push([x, y]);
    }
  }
  return hits;
}

export function bbox(points) {
  if (!points.length) return null;
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return [x0, y0, x1, y1];
}

/**
 * Where did the dark pixels `points` (taken from a screenshot) go in `img`? Tries vertical shifts within
 * [minDy, maxDy]; returns {dy, score} with score = fraction of points that are dark at the shifted position.
 */
export function findShift(img, points, minDy, maxDy, darkBelow = 90) {
  let best = { dy: 0, score: -1 };
  for (let dy = minDy; dy <= maxDy; dy++) {
    let hit = 0;
    for (const [x, y] of points) if (inside(img, x, y + dy) && lum(img, x, y + dy) < darkBelow) hit++;
    const score = hit / points.length;
    if (score > best.score) best = { dy, score };
  }
  return best;
}

/** Dark pixels (luminance below `darkBelow`) inside a rect. */
export function darkPixels(img, rect, darkBelow = 90) {
  const [x0, y0, x1, y1] = rect;
  const out = [];
  for (let y = Math.max(0, y0); y < Math.min(img.h, y1); y++)
    for (let x = Math.max(0, x0); x < Math.min(img.w, x1); x++) if (lum(img, x, y) < darkBelow) out.push([x, y]);
  return out;
}

/**
 * Vertical shift of page content between two screenshots: the dy (b row = a row + dy) with the smallest mean
 * absolute luminance difference over `rect` of `a`. Independent of what the content is.
 */
export function verticalShift(a, b, rect, minDy, maxDy) {
  const [x0, y0, x1, y1] = rect;
  let best = { dy: 0, err: Infinity };
  for (let dy = minDy; dy <= maxDy; dy++) {
    let sum = 0;
    let n = 0;
    for (let y = y0; y < y1; y += 2) {
      if (y + dy < 0 || y + dy >= b.h) continue;
      for (let x = x0; x < x1; x += 3) {
        sum += Math.abs(lum(a, x, y) - lum(b, x, y + dy));
        n++;
      }
    }
    if (n > 200 && sum / n < best.err) best = { dy, err: sum / n };
  }
  return best;
}
