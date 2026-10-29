# Penwise design harness — user guide

Draw on a live page from the tablet. The agent reads your ink, proposes a change on the canvas, and edits the
code after you pick. This guide covers setup, the daily loop, the agent's tools, and what does not work yet.
The design is in `docs/design-harness-spec.md`; the agent interface in `docs/mcp.md`; the prompt to paste into
an agent in `docs/agent-prompt.md`; known gaps in `docs/next-iteration.md`.

## Setup once

### PC

1. Install the app, the CLI and the agent server:

   ```
   cd ~/workspace/penwise
   just install-desktop
   ```

   This builds release binaries, installs `penwise`, `penwise-server` and `penwise-mcp` in `~/.local/bin`, and
   starts `penwise.service` as a login service. The server listens on port 7420.

2. Allow the tablet to reach the PC. Open the app ports in the firewall:

   ```
   sudo ufw allow 7420:7430/tcp
   ```

   Linked projects take ports from 7421 up. They must be reachable too. Use plain LAN or Tailscale; do not
   expose these ports to the internet.

3. Open the **Penwise** window from the app menu, or run:

   ```
   penwise
   ```

### Tablet

1. Put the tablet on the same network, or connect Tailscale.
2. Open Penwise and open the sync settings.
3. Enter the PC server address, and the pairing code in the access-token field. A six-character code there is
   exchanged for the real token. The code and QR are in the PC window's **Pair device** panel. The camera app
   opens the QR link directly. The code lasts 10 minutes.
4. Wait for the sync dot to turn green.

## Link a project

The Penwise server must be running. `penwise` opens the window and starts it; in the repo, `just serve` does
the same. The CLI pairs over loopback, so no token is copied.

Run the CLI in the project folder, once:

```
cd ~/src/my-site
penwise link
```

The CLI reads the folder and prints what it found: a static site, or a dev server for Vite, Next, Astro or
SvelteKit. Then it asks:

1. **`register with Penwise? [Y/n]`** — adds the project to the server and writes `.penwise.json` in the
   project root. Linking the same root again updates the entry; it does not duplicate it.
2. **`apply the adapter to <config>? [y/N]`** — for Vite (including SvelteKit) and Next only. The CLI prints
   the exact config diff first: the import, the plugin entry, and a `devDependencies` line
   (`"@penwise/vite-plugin": "file:<repo>/adapters/vite"` or `"@penwise/next": "file:<repo>/adapters/next"`).
   It changes the config only after you say yes. Then run `npm install` in the project to link the adapter
   package.
3. **`write the MCP config? [Y/n]`** — writes `.mcp.json` for Claude Code and `.omp/mcp.json` for omp in the
   project root. Both hold the same `penwise` entry; other servers in those files are kept. A broken existing
   file is reported and left untouched. A write failure is reported and does not undo the link.

Flags:

- `penwise link --yes` or `-y` — accept every prompt.
- `penwise link --no-adapter` — never touch the config or `package.json`.
- `penwise unlink` — remove the link and the registry entry.
- `penwise status` — list linked projects, ports, and whether each dev server answers.

Start the project's dev server yourself (`npm run dev`) in another terminal. Penwise proxies it; Penwise does
not start it.

### What the adapter does

The adapter adds one attribute to rendered HTML elements:

```
data-pw-src="src/App.jsx:12:5"
```

The value is the root-relative file, plus the 1-based line and column of the element's opening tag. It is
added only in a dev build, only to intrinsic elements (lowercase tags such as `div` and `my-el`), and never to
components. Production builds are untouched.

- **Vite:** the plugin is `@penwise/vite-plugin` in `<repo>/adapters/vite`. It adds
  `import penwise from '@penwise/vite-plugin'` and `plugins: [penwise()]` to `vite.config.*`. JSX/TSX, Vue
  SFC templates and Svelte components are covered; only their plain HTML elements get the attribute. SvelteKit
  projects use this plugin too. On a fresh checkout run `npm install --prefix <repo>/adapters/vite`.
- **Next:** the helper is `@penwise/next` in `<repo>/adapters/next`. It wraps the config:
  `export default withPenwise(nextConfig)`. It works for App Router server and client components, because the
  attribute is rendered into the HTML. On a fresh checkout run `npm install --prefix <repo>/adapters/next`.
- **Astro:** detected, but no adapter yet. Marks do not map to Astro component files; CSS source lines may
  still map.

