# Penwise design harness — owner guide (v2)

Draw on a live copy of your site with the pen. The agent reads your marks, shows a proposal on the board, edits
the code after you pick, and the board keeps the history. This guide covers setup, the board, a round, and what
does not work yet.

- Design and contract: `docs/design-harness-v2-spec.md` (what v2 is, rounds, the agent tools) and
  `docs/design-harness-v2-contract.md` (full-length frames, the camera, stills). `docs/design-harness-spec.md`
  stays the reference for what v2 does not change (bundles, ghosts, bridge).
- The agent interface: `docs/mcp.md`. The agent's own instructions are the `penwise` skill that
  `penwise link` writes into the project (source: `crates/penwise-cli/assets/SKILL.md`).
- Known limits and gaps: `docs/next-iteration.md`.

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
4. **`add the hook to .claude/settings.json? [Y/n]`** — a `UserPromptSubmit` hook that runs `penwise inbox`
   (see "Push" below). The CLI prints the diff first and merges into an existing file: your other keys and
   hooks stay, and linking again adds nothing twice. The command is plain `penwise inbox` when `penwise` is on
   your `PATH` (the file is usually committed and shared, so it stays portable) and the absolute path of the
   binary that linked otherwise.
5. **`write the skill? [Y/n]`** — writes the same skill, `penwise`, to `.claude/skills/penwise/SKILL.md` and
   `.omp/skills/penwise/SKILL.md`. It tells the agent the loop, the mark vocabulary and the four tools. If a
   file already exists and differs, the diff is shown first.

`penwise link` records what it wrote in `.penwise.json`, so `penwise unlink` removes exactly that: the registry
entry, the `penwise` MCP entries, the hook, and the skill files (a skill you edited stays). Anything else in
those files is untouched.

Flags:

- `penwise link --yes` or `-y` — accept every prompt.
- `penwise link --no-adapter` — never touch the config or `package.json`.
- `penwise unlink` — remove the link, the registry entry and the agent files `link` wrote.
- `penwise status` — list linked projects, ports, and whether each dev server answers.
- `penwise inbox` — print one line when a sent round is waiting for the agent; nothing otherwise. It is what the
  hook runs; run it by hand to check.

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
was built from); if none resolves, it skips the adapter step silently and the link, MCP, hook and skill steps still work.

## The board

Each linked project has one **board**: an infinite canvas page named `<project> review`, in the project's
notebook. Frames, history pictures, your sketches and the agent's sketches all live on it. Notebooks stay for
plain notes.

- **Frame.** A live page at a preset: `desktop` 1440x900 (the default), `tablet` 834x1194, `phone` 390x844. A
  frame keeps its preset at any zoom, and marks belong to the preset they were drawn at.
- **Full length.** A frame is the whole page: its box is the preset width times the document height. The canvas
  scrolls; the site never scrolls inside a box with white around it. Pages that cannot be unrolled (an endless
  feed) fall back to a viewport-sized frame (`extent: viewport`).
- **Lanes.** Each screen (route and preset) has its own lane, a row on the board. Lanes stack downward. Inside
  a lane time runs left to right: history pictures of resolved rounds on the left, the live frame ("now") in the
  middle, variants the agent proposed on the right.
- **Camera.** Per device, never synced. *Board view* is zoomed out for free sketching and a look at the whole
  page. *Focus view* fits one frame's width to the screen (a desktop frame fills a landscape tablet edge to
  edge). Double-tap a frame with one finger to focus it; pinch out to return to board view. The agent can move
  your camera (`show` and `focus`); if you were drawing in the last 10 seconds your device shows a chip
  ("Agent: 3 variants") instead of moving.
- **One live window per device.** Only the focused frame runs as a real browser view; the others show their
  latest snapshot, so a long page does not cost a WebView per frame. Fixed and sticky elements appear once, at
  their position at the top, in a snapshot; the live window shows the truth.

Open a frame from the agent (`show {frame: {route: "/", preset: "phone"}}`; asking again for the same route and
preset reuses the frame) or from the tablet (**New frame**, then project, route and size).

## Use the page

- **One finger** scrolls and clicks inside the live frame.
- **Two fingers** pan and zoom the canvas.
- The **pen** always draws. Nothing on the page reacts to the pen.

## Draw marks

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

Only those three pinned colours (the red, blue and green buttons in the colour flyout) carry a meaning. Any other
colour, from the wheel or the recents, is a plain note that the agent reads like handwriting.

### Choose bound or area

The **Element / Area** button in the mini bar sets how new marks attach:

- **Element** (default): the mark follows its element when the layout moves it. It never scales. When the
  element is gone, the mark hides.
- **Area**: the mark stays at its document position. It scrolls with the page and is clipped to the frame.

A mark only shows at the preset it was drawn on. Phone marks do not show at desktop, and the other way round.

### Pages with state

Open a menu or a dialog with a finger, then draw. The mark remembers the clicks that opened it. Marks drawn in
another state show greyed and are not hittable. Tap the mark chip and choose **Replay state** to replay the
clicks.

Only clicks on elements that toggle `aria-expanded`, a `details` or `dialog` open state, or a state class are
recorded. If replay fails, the mark stays greyed; open the menu by hand and the mark becomes active.

## A round

