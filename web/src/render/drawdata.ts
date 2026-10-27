/**
 * Plain-array copy of a wasm `DrawList` (layout contract: `crates/penwise-wasm/src/draw_list.rs`).
 *
 * Reading a list copies each array out of wasm memory once, then frees the wasm object, so painting never
 * touches wasm and tests can build lists by hand.
 */

import type { DrawList } from "../wasm";

/** Floats per dab: `x, y, radius_x, radius_y, angle, opacity, hardness, grain_strength, grain_level`. */
export const DAB_STRIDE = 9;
/** Floats per style: `r, g, b, a, opacity, blend, grain_scale, grain_softness`. */
export const STYLE_STRIDE = 8;
/** u32s per command: `kind, first, count, style, aux`. */
export const COMMAND_STRIDE = 5;

/** Command kinds (`DrawKind` codes, frozen). */
export const Kind = {
  PathNonZero: 0,
  PathEvenOdd: 1,
  Dabs: 2,
  GroupBegin: 3,
  Mask: 4,
  GroupEnd: 5,
  Text: 6,
  Image: 7,
} as const;

/** One draw list as typed arrays. */
export interface DrawData {
  readonly points: Float32Array;
  readonly runs: Uint32Array;
  readonly dabs: Float32Array;
  readonly styles: Float32Array;
  readonly commands: Uint32Array;
  readonly maskWidths: Float32Array;
  readonly texts: readonly string[];
  readonly textSizes: Float32Array;
  readonly textBoxes: Float32Array;
  readonly images: readonly string[];
}

/** Nothing to draw. */
export const EMPTY_DRAW: DrawData = {
  points: new Float32Array(),
  runs: new Uint32Array(),
  dabs: new Float32Array(),
  styles: new Float32Array(),
  commands: new Uint32Array(),
  maskWidths: new Float32Array(),
  texts: [],
  textSizes: new Float32Array(),
  textBoxes: new Float32Array(),
  images: [],
};

/** One decoded command record. */
export interface Command {
  readonly kind: number;
  readonly first: number;
  readonly count: number;
  readonly style: number;
  readonly aux: number;
}

/** Copies `list` into typed arrays and frees it. */
export function readDrawList(list: DrawList): DrawData {
  const data: DrawData = {
    points: list.points(),
    runs: list.runs(),
    dabs: list.dabs(),
    styles: list.styles(),
    commands: list.commands(),
    maskWidths: list.mask_widths(),
    texts: list.texts(),
    textSizes: list.text_sizes(),
    textBoxes: list.text_boxes(),
    images: list.images(),
  };
  list.free();
  return data;
}

/** True when nothing would be drawn. */
export function isEmpty(data: DrawData): boolean {
  return data.commands.length === 0;
}

/** Number of commands. */
export function commandCount(data: DrawData): number {
  return data.commands.length / COMMAND_STRIDE;
}

/** Command `index` (callers stay below {@link commandCount}). */
export function commandAt(data: DrawData, index: number): Command {
  const o = index * COMMAND_STRIDE;
  return {
    kind: data.commands[o] ?? 0,
    first: data.commands[o + 1] ?? 0,
    count: data.commands[o + 2] ?? 0,
    style: data.commands[o + 3] ?? 0,
    aux: data.commands[o + 4] ?? 0,
  };
}

/** Style fields of record `index`. */
export interface Style {
  readonly r: number;
  readonly g: number;
  readonly b: number;
  readonly a: number;
  readonly opacity: number;
  readonly multiply: boolean;
  readonly grainScale: number;
  readonly grainSoftness: number;
}

/** Style record `index`. */
export function styleAt(data: DrawData, index: number): Style {
  const o = index * STYLE_STRIDE;
  const s = data.styles;
  return {
    r: s[o] ?? 0,
    g: s[o + 1] ?? 0,
    b: s[o + 2] ?? 0,
    a: s[o + 3] ?? 1,
    opacity: s[o + 4] ?? 1,
    multiply: (s[o + 5] ?? 0) === 1,
    grainScale: s[o + 6] ?? 0,
    grainSoftness: s[o + 7] ?? 0,
  };
}

/** `rgb(r g b)` CSS color of a style (straight sRGB `0..=1` → bytes). */
export function cssColor(style: Style): string {
  const byte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255);
  return `rgb(${byte(style.r)} ${byte(style.g)} ${byte(style.b)})`;
}

/** Points of run `run` as `[start, end)` indices into `points` (float offsets). */
export function runRange(data: DrawData, run: number): [number, number] {
  const startPoint = run === 0 ? 0 : (data.runs[run - 1] ?? 0);
  const endPoint = data.runs[run] ?? startPoint;
  return [startPoint * 2, endPoint * 2];
}