The server reads the attribute from the resolved element, or from the nearest ancestor that has it. The bundle
then reports it as the first source entry, `{file, line, col, kind: "component"}`, with `inferred: true` when
it came from an ancestor.

If the CLI does not recognise the config shape, it prints manual steps and changes nothing. It finds the
adapter packages at `PENWISE_ADAPTERS=/some/dir`, next to the binary, or in `<repo>/adapters` (the checkout it
was built from); if none resolves, it skips the adapter step silently and the link and MCP steps still work.

## The agent loop

### 1. Open a frame

A frame is a live page at a chosen size. Open one:

- from the agent: `penwise_frame_open {project, route: "/", preset: "phone"}`, or
- from the tablet: **New frame**, then project, route and size.

The frame appears on the project's `<name> review` page. If the tablet or PC window shows another page, select
the review page; the agent can call `penwise_switch_page` for you. Presets: `phone` 390×844, `tablet`
834×1194, `desktop` 1440×900. A frame keeps its preset at any canvas zoom.

### 2. Use the page

- **One finger** scrolls and clicks inside the frame.
- **Two fingers** pan and zoom the canvas.
- The **pen** always draws. Nothing on the page reacts to the pen.

### 3. Draw marks

A mark is one group of strokes. Strokes drawn within about 1.5 s and 40 px of each other join one mark.

| Mark | Meaning |
|---|---|
| Circle or loop | Look here (focus) |
| Arrow | Move this there: tail = what, head = where |
| Cross (X) | Remove |
| Wavy line under | Feels off; the agent asks or proposes |
| Box in empty space | New thing here; the contents are the sketch |
| Line through text | Change this text; handwriting nearby is the new text |
| Anything else, handwriting | The agent reads it freely |

Colours carry meaning:

- **Red ink** — a problem.
- **Blue ink** — an idea.
- **Green ink** — good, keep.

### 4. Choose bound or area

The **Element / Area** button in the mini bar sets how new marks attach:

- **Element** (default): the mark follows its element when the layout moves it. It never scales. When the
  element is gone, the mark hides.
- **Area**: the mark stays at its document position. It scrolls with the page and is clipped to the frame.

A mark only shows at the preset it was drawn on. Phone marks do not show at desktop, and the other way round.

### 5. Pages with state

Open a menu or a dialog with a finger, then draw. The mark remembers the clicks that opened it. Marks drawn in
another state show greyed and are not hittable. Tap the mark chip and choose **Replay state** to replay the
clicks.

Only clicks on elements that toggle `aria-expanded`, a `details` or `dialog` open state, or a state class are
recorded. If replay fails, the mark stays greyed; open the menu by hand and the mark becomes active.

### 6. Ask the agent

Say "look at what I marked" in the terminal, or ask for a change. The agent calls `penwise_marks` and gets one
bundle per mark:

```
id, status, frame, project, route, preset, state,
binding ("bound" or "area"), rect,
kind {guess, confidence, from, to}, colour,
element {selector, tag, role, text, rect, styles, source},
text (handwriting), text_status, images {with_ink, without_ink, context}
```

`element` is `null` when the element is gone or cannot be found. A source entry with `kind: "component"` is the
component file; an entry with `rule` is a CSS rule.

`penwise_marks` returns marks with status `new` or `reopened` by default, and only what changed since that
agent's previous check. The agent's first call therefore returns everything new.

### 7. Status

Each mark carries a status. The chip colour shows it: blue = new, grey = seen, orange = reopened, green = done.

- **Seen** — the agent read it.
- **Done** — the edit landed. Done marks hide unless **Show done** is on.
- **Reopened** — the change was wrong; look again.

On the tablet, tap a chip for **Done**, **Reopen**, and **Replay state**. **Show done** is in the Marks drawer.
On the PC, **Show done** is in the toolbar. Status changes sync and undo like any other edit.

### 8. Ghosts and variants

The agent must propose before it edits. Two ways:

- **Ghost** — one temporary patch on the frame. The **Ghost** chip toggles it on and off, so you can compare
  before and after.
- **Variants** — 2 to 4 frames beside the base frame, each with a short label and its own patch. Tap **Use
  this** on the one you want. The button changes to **Chosen**.

A patch is CSS plus simple DOM ops: replace, remove, insert, append, prepend, set or remove an attribute, set
text, add or remove a class. It cannot run JavaScript. Failed ops come back in the tool's `failed` list with an
index and a reason, so a stale selector is visible.

