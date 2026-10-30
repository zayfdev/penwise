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

- from the agent: `show {frame: {route: "/", preset: "phone"}}` (the preset defaults to `desktop`; the project
  comes from the `penwise link` config), or
- from the tablet: **New frame**, then project, route and size.

The frame appears on the project's `<name> review` page, and your tablet and PC window move to it. Asking for a
route and preset that already has a frame reuses that frame. Presets: `phone` 390×844, `tablet` 834×1194,
`desktop` 1440×900. A frame keeps its preset at any canvas zoom.

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

Marks you draw between two sends are one **round**. Tap **Send** to close the round, or just say "look at what I
marked" in the terminal. An agent prompt hook that runs `penwise inbox` (spec v2 §6; `penwise link` does not
write it yet) lets the agent hear about a sent round on your next prompt: `penwise inbox` prints
`Penwise: round 3 sent — 4 marks on Desktop / (call look)`, and nothing when no sent round is waiting.

The agent then calls `look` and gets compact text plus one numbered overview image per frame. A line per mark:

```
2 <mark id> f1 arrow red/problem "less heavy" h1.hero-title → section#work · index.html:42 · styles.css:118 {font-size:72px; font-weight:800}
```

That is the mark's number (the same number is on the overview image), id, frame, kind guess, colour meaning,
your handwriting, the element, the element an arrow points at, source `file:line` entries (the component first
when an adapter maps one) and a few computed styles. `look {mark: id}` returns the full bundle of one mark with
its crops: `element` is `null` when the element is gone or cannot be found, a source entry with `kind:
"component"` is the component file, and an entry with `rule` is a CSS rule.

`look` shows the latest sent round that is not resolved yet, else the round you are drawing now. The marks it
returns change from `new` to `seen`, so a second `look` shows the same marks without calling them new.

### 7. Status

Each mark carries a status. The chip colour shows it: blue = new, grey = seen, orange = reopened, green = done.

- **Seen** — the agent read it (`look` sets it).
- **Done** — the edit landed (`resolve`). Done marks hide unless **Show done** is on.
- **Reopened** — the change was wrong; look again.

When every mark of a sent round is done, the round resolves: the frame with your ink freezes into a picture where
the frame stood, the live frame moves one column to the right (history to the left, now in the middle, variants
to the right) and takes everything to its right in its lane along, its ghost clears and the variants beside it
go, unless you marked a variant after sending. The agent can also close a round with
`resolve {round: n}` (the round number `look` shows).

On the tablet, tap a chip for **Done**, **Reopen**, and **Replay state**. **Show done** is in the Marks drawer.
On the PC, **Show done** is in the toolbar. Status changes sync and undo like any other edit.

### 8. Ghosts and variants

The agent must propose before it edits. Two ways:

- **Ghost** (`show {ghost}`) — one temporary patch on the frame. The **Ghost** chip toggles it on and off, so you
  can compare before and after.
- **Variants** (`show {variants}`) — 2 to 4 frames beside the base frame, each with a short label and its own
  patch. Tap **Use this** on the one you want. The button changes to **Chosen**.

A patch is CSS plus simple DOM ops: replace, remove, insert, append, prepend, set or remove an attribute, set
text, add or remove a class. It cannot run JavaScript. Failed ops come back in the tool's `failed` list with an
index and a reason, so a stale selector is visible.

The agent reads your pick with `look` (the `picks:` section names the chosen frame). Then it edits the code. Live reload or HMR refreshes the frame;
the **Reload** chip reloads it by hand.

## Agent tools

Four tools, all in `docs/mcp.md`. None takes a project argument: the project is `PENWISE_PROJECT`, which
`penwise link` writes into the agent config.

| Tool | Purpose |
|---|---|
| `look` | Read the round: compact text (frames, marks with kind, colour, handwriting, element, `file:line`, styles, your variant pick) and one numbered overview image per frame. `{mark}` gives one full bundle with crops. |
| `show` | Put one thing on the board: `frame` (a route), `ghost`, `variants`, `sketch`, `image`, or `focus` (move your view). |
| `resolve` | Set marks `done`, `reopened` or `seen`; close round `n` (`round: n` from `look`, or automatically with the last open mark). |
| `notes` | Your plain notes: list pages, handwriting as text, search, a page as PNG. |

A round costs four calls: `look`, `show`, `look` (your pick), `resolve`.

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

**The agent does not see the project.** Run `penwise status`. If the list is empty, run `penwise link` with the
server up. Restart the agent after the CLI writes the MCP config; `look`, `show` and `resolve` need its
`PENWISE_PROJECT` and say so when it is missing.

**The agent does not hear about a sent round.** `penwise inbox` only prints while a sent round has not been
looked at; run it by hand in the project folder to check. It prints nothing when the server is down.

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
