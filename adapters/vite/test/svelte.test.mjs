import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { transformSvelte } from '../svelte.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, 'fixtures');
const file = path.join(root, 'src', 'Counter.svelte');
const code = readFileSync(file, 'utf8');

test('Svelte: markup elements get the exact location', async () => {
  const result = await transformSvelte(code, file, root);
  assert.ok(result);
  assert.match(result.code, /<div data-pw-src="src\/Counter\.svelte:5:1" class="counter">/);
  assert.match(result.code, /<button data-pw-src="src\/Counter\.svelte:6:3" on:click=/);
  assert.equal(result.code.match(/data-pw-src/g)?.length, 2);
});

test('Svelte: components, svelte: elements and slots stay untouched', async () => {
  const result = await transformSvelte(code, file, root);
  assert.ok(result);
  assert.ok(!result.code.includes('<Counter data-pw-src'));
  assert.ok(!result.code.includes('<svelte:component data-pw-src'));
  assert.ok(!result.code.includes('<slot data-pw-src'));
});

test('Svelte: script and style blocks are byte-identical', async () => {
  const result = await transformSvelte(code, file, root);
  assert.ok(result);
  const script = (text) => text.slice(0, text.indexOf('</script>'));
  const style = (text) => text.slice(text.indexOf('<style'));
  assert.equal(script(result.code), script(code));
  assert.equal(style(result.code), style(code));
});

test('Svelte: a source map for the whole component is returned', async () => {
  const result = await transformSvelte(code, file, root);
  assert.ok(result);
  assert.equal(result.map.version, 3);
  assert.deepEqual(result.map.sources, [file]);
  assert.ok(result.map.mappings.length > 0);
});
