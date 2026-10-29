/**
 * Vue SFC half of the source contract: add `data-pw-src` to plain HTML elements in the `<template>` block.
 *
 * `@vue/compiler-sfc` finds the block (and its exact content offset); `@vue/compiler-dom` parses only the template
 * markup, so the offsets it reports are stable regardless of the `<script>` and `<style>` blocks. The attribute is
 * inserted into the original SFC text, so the SFC compiler sees an ordinary attribute and the map stays exact.
 *
 * Both compilers are optional dependencies: a React-only project never loads them.
 */

import MagicString from 'magic-string';

import source from './source.cjs';

const { attribute, locator, rootRelative, walk } = source;

/** `@vue/compiler-core`'s `NodeTypes.ELEMENT`. */
const NODE_ELEMENT = 1;
/** `ElementTypes.ELEMENT`: a plain HTML tag, not a component, slot or nested template. */
const TAG_ELEMENT = 0;
/** A plain HTML tag; components (`<MyThing>`), slots and nested `<template>` never qualify. */
const HOST_TAG = /^[a-z][a-z0-9-]*$/;
/** Compiler built-ins that are resolved as components despite the lowercase spelling. */
const BUILT_INS = new Set([
  'component',
  'keep-alive',
  'slot',
  'suspense',
  'teleport',
  'transition',
  'transition-group',
  'template',
]);
/** `@vue/compiler-core`'s `NodeTypes.ATTRIBUTE`. */
const ATTRIBUTE = 6;

/**
 * @param {any} element
 * @returns {boolean}
 */
function tagged(element) {
  return element.props.some((prop) => prop.type === ATTRIBUTE && prop.name === 'data-pw-src');
}

/**
 * @param {string} code
 * @param {string} id
 * @param {string} root
 * @returns {Promise<{ code: string, map: import('magic-string').SourceMap } | null>}
 */
export async function transformVue(code, id, root) {
  const file = id.split('?')[0];
  if (!rootRelative(root, file)) return null;

  const [{ parse: parseSfc }, { parse: parseTemplate }] = await Promise.all([
    import('@vue/compiler-sfc'),
    import('@vue/compiler-dom'),
  ]);

  const { descriptor, errors } = parseSfc(code, { filename: file });
  if (errors.length > 0 || !descriptor.template || descriptor.template.src) return null;
  const block = descriptor.template;
  // `lang="pug"` (or any other preprocessor) has no HTML text to locate elements in.
  if (block.lang && block.lang !== 'html') return null;
  const base = block.loc.start.offset;
  if (code.slice(base, base + block.content.length) !== block.content) return null;

  const templateErrors = [];
  let ast;
  try {
    ast = parseTemplate(block.content, {
      onError: (error) => templateErrors.push(error),
      onWarn: () => {},
    });
  } catch {
    return null;
  }
  if (templateErrors.length > 0) return null;

  const stream = new MagicString(code);
  const at = locator(code);
  let changed = false;
  walk(ast, (node) => {
    if (node.type !== NODE_ELEMENT || node.tagType !== TAG_ELEMENT) return;
    if (!HOST_TAG.test(node.tag) || BUILT_INS.has(node.tag) || tagged(node)) return;
    const text = attribute(root, file, at(base + node.loc.start.offset));
    if (!text) return;
    stream.appendLeft(base + node.loc.start.offset + 1 + node.tag.length, text);
    changed = true;
  });
  if (!changed) return null;
  return {
    code: stream.toString(),
    map: stream.generateMap({ source: file, includeContent: true, hires: true }),
  };
}
