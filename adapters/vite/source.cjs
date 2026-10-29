'use strict';

/**
 * Shared helpers for the Penwise dev adapters: 1-based line/column lookup over an original file, root-relative
 * paths, and a generic AST walk.
 *
 * CommonJS on purpose: the Next loader (`adapters/next/loader.cjs`) must be loadable synchronously by webpack and
 * Turbopack, and it reuses the JSX transform in `adapters/vite/jsx.cjs`.
 */

const path = require('node:path');

/** Keys that hold metadata or back-references, never children to visit. */
const SKIPPED_KEYS = new Set([
  'loc',
  'start',
  'end',
  'range',
  'extra',
  'parent',
  'tokens',
  'comments',
  'leadingComments',
  'trailingComments',
  'innerComments',
]);

/**
 * `file` relative to `root` with `/` separators, or `null` when it lies outside the project.
 *
 * @param {string} root
 * @param {string} file
 * @returns {string | null}
 */
function rootRelative(root, file) {
  if (!root || !path.isAbsolute(file)) return null;
  const relative = path.relative(path.resolve(root), file);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) return null;
  return relative.split(path.sep).join('/');
}

/**
 * Returns a function mapping a UTF-16 offset in `text` to `{ line, column }`, both 1-based.
 *
 * @param {string} text
 * @returns {(offset: number) => { line: number, column: number }}
 */
function locator(text) {
  const starts = [0];
  for (let index = 0; index < text.length; index += 1) {
    if (text.charCodeAt(index) === 10) starts.push(index + 1);
  }
  return (offset) => {
    const clamped = Math.max(0, Math.min(offset, text.length));
    let low = 0;
    let high = starts.length - 1;
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (starts[middle] <= clamped) low = middle;
      else high = middle - 1;
    }
    return { line: low + 1, column: clamped - starts[low] + 1 };
  };
}

/**
 * The attribute to insert: ` data-pw-src="<file>:<line>:<col>"`, or `null` outside the project.
 *
 * @param {string} root
 * @param {string} file absolute path
 * @param {{ line: number, column: number }} position
 * @returns {string | null}
 */
function attribute(root, file, position) {
  const relative = rootRelative(root, file);
  if (!relative) return null;
  return ` data-pw-src="${relative}:${position.line}:${position.column}"`;
}

/**
 * Depth-first walk over an AST. Visits every node that has a string `type`; arrays and plain objects are
 * descended, metadata keys are skipped. Safe on cyclic ASTs (`parent` is skipped).
 *
 * @param {unknown} node
 * @param {(node: any) => void} visit
 */
function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (Array.isArray(node)) {
    for (const child of node) walk(child, visit);
    return;
  }
  if (typeof node.type === 'string' || typeof node.type === 'number') visit(node);
  for (const key of Object.keys(node)) {
    if (SKIPPED_KEYS.has(key)) continue;
    const value = node[key];
    if (value && typeof value === 'object') walk(value, visit);
  }
}

module.exports = { attribute, locator, rootRelative, walk };
