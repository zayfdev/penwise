import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer as createNetServer } from 'node:net';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { createServer as createViteServer } from 'vite';

import penwise from '../index.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.dirname(here);

/** An ephemeral loopback port, never in Penwise's reserved 7420–7430 range. */
function freePort() {
  return new Promise((resolve, reject) => {
    const server = createNetServer();
    server.unref();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => (port >= 7420 && port <= 7430 ? resolve(freePort()) : resolve(port)));
    });
  });
}

function fixture(root) {
  mkdirSync(path.join(root, 'src'), { recursive: true });
  writeFileSync(
    path.join(root, 'index.html'),
    '<div id="root"></div><script type="module" src="/src/main.jsx"></script>\n',
  );
  writeFileSync(
    path.join(root, 'src', 'main.jsx'),
    "import { App } from './App.jsx';\n\nconsole.log(App);\n",
  );
  writeFileSync(
    path.join(root, 'src', 'Button.jsx'),
    "export function Button() {\n  return <button type=\"button\">x</button>;\n}\n",
  );
  writeFileSync(
    path.join(root, 'src', 'App.jsx'),
    [
      "import { useState } from 'react';",
      "import { Button } from './Button.jsx';",
      '',
      'export function App() {',
      '  const [count] = useState(0);',
      '  return (',
      '    <main className="app">',
      '      <h1>Hello {count}</h1>',
      '      <Button />',
      '    </main>',
      '  );',
      '}',
      '',
    ].join('\n'),
  );
}

test('a real Vite dev server serves modules with data-pw-src', async (t) => {
  const root = mkdtempSync(path.join(packageRoot, '.tmp-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  fixture(root);
  const port = await freePort();
  const server = await createViteServer({
    configFile: false,
    root,
    logLevel: 'silent',
    server: {
      host: '127.0.0.1',
      port,
      strictPort: true,
      fs: { allow: [packageRoot] },
    },
    plugins: [penwise()],
  });
  t.after(() => server.close());
  await server.listen();

  const response = await fetch(`http://127.0.0.1:${port}/src/App.jsx`);
  assert.equal(response.status, 200);
  const body = await response.text();
  assert.ok(body.includes('data-pw-src'), 'the transformed module carries the attribute');
  assert.ok(body.includes('"src/App.jsx:7:5"'), '<main> maps to its component location');
  assert.ok(body.includes('"src/App.jsx:8:7"'), '<h1> maps to its component location');
  assert.ok(!body.includes('Button data-pw-src'), 'the <Button /> component is not tagged');
  assert.ok(body.includes('src/Button.jsx'), 'the component import is still rewritten');
});
