import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import penwise from '../index.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, 'fixtures');

const plugin = (extra = {}) => {
  const instance = penwise({ root, ...extra });
  instance.configResolved({ root });
  return instance;
};

test('the plugin is dev-only and runs before the framework plugins', () => {
  const instance = penwise();
  assert.equal(instance.name, 'penwise');
  assert.equal(instance.apply, 'serve');
  assert.equal(instance.enforce, 'pre');
});

test('Vite ids with a query (generated sub-modules) are ignored', async () => {
  const instance = plugin();
  assert.equal(await instance.transform('const a = 1;', `${root}/src/Card.jsx?vue&type=script`), null);
});

test('files outside the root are ignored', async () => {
  const instance = plugin();
  assert.equal(await instance.transform('const a = <div />;', '/elsewhere/Card.jsx'), null);
});

test('the transform dispatches on the extension', async () => {
  const instance = plugin();
  const jsx = await instance.transform('<div />', `${root}/src/Card.jsx`);
  assert.ok(jsx?.code.includes('data-pw-src='));
  const vue = await instance.transform('<template><div /></template>', `${root}/src/Widget.vue`);
  assert.ok(vue?.code.includes('data-pw-src="src/Widget.vue:1:11"'));
  const css = await instance.transform('.a { color: red }', `${root}/src/app.css`);
  assert.equal(css, null);
});

test('the configured root wins over config.root', async () => {
  const instance = penwise({ root });
  instance.configResolved({ root: '/somewhere/else' });
  const result = await instance.transform('<div />', `${root}/src/Card.jsx`);
  assert.ok(result?.code.includes('data-pw-src="src/Card.jsx:1:1"'));
});
