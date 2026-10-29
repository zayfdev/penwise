'use strict';

const assert = require('node:assert/strict');
const { existsSync } = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const withPenwise = require('../index.cjs');
const loader = require('../loader.cjs');

const LOADER = path.join(__dirname, '..', 'loader.cjs');

/** Runs `body` with NODE_ENV forced, restoring it afterwards. */
function withEnv(value, body) {
  const previous = process.env.NODE_ENV;
  process.env.NODE_ENV = value;
  try {
    return body();
  } finally {
    if (previous === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = previous;
  }
}

test('dev: adds a pre webpack loader for JSX-bearing files', () => {
  const config = withEnv('development', () => withPenwise({ reactStrictMode: true }));
  assert.equal(typeof config.webpack, 'function');
  assert.equal(config.reactStrictMode, true);

  const webpack = config.webpack({ module: { rules: [] } }, { dev: true });
  assert.equal(webpack.module.rules.length, 1);
  const rule = webpack.module.rules[0];
  assert.equal(rule.enforce, 'pre');
  assert.ok(rule.test.test('/proj/app/page.tsx'));
  assert.ok(rule.test.test('/proj/app/page.js'));
  assert.ok(rule.test.test('/proj/app/page.mjs'));
  assert.ok(!rule.test.test('/proj/app/logo.svg'));
  assert.deepEqual(rule.include, [process.cwd()]);
  assert.deepEqual(rule.use, [{ loader: LOADER, options: { root: process.cwd() } }]);
  assert.ok(existsSync(rule.use[0].loader), 'the loader file must exist');
});

test('dev: the previous webpack hook still runs and its config is kept', () => {
  const previous = (config) => ({ ...config, marker: 'kept' });
  const config = withEnv('development', () => withPenwise({ webpack: previous }));
  const out = config.webpack({ module: { rules: [] } }, { dev: true });
  assert.equal(out.marker, 'kept');
  assert.equal(out.module.rules.length, 1);
});

test('dev: turbopack rules are added and the user rules are preserved', () => {
  const existing = { '*.tsx': { loaders: ['other-loader'], as: '*.js' } };
  const config = withEnv('development', () => withPenwise({ turbopack: { rules: existing } }));
  const rules = config.turbopack.rules;
  for (const pattern of ['*.js', '*.jsx', '*.ts', '*.tsx', '*.mjs']) {
    assert.ok(rules[pattern], `${pattern} must have a rule`);
  }
  assert.equal(rules['*.tsx'].loaders.length, 2);
  assert.equal(rules['*.tsx'].loaders[0], 'other-loader');
  assert.equal(rules['*.tsx'].as, '*.js');
  assert.deepEqual(rules['*.jsx'].loaders, [{ loader: LOADER, options: { root: process.cwd() } }]);
  assert.equal(config.turbopack.rules['*.css'], undefined);
});

test('dev: the loader appends a rule without dropping Next rules', () => {
  const next = { module: { rules: [{ test: /\.svg$/, use: ['svg'] }] } };
  const config = withEnv('development', () => withPenwise({}));
  const out = config.webpack(next, { dev: true });
  assert.equal(out.module.rules.length, 2);
  assert.equal(out.module.rules[0].test.source, '\\.svg$');
});

test('production: the config comes back untouched', () => {
  const original = { reactStrictMode: true };
  const config = withEnv('production', () => withPenwise(original));
  assert.equal(config, original);
  assert.equal(config.webpack, undefined);
  assert.equal(config.turbopack, undefined);
});

test('build phase: webpack is skipped when context.dev is false', () => {
  const config = withEnv('development', () => withPenwise({}));
  const out = config.webpack({ module: { rules: [] } }, { dev: false });
  assert.equal(out.module.rules.length, 0);
});

test('a function config is wrapped and resolved', async () => {
  const wrapped = withEnv('development', () =>
    withPenwise(async (phase) => ({ phase, reactStrictMode: true })),
  );
  const config = await wrapped('phase-development-server', {});
  assert.equal(config.phase, 'phase-development-server');
  assert.equal(config.reactStrictMode, true);
  assert.equal(typeof config.webpack, 'function');
});

test('a promise config is wrapped', async () => {
  const wrapped = withEnv('development', () => withPenwise(Promise.resolve({ reactStrictMode: true })));
  const config = await wrapped;
  assert.equal(typeof config.webpack, 'function');
});

test('the ESM entry exports the same function', async () => {
  const esm = await import('../index.mjs');
  assert.equal(typeof esm.withPenwise, 'function');
  assert.equal(esm.default, esm.withPenwise);
});

test('loader: inserts data-pw-src and returns a source map', () => {
  const calls = [];
  const context = {
    resourcePath: '/proj/app/Page.tsx',
    rootContext: '/proj',
    cacheable() {},
    getOptions: () => ({ root: '/proj' }),
    callback(...args) {
      calls.push(args);
    },
  };
  const source = 'export default function Page() {\n  return <main>hi</main>;\n}\n';
  loader.call(context, source);
  assert.equal(calls.length, 1);
  const [error, code, map] = calls[0];
  assert.equal(error, null);
  assert.match(code, /<main data-pw-src="app\/Page\.tsx:2:10">/);
  assert.equal(map.version, 3);
  assert.deepEqual(map.sources, ['/proj/app/Page.tsx']);
});

test('loader: falls back to rootContext, and leaves untouched code alone', () => {
  const calls = [];
  const context = {
    resourcePath: '/proj/app/Page.jsx',
    rootContext: '/proj',
    cacheable() {},
    callback(...args) {
      calls.push(args);
    },
  };
  const untouched = 'export default function Page() {\n  return <Widget />;\n}\n';
  assert.equal(loader.call(context, untouched), untouched);
  assert.equal(calls.length, 0);
});

test('loader: without a callback it returns the transformed code', () => {
  const context = { resourcePath: '/proj/app/Page.jsx', rootContext: '/proj' };
  const out = loader.call(context, '<main />\n');
  assert.equal(out, '<main data-pw-src="app/Page.jsx:1:1" />\n');
});