A **round** is one request to the agent: the marks you drew since the last round was sent.

1. Draw marks on the frame (see the vocabulary above).
2. Tap **Send** (tablet toolbar and PC window) to close the round, or just tell the agent to look; `look` works on
   the round you are still drawing too.
3. The agent calls `look` and gets compact text plus one numbered overview image per frame. A line per mark:

   ```
   2 <mark id> f1 new arrow red/problem "less heavy" h1.hero-title → section#work · index.html:42 · styles.css:118 {font-size:72px; font-weight:800}
   ```

   The number is on the overview image too. Then: id, frame, status, kind guess, colour meaning, your
   handwriting, the element, the element an arrow points at, source `file:line` entries (the component first
   when an adapter maps one) and a few computed styles. `look {mark: id}` gives one full bundle with crops
   (`element` is `null` when the element is gone). `look` also lists every mark you **reopened**, from any
   round, and, apart, marks you drew after Send on the same screens (`since send`: they are next round's).
4. The agent proposes before it edits (below). You pick. The agent edits the code; live reload or HMR refreshes
   the frame (the **Reload** chip does it by hand).
5. The agent resolves the round (`resolve {round: n}`, or automatically when the last open mark is `done`).

**Resolve** freezes history: the frame with your ink for that round becomes a picture where the frame stood,
and each variant beside it becomes a picture too (with the change applied and the ink you drew on it), in one row:
the original first, then the variants by label. The live frame moves to the right of that row and takes everything
to its right in its lane along, its ghost clears, and the variants go, the picked one too (the live frame now shows
it in code). A variant you marked after Send stays for the next round, unpictured. History pictures are ordinary
board items: undo, move or delete them.

The server also keeps each resolved round as a **generation**: the original picture, one picture per variant, and
which variant you had picked. `GET /projects/<id>/history?route=&preset=` lists a screen's generations, newest
first, and `look` mentions the latest one under the screen (`history: round 3 picked "A" (2 variants)`).

### Status

Each mark carries a status; the chip colour shows it: blue = new, grey = seen, orange = reopened, green = done.
`look` sets **seen**; the agent sets **done** (hidden unless **Show done** is on) when the edit landed. Tap a
chip for **Done**, **Reopen** and **Replay state** if the agent was wrong; a reopened mark comes back in the next
`look`. Status changes sync and undo like any edit.

### Ghosts and variants

The agent proposes before it edits:

- **Ghost** (`show {ghost}`) — one temporary patch on the frame; the **Ghost** chip toggles it so you can compare.
- **Variants** (`show {variants}`) — 2 to 4 frames beside the base frame, each with a label and its own patch.
  Tap **Use this** on the one you want; the button changes to **Chosen**. The agent reads your pick with `look`
  (`picks:` names it).

A patch is CSS plus simple DOM ops (replace, remove, insert, append, prepend, set or remove an attribute, set
text, add or remove a class); it cannot run JavaScript. Failed ops come back in the tool's `failed` list.

## Push: the agent hears about a sent round

`penwise inbox` prints one line when a sent round has not been looked at, for example
`Penwise: round 3 sent — 4 marks on Desktop / (call look)`, and nothing otherwise (reopened marks you changed since
the agent last looked are counted too). It always exits 0 and gives up after 300 ms, so a stopped server costs
nothing. `penwise link` offers to run it from a Claude Code `UserPromptSubmit` hook: Claude Code adds the hook's
output to the prompt, so after you tap Send, whatever you type next ("go", "check") arrives with the round in
context. The hook runs when you submit a prompt; it does not wake an idle session. omp has no documented prompt
hook, so it only gets the skill: say "look at my marks".

## Agent tools

Four tools (details in `docs/mcp.md`), no project argument: the project is `PENWISE_PROJECT`, which `penwise link`
writes into the agent config.

| Tool | Purpose |
|---|---|
| `look` | Read the round: text plus numbered overview images. `{mark}` gives one full bundle with crops. |
| `show` | Put one thing on the board: `frame`, `ghost`, `variants`, `sketch`, `image`, or `focus`. |
| `resolve` | Set marks `done`, `reopened` or `seen`; close round `n` (`round: n` from `look`, or automatically with the last open mark). |
| `notes` | Your plain notes: list pages, handwriting as text, search, a page as PNG. |

A round costs four calls: `look`, `show`, `look` (your pick), `resolve`.

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
looked at; run it by hand in the project folder to check. It prints nothing when the server is down. The hook
only runs when you submit a prompt, and only for Claude Code.

**A ghost changed nothing.** The agent's selector matched no element. Ask it to read the `failed` list and fix
the selector. Nothing on the page changes when an op fails.

## Limits

- Local dev sites only. Live public sites are out of scope; the proxy dials a local origin.
- No Astro adapter. Astro is detected, but marks do not map to Astro component files.
- Patches are CSS and simple DOM ops. They cannot run JavaScript.
- Next CSS source is a URL, not a file line.
- Marks belong to the preset they were drawn at.
- Variants are patches on the live page, not branches or worktrees.
- Access is one shared token on the LAN, over cleartext HTTP. Keep the ports inside the LAN or Tailscale.
- More v2 gaps (scroll link not yet verified on the device, snapshots and fixed elements, endless feeds): `docs/next-iteration.md`.
