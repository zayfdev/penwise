/**
 * Svelte half of the source contract: add `data-pw-src` to markup elements before the Svelte plugin compiles the
 * component, so the compiler emits it as an ordinary attribute.
 *
 * `svelte/compiler`'s `parse` runs on the untouched component text and reports offsets into it; the attribute is
 * inserted with `magic-string`, leaving `<script>` and `<style>` blocks (and the expression syntax) alone.
 * `svelte` is an optional dependency: the compiler is only loaded for `.svelte` files.
 */

import MagicString from 'magic-string';

import source from './source.cjs';

const { attribute, locator, rootRelative, walk } = source;

/** Legacy (`Element`) and Svelte 5 (`RegularElement`) AST names for a host element. */
const ELEMENT_TYPES = new Set(['Element', 'RegularElement']);
/** Lowercase tags only: `<Counter>` is a component node and never reaches this test. */
const HOST_TAG = /^[a-z][a-z0-9-]*$/;

/**
 * @param {any} element
 * @returns {boolean}
 */
function tagged(element) {
  const attributes = element.attributes || [];
  return attributes.some((item) => item.type === 'Attribute' && item.name === 'data-pw-src');
}

/**
 * @param {string} code
 * @param {string} id
 * @param {string} root
 * @returns {Promise<{ code: string, map: import('magic-string').SourceMap } | null>}
 */
export async function transformSvelte(code, id, root) {
  const file = id.split('?')[0];
  if (!rootRelative(root, file)) return null;

  const { parse } = await import('svelte/compiler');
  let ast;
  try {
    ast = parse(code, { modern: true });
  } catch {
    return null;
  }

  const stream = new MagicString(code);
  const at = locator(code);
  let changed = false;
  walk(ast.fragment ?? ast.html, (node) => {
    if (!ELEMENT_TYPES.has(node.type)) return;
    if (typeof node.name !== 'string' || !HOST_TAG.test(node.name) || tagged(node)) return;
    const text = attribute(root, file, at(node.start));
    if (!text) return;
    stream.appendLeft(node.start + 1 + node.name.length, text);
    changed = true;
  });
  if (!changed) return null;
  return {
    code: stream.toString(),
    map: stream.generateMap({ source: file, includeContent: true, hires: true }),
  };
}
