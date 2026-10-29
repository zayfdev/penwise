import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import jsx from '../jsx.cjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, 'fixtures');
const fixture = (name) => path.join(root, 'src', name);
const read = (name) => readFileSync(fixture(name), 'utf8');

const { transformJsx } = jsx;

test('JSX: intrinsic elements get the exact file:line:col, components do not', () => {
  const result = transformJsx(read('Card.jsx'), fixture('Card.jsx'), root);
  assert.ok(result, 'the fixture must change');
  assert.match(result.code, /<section data-pw-src="src\/Card\.jsx:5:5" className="card">/);
  assert.match(result.code, /<h2 data-pw-src="src\/Card\.jsx:6:7">/);
  assert.match(result.code, /<my-el data-pw-src="src\/Card\.jsx:8:7" \/>/);
  assert.match(result.code, /<img data-pw-src="src\/Card\.jsx:9:7" src=\{title\} \/>/);
  assert.ok(!result.code.includes('<Widget data-pw-src'));
  assert.equal(result.code.match(/data-pw-src/g)?.length, 4);
});

test('TSX: TypeScript syntax parses and typed elements are tagged', () => {
  const result = transformJsx(read('Badge.tsx'), fixture('Badge.tsx'), root);
  assert.ok(result);
  assert.match(result.code, /<span data-pw-src="src\/Badge\.tsx:8:5" className="badge"/);
});

test('the source map points at the original file', () => {
  const result = transformJsx(read('Card.jsx'), fixture('Card.jsx'), root);
  assert.ok(result);
  assert.equal(result.map.version, 3);
  assert.deepEqual(result.map.sources, [fixture('Card.jsx')]);
  assert.equal(result.map.sourcesContent?.[0], read('Card.jsx'));
  assert.ok(result.map.mappings.length > 0);
});

test('an existing data-pw-src is never duplicated', () => {
  assert.equal(transformJsx(read('Already.jsx'), fixture('Already.jsx'), root), null);
});

test('a component-only module is left alone', () => {
  assert.equal(transformJsx(read('ComponentsOnly.jsx'), fixture('ComponentsOnly.jsx'), root), null);
});

test('.ts angle-bracket assertions are not JSX', () => {
  assert.equal(transformJsx(read('Assertion.ts'), fixture('Assertion.ts'), root), null);
});

test('a file outside the project root is skipped', () => {
  assert.equal(transformJsx(read('Card.jsx'), '/elsewhere/Card.jsx', root), null);
});

test('a node_modules file is skipped', () => {
  const file = path.join(root, 'node_modules', 'pkg', 'index.jsx');
  assert.equal(transformJsx('export const a = () => <div />;', file, root), null);
});