The agent reads your pick with `penwise_choice`. Then it edits the code. Live reload or HMR refreshes the frame;
the **Reload** chip reloads it by hand.

## Agent tools

Design tools:

| Tool | Purpose |
|---|---|
| `penwise_projects` | List linked projects: id, root, kind, dev origin, port, notebook. |
| `penwise_frame_open` | Open a project route as a live frame at a preset. |
| `penwise_frame_snapshot` | PNG of a frame at its preset, with or without your ink. |
| `penwise_marks` | Read mark bundles, with element source and crops; moves the checkpoint. |
| `penwise_mark_status` | Set a mark to seen, done, reopened or new, with an optional note. |
| `penwise_ghost` | Show or clear one temporary patch on a frame. |
| `penwise_variants` | Create 2–4 labelled variant frames beside the base frame. |
| `penwise_choice` | Read the picked variant and the ink drawn on the variants. |
| `penwise_variants_clear` | Remove a variant group; its marks are kept as done. |

Page tools, from the pen-and-paper side:

| Tool | Purpose |
|---|---|
| `penwise_page_png` | PNG of a notebook page, your ink included. |
| `penwise_page_status` | Stroke and page counts. |
| `penwise_annotate` | Draw polylines on the agent layer of a page. |
| `penwise_clear_agent_marks` | Remove every agent-drawn mark on a page. |
| `penwise_place_image` | Put a picture on a page. |
| `penwise_list_images` | List the pictures on a page. |
| `penwise_remove_image` | Remove one placed picture. |
| `penwise_page_annotations` | Attribute a page's ink to captured elements by selector. |
| `penwise_list_pages` | List notebooks, pages and the active page. |
| `penwise_create_page` | Create a page. |
| `penwise_create_notebook` | Create an empty notebook. |
| `penwise_page_text` | Read handwriting as text with a local vision model. |
| `penwise_search` | Search the handwriting already read as text. |
| `penwise_switch_page` | Make a page active and make the tablet and PC follow. |

There is no MCP reload tool. Live reload and HMR handle edits; the tablet and PC have a **Reload** chip.

## Troubleshooting

**The frame says the dev server is unreachable.** Start the dev server in the project folder. Check the port:
`penwise status` shows the origin and whether it answers. If the port is wrong, fix the dev script or config,
then run `penwise link` again in the same folder. The registry entry is updated, not duplicated. Press
**Reload** on the frame.

**The tablet cannot reach a project port.** The PC firewall must allow 7421 and up. Re-run
`sudo ufw allow 7420:7430/tcp`. For a USB-connected tablet that uses localhost, forward the port:

```
adb reverse tcp:7420 tcp:7420
adb reverse tcp:7421 tcp:7421
```

Repeat for each project port in use. LAN or Tailscale needs no `adb reverse`.

**A mark disappeared after an edit.** Its element is gone; bound marks hide. The bundle still carries the mark,
but `element` is `null`. Tell the agent, or draw a new mark on the new element.

**A mark is greyed out.** The page state differs from the state the mark was drawn in. Tap the chip and choose
**Replay state**, or open the menu yourself. If the selector changed, replay fails and the mark stays greyed.

**A bundle has no component source.** The project has no adapter, or the element is not a plain HTML element,
or the file is outside the project root. Static sites map HTML and CSS lines. A Vite dev origin maps CSS
(`/src/x.css`, `/@fs/…` under the root) to files; its HTML has no file line without the adapter. Next CSS
stays URL-only.

**The agent does not see the project.** Run `penwise_projects`. If the list is empty, run `penwise link` with
the server up. Restart the agent after the CLI writes the MCP config.

**A ghost changed nothing.** The agent's selector matched no element. Ask it to read the `failed` list and fix
the selector. Nothing on the page changes when an op fails.

## Limits

- Local dev sites only. Live public sites are out of scope; the proxy dials a local origin.
- No Astro adapter. Astro is detected, but marks do not map to Astro component files.
- Patches are CSS and simple DOM ops. They cannot run JavaScript.
- Next CSS source is a URL, not a file line.
- One size at a time per frame. Marks belong to the preset they were drawn at.
- Variants are patches on the live page, not branches or worktrees.
- Access is one shared token on the LAN, over cleartext HTTP. Keep the ports inside the LAN or Tailscale.
