import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { transformVue } from '../vue.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, 'fixtures');
const file = path.join(root, 'src', 'Widget.vue');
const code = readFileSync(file, 'utf8');

test('Vue: plain elements in the template get the exact location', async () => {
  const result = await transformVue(code, file, root);
  assert.ok(result);
  assert.match(result.code, /<div data-pw-src="src\/Widget\.vue:2:3" class="wrap">/);
  assert.match(result.code, /<button data-pw-src="src\/Widget\.vue:4:5" @click="go">/);
  assert.match(result.code, /<span data-pw-src="src\/Widget\.vue:5:25">ok<\/span>/);
  assert.equal(result.code.match(/data-pw-src/g)?.length, 3);
});

test('Vue: components, slots and nested <template> stay untouched', async () => {
  const result = await transformVue(code, file, root);
  assert.ok(result);
  assert.ok(!result.code.includes('<MyButton data-pw-src'));
  assert.ok(!result.code.includes('<slot data-pw-src'));
  assert.ok(!result.code.includes('<template data-pw-src'));
  assert.ok(result.code.includes('<template v-if="ok">'));
});

test('Vue: script and style blocks are byte-identical', async () => {
  const result = await transformVue(code, file, root);
  assert.ok(result);
  const script = (text) => text.slice(text.indexOf('<script'));
  assert.equal(script(result.code), script(code));
});

test('Vue: a template-less SFC (render function only) is skipped', async () => {
  const renderOnly = '<script>\nexport default { render: () => null };\n</script>\n';
  assert.equal(await transformVue(renderOnly, file, root), null);
});

test('Vue: a source map for the whole SFC is returned', async () => {
  const result = await transformVue(code, file, root);
  assert.ok(result);
  assert.equal(result.map.version, 3);
  assert.deepEqual(result.map.sources, [file]);
  assert.ok(result.map.mappings.length > 0);
});
