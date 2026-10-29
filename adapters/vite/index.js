/**
 * `@penwise/vite-plugin`: the dev-only Vite adapter for source mapping (design-harness spec §7.3).
 *
 * `penwise link` adds `penwise()` to a project's Vite config. In `vite dev` every transformed module gets
 * `data-pw-src="<root-relative file>:<line>:<col>"` on its intrinsic elements, so a mark drawn on the page carries
 * the component source location. `vite build` never sees the plugin (`apply: 'serve'`).
 *
 * Supports:
 *   - JSX/TSX modules (React, Preact, Solid): every lowercase host element, components untouched;
 *   - Vue SFCs: plain HTML elements in the `<template>` block (`@vue/compiler-sfc` + `@vue/compiler-dom`);
 *   - Svelte components: markup elements (`svelte/compiler`).
 * Astro is not supported: its compiler owns the markup and no adapter is implemented in v1 (documented in the
 * user guide).
 */

import jsx from './jsx.cjs';
import source from './source.cjs';
import { transformSvelte } from './svelte.js';
import { transformVue } from './vue.js';

const { rootRelative } = source;

/** Modules that may contain JSX; `.d.ts` is excluded separately. */
const SCRIPT = /\.(jsx|tsx|js|ts|mjs)$/;

/**
 * @typedef {object} PenwiseOptions
 * @property {string} [root] project root; defaults to Vite's `config.root`
 */

/**
 * @param {PenwiseOptions} [options]
 * @returns {import('vite').Plugin}
 */
export default function penwise(options = {}) {
  const configured = options.root ? String(options.root) : null;
  let root = configured ?? process.cwd();

  return {
    name: 'penwise',
    apply: 'serve',
    enforce: 'pre',
    configResolved(config) {
      root = configured ?? config.root;
    },
    async transform(code, id) {
      // Vue/Svelte sub-requests (`App.vue?vue&type=template`) carry generated code, not the source file.
      if (id.includes('?')) return null;
      if (id.includes('/node_modules/')) return null;
      if (!rootRelative(root, id)) return null;
      if (id.endsWith('.vue')) return transformVue(code, id, root);
      if (id.endsWith('.svelte')) return transformSvelte(code, id, root);
      if (SCRIPT.test(id) && !id.endsWith('.d.ts')) return jsx.transformJsx(code, id, root);
      return null;
    },
  };
}
