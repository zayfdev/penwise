'use strict';

/**
 * `@penwise/next`: the dev-only Next.js adapter (design-harness spec §7.3).
 *
 * `withPenwise(nextConfig)` adds the JSX transform as a webpack `pre` loader and as a Turbopack rule. Both the
 * client and the server graph are covered: a Server Component's attributes end up in the HTML the browser and the
 * inspector see, so they are exactly what a mark needs. Production configs (`next build`, `NODE_ENV=production`)
 * are returned untouched.
 *
 * The loader itself is `loader.cjs`; it reuses the JSX transform of `@penwise/vite-plugin` so both adapters emit
 * one attribute format.
 */

const path = require('node:path');

const LOADER = path.join(__dirname, 'loader.cjs');

/** Files Next compiles that may contain JSX. */
const SOURCE_FILES = /\.(?:js|jsx|ts|tsx|mjs)$/;
/** Turbopack matches a pattern without `/` against the file name, in every directory. */
const TURBOPACK_FILES = ['*.js', '*.jsx', '*.ts', '*.tsx', '*.mjs'];

/**
 * @returns {boolean} whether Next is serving (`next dev`) rather than building
 */
function serving() {
  return process.env.NODE_ENV !== 'production';
}

/**
 * @param {Record<string, unknown>} config webpack config to mutate
 * @param {string} root project root
 * @returns {Record<string, unknown>}
 */
function addWebpack(config, root) {
  const module = { ...(config.module || {}) };
  const rules = Array.isArray(module.rules) ? [...module.rules] : [];
  rules.push({
    test: SOURCE_FILES,
    include: [root],
    exclude: [/node_modules/],
    enforce: 'pre',
    use: [{ loader: LOADER, options: { root } }],
  });
  return { ...config, module: { ...module, rules } };
}

/**
 * @param {string} root project root
 * @returns {{ loaders: { loader: string, options: { root: string } }[] }}
 */
function rule(root) {
  return { loaders: [{ loader: LOADER, options: { root } }] };
}

/**
 * Adds the loader for every JSX-bearing extension, keeping the user's own rules: an existing rule for the same
 * pattern gets our loader appended (ours only inserts an attribute, so it must run before the compiler).
 *
 * @param {Record<string, any> | undefined} turbopack
 * @param {string} root
 * @returns {Record<string, any>}
 */
function addTurbopack(turbopack, root) {
  const rules = { ...((turbopack && turbopack.rules) || {}) };
  for (const pattern of TURBOPACK_FILES) {
    const own = rule(root);
    const existing = rules[pattern];
    if (!existing) rules[pattern] = own;
    else if (Array.isArray(existing)) rules[pattern] = [...existing, own];
    else if (Array.isArray(existing.loaders)) {
      rules[pattern] = { ...existing, loaders: [...existing.loaders, ...own.loaders] };
    }
  }
  return { ...(turbopack || {}), rules };
}

/**
 * Wraps a Next config. Accepts every form Next does: a plain object, a sync/async function of `(phase, context)`,
 * or a promise.
 *
 * @template {object} T
 * @param {T | ((phase: string, context: object) => T | Promise<T>) | Promise<T>} [nextConfig]
 * @returns {T | ((phase: string, context: object) => Promise<T>) | Promise<T>}
 */
function withPenwise(nextConfig = {}) {
  if (typeof nextConfig === 'function') {
    return async function penwiseConfig(phase, context) {
      return withPenwise(await nextConfig(phase, context));
    };
  }
  if (nextConfig && typeof nextConfig.then === 'function') {
    return nextConfig.then((resolved) => withPenwise(resolved));
  }
  if (!serving()) return nextConfig;

  const root = process.cwd();
  const config = { ...nextConfig };
  const previous = config.webpack;
  config.webpack = (webpackConfig, context) => {
    if (context && context.dev === false) return webpackConfig;
    const resolved = previous ? previous(webpackConfig, context) : webpackConfig;
    return addWebpack(resolved || webpackConfig, root);
  };
  config.turbopack = addTurbopack(config.turbopack, root);
  return config;
}

module.exports = withPenwise;
module.exports.withPenwise = withPenwise;
module.exports.default = withPenwise;
