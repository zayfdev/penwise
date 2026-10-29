'use strict';

/**
 * The webpack/Turbopack loader behind `withPenwise`. Synchronous by design: Turbopack's loader runtime offers only
 * a subset of the webpack loader API and synchronous loaders are the supported shape.
 *
 * The project root is the `root` option `index.cjs` passes (the loader runs in the Next process, where
 * `process.cwd()` is the project); `rootContext` is the fallback for a hand-wired webpack config.
 */

const { transformJsx } = require('../vite/jsx.cjs');

/**
 * @this {any} webpack/Turbopack loader context
 * @param {string} content
 * @param {object | undefined} inputMap
 * @returns {string | undefined}
 */
module.exports = function penwiseLoader(content, inputMap) {
  if (this && typeof this.cacheable === 'function') this.cacheable();
  const options =
    this && typeof this.getOptions === 'function'
      ? this.getOptions() || {}
      : (this && this.options) || {};
  const root = options.root || (this && this.rootContext) || process.cwd();
  const resource = this && this.resourcePath ? String(this.resourcePath) : '';

  const result = transformJsx(String(content), resource, root);
  if (!result) return content;

  if (this && typeof this.callback === 'function') {
    this.callback(null, result.code, JSON.parse(result.map.toString()), inputMap);
    return undefined;
  }
  return result.code;
};
