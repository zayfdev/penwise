'use strict';

/**
 * The JSX half of the source contract (design-harness spec §7.3): add
 * `data-pw-src="<root-relative file>:<line>:<col>"` to intrinsic elements.
 *
 * The attribute goes only on lowercase host tags, never on components, and never when one is already present
 * (a second adapter pass over the same file must be a no-op). Offsets come from the parser, so the transform is
 * purely a text insertion: formatting, comments and Babel's own output are untouched, and the source map maps
 * every original character back to `<root-relative file>:<line>:<col>` of the input.
 */

const { parse } = require('@babel/parser');
const MagicString = require('magic-string');

const { attribute, locator, rootRelative, walk } = require('./source.cjs');

const JSX_FILE = /\.(jsx|tsx|js|ts|mjs)$/;
const TS_FILE = /\.tsx?$/;

/**
 * @param {string} file
 * @returns {string[]} @babel/parser plugins
 */
function pluginsFor(file) {
  // A `.ts` file cannot contain JSX, so leave the JSX plugin off there: `<T>x` must stay a type assertion.
  if (TS_FILE.test(file)) return file.endsWith('.tsx') ? ['typescript', 'jsx'] : ['typescript'];
  return ['jsx'];
}

/**
 * @param {any} name JSXOpeningElement's name
 * @returns {boolean}
 */
function intrinsic(name) {
  return name && name.type === 'JSXIdentifier' && /^[a-z]/.test(name.name);
}

/**
 * @param {any} opening
 * @returns {boolean}
 */
function tagged(opening) {
  return opening.attributes.some(
    (item) =>
      item.type === 'JSXAttribute' &&
      item.name &&
      item.name.type === 'JSXIdentifier' &&
      item.name.name === 'data-pw-src',
  );
}

/**
 * Transforms one JSX/TSX module.
 *
 * @param {string} code
 * @param {string} id absolute file path, with or without a query
 * @param {string} root project root
 * @returns {{ code: string, map: import('magic-string').SourceMap } | null} `null` when nothing changed
 */
function transformJsx(code, id, root) {
  const file = String(id).split('?')[0];
  if (!JSX_FILE.test(file) || file.includes('/node_modules/')) return null;
  if (!rootRelative(root, file)) return null;

  let ast;
  try {
    ast = parse(code, {
      sourceType: 'module',
      plugins: pluginsFor(file),
      errorRecovery: true,
    });
  } catch {
    return null;
  }
  if (ast.errors && ast.errors.length > 0) return null;

  const stream = new MagicString(code);
  const at = locator(code);
  let changed = false;
  walk(ast.program, (node) => {
    if (node.type !== 'JSXOpeningElement' || !intrinsic(node.name) || tagged(node)) return;
    const text = attribute(root, file, at(node.start));
    if (!text) return;
    stream.appendLeft(node.name.end, text);
    changed = true;
  });
  if (!changed) return null;
  return {
    code: stream.toString(),
    map: stream.generateMap({ source: file, includeContent: true, hires: true }),
  };
}

module.exports = { transformJsx };
