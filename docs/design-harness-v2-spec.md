# Penwise design harness — spec v2

Status: **draft for owner review** (2026-10-30). v1 (`docs/design-harness-spec.md`) stays the reference for
everything this file does not change. Decisions marked **[owner]** came from the 2026-10-30 talk; **[default]** are
the orchestrator's choices and are listed again in §11 for confirmation.

## 1. Why v2

v1 works in tests but puts a small, scrolling window on a page with white around it. The owner wants:

- a frame that **is** the page: full length, edge to edge, no scrolling inside anything **[owner]**;
- open space around it to keep sketching and iterating **[owner]**;
- desktop first; the tablet in landscape **[owner]**;
- a small agent surface: few tools, few calls, few tokens **[owner]**;
- push, not pull: the agent hears about a finished round without the owner asking **[owner]**.

## 2. Concepts

| Concept | Meaning |
|---|---|
| **Board** | One infinite canvas per linked project (v1's `<name> review` page, renamed). Frames, snapshots, sketches and notes all live on it. Notebooks stay for plain notes. |
| **Full-length frame** | A frame whose box on the board is `preset width × document height`. The canvas does the scrolling; the site never scrolls inside a box **[owner]**. `extent: full` is the default; `viewport` (v1 behaviour) stays as an option for pages that cannot be unrolled. |
| **Live window** | The part of a frame that runs as a real browser view: preset width × preset height, positioned where the camera looks. The rest of the frame shows the latest snapshot. |
| **Camera** | Per-device view of the board (local, never synced). Two states: **board view** (zoomed out, free sketching) and **focus view** (one frame fills the screen width). |
| **Round** | One request from the owner: the marks drawn since the last round was sent. The owner sends it with **Send** or just tells the agent to look **[owner]**. |
| **History snapshot** | When a round is resolved, the frame as it looked, with that round's marks, freezes into an image on the board. |

### 2.1 Board layout **[default]**

Time runs left to right. The live frame is "now"; variants to its right are possible futures.

```
 [round 1][round 2][ LIVE Desktop / ][variant A][variant B]
                    ^ focus view fits this to the screen
 sketches, wireframes and notes anywhere around it
```

Each screen (route + preset) has its own **lane**: one row holding that screen's history, live frame and
variants. Lanes stack downward (`y = bottom of the tallest box above + 200`). On resolve, the snapshot takes the
live frame's place and the live frame moves one column right (one `Update` op); within its lane only the
variants were there, and resolve removes them (the picked one too: the live frame now shows it in code).

## 3. Full-length frames

### 3.1 Geometry
- The live window keeps the **preset height** as its viewport (1440×900 for desktop). A view as tall as the
  document would make `100vh` equal the whole document and break hero sections; this is why the site still
  scrolls underneath, but the owner never sees a scroll box.
- Frame box height on the board = document height. The rendering client uses its live value from the bridge
  (`pw:layout`); the synced `height` field is a hint the **server** writes (from headless Chromium) for layout of
  neighbours and for clients with no live view.
- Focus view fits the frame width to the screen. Tab S9 landscape (2560×1600) and desktop (1440×900) are both
  16:10, so the desktop preset fills the screen edge to edge at scale 2560/1440.
- Only the **focused** frame has a live window, on the tablet and on the PC. Every other frame shows its snapshot.
  This bounds WebViews/iframes to one per device (battery; the 2026-10-28 GPU incident).

### 3.2 Scroll link (the spike, §9 V1)
The live window's scroll must match the camera in the same frame, or marks appear to slide over the site. Two
directions, chosen by feel on the device:

- **A — camera drives page:** canvas pan sets the page scroll (`WebView.scrollTo` / `pw:scrollTo`).
- **B — page drives camera:** a finger drag goes to the page (native scrolling, inner scroll boxes and
  smooth-scroll libraries keep working) and the bridge's scroll reports move the camera.

Likely result: B for one-finger drags in focus view, A for two-finger pan and agent focus. Fallback if neither
holds the frame: during a pan, show the live window as a bitmap that moves with the canvas and let the page catch
up on release.

### 3.3 Snapshots
- The server's headless Chromium (already one per active project, `--disable-gpu`) captures the full document at
  the preset width on load, after live reload/HMR settles, and after a patch changes (debounced 500 ms).
- The bridge reports bound-element rects in **document** coordinates for every watched key, not only visible
  ones, so marks outside the live window still sit on the snapshot correctly.
- Fixed and sticky elements appear once, at their position at scroll 0, in a snapshot. The live window shows
  the truth. Known limit, listed in the edge-case set.

### 3.4 Input in a frame
- Pen always draws (unchanged).
- One finger: tap = click in the page; drag = scroll (direction per §3.2).
- Two fingers: canvas pan/zoom (unchanged).
- Double-tap a frame with one finger in board view = focus it. Pinch out of focus = board view.

## 4. Rounds

- A round is a server row: `rounds(project, n, status, sent_seq, resolved_seq, sent_at, resolved_at)`; status is
  `open → sent → resolved`. **[default]**
- Marks belong to a round by server `seq`: round *n* holds marks added after round *n−1* was sent, up to round
  *n*'s `sent_seq`. No core or codec change. **[default]**
- **Send** (tablet toolbar and PC window): closes the open round and opens the next. Disabled while offline.
- `look` without Send works too: it returns the open round's marks **[owner]**.
- Resolve: when every mark of the round is done (or the agent resolves the whole round), the server captures the
  frame **with** the round's ink, places the image where the live frame was, moves the live frame right, clears
  ghosts, and removes the lane's variants (their marks become `done`, as `variants_clear` does today) — except a
  variant the owner marked after Send, which stays for the next round.

## 5. Agent interface — MCP v2

Four tools replace the v1 tools **[owner]**. No compat layer; v1 tools are deleted. The v1 schemas cost about
2.6k tokens (measured from `tools/list`); the bigger costs were the number of calls per round (8+) and two
images per mark.

| Tool | Arguments | Returns |
|---|---|---|
| `look` | `{mark?, frame?, images?: "overview"\|"none"}` | Compact text: project, board, frames, round, marks, picks. One **overview image** per touched frame: the region around the marks, numbered, long side ≤ 1280 px. `{mark}` returns that one mark's full bundle (v1 §7.2) with crops. Returned marks move `new → seen` automatically. |
| `show` | exactly one of `frame {route, preset?=desktop, extent?=full}`, `ghost {frame, patch \| clear}`, `variants {frame, options:[{label, patch}] 2–4 \| clear}`, `sketch {strokes, near?}`, `image {path, x?, y?, label?}`, `focus {frame \| page}` | What was created (ids) and failed patch ops. `frame` reuses an existing frame with the same route and preset. |
| `resolve` | `{marks?: [{id, status: done\|reopened\|seen, note?}], round?: n \| true}` | New statuses. `round: n` (the number `look` shows) closes round *n* as in §4; `round: true` means the latest *sent* round and is refused when none is waiting, so an open round is only closed by its number. The round also closes when no open mark remains in the sent round. |
| `notes` | `{query?, page?, png?}` | Plain notes: list pages, handwriting as text, search, or a page PNG. |

- **Camera on `show`:** the owner's device moves to the result unless the owner drew in the last 10 s; then it
  shows a chip "Agent: 3 variants →" instead. **[default]**
- `look` text format, one mark per line:
  `2 mk_… arrow red/problem "less heavy" h1.hero-title → section#work · index.html:42 · styles.css:118 {font-size:72px; font-weight:800}`
- The project comes from `PENWISE_PROJECT` (written by `penwise link`); no tool takes a project argument.

## 6. Push — agent integrations

- `penwise inbox` (CLI): prints one line when a sent round has not been looked at, e.g.
  `Penwise: round 3 sent — 4 marks on Desktop / (call look)`. Prints nothing otherwise. Exit 0 always.
- **Claude Code:** `penwise link` offers (asks first, shows the diff) to write into the project:
  - `.claude/settings.json`: a `UserPromptSubmit` hook running `penwise inbox`, whose output is added to the
    prompt's context;
  - `.claude/skills/penwise/SKILL.md`: the loop, the mark vocabulary, the four tools.
- The hook runs when the owner submits a prompt, so the owner types anything ("go", "check") and the round is
  already in context. It does not wake an idle session. **[default]**
- **omp:** the same skill text at `.omp/skills/penwise/SKILL.md`. omp has no documented prompt hook (researched
  in V5: `--hook`/extension flags exist, but no prompt-submit event is documented), so there is no omp hook.
- `docs/agent-prompt.md` is gone; the skill replaces it (source `crates/penwise-cli/assets/SKILL.md`).

## 7. Retired in v2

| What | Why |
|---|---|
| Chrome extension (`extension/`, `docs/extension.md`) | Static screenshot capture onto a page; live frames supersede it. The DB holds 3 anchors, all test data (`example.com`). |
| Grabber (`scripts/grab.mjs`, `just grab`) | Cut components into static tiles; frames + snapshots supersede it. |
| Anchors (`penwise-server/src/anchors/`, `/pages/:id/anchors`, `penwise_page_annotations`) | Only served the two above. Its tables (`page_anchors`, `page_anchors_seen`) are lazy sidecars outside the numbered schema, so no migration: the code goes and the clean start drops the tables. |
| The v1 MCP tools | Replaced by §5. |
| `docs/agent-prompt.md` | Deleted; replaced by the skill (§6). |

Pictures on the canvas (`/assets`, image items) stay: history snapshots are image items.

Rule for later milestones: when new work supersedes old work, the same milestone deletes the old.

## 8. Edge-case test pages

Static pages under `fixtures/sites/<case>/`, linked as test projects and run in `just web-e2e`. Each one either
passes (marks stay put through scroll, reload and snapshot) or is written down as a known limit.

| Case | What can break |
|---|---|
| `sticky-header` | header position in the live window vs the snapshot |
| `hero-100vh` | viewport height: must stay the preset height |
| `reveal-on-scroll` | content hidden until scrolled; snapshot shows it hidden |
| `lazy-images` | images missing in the snapshot until scrolled; document height changes |
| `inner-scroll` | a scrolling sidebar/code block inside the page (finger routing) |
| `smooth-scroll` | a library that takes over scrolling (scroll-link direction B) |
| `infinite-feed` | document height grows without end; must fall back to `extent: viewport` |
| `portfolio` | the owner's real site, read-only copy |

## 9. Milestones

| # | Deliverable | Done when |
|---|---|---|
| V0 | Clean start: back up `~/.local/share/penwise`, fresh DB, tablet app data cleared at install. Retire §7 (not the v1 tools yet). Tablet: screen stays awake 2 min after the last pen input, then the system timeout applies **[owner]**. | gates green; `extension/`, grabber and anchors gone |
| V1 | **Spike:** a full-length frame with scroll link A and B on the tablet and the PC window | the owner pans the portfolio and the marks stay stuck to the page |
| V2 | Full-length frames, board + camera (board/focus view), one live window per device, server snapshots, desktop default | the portfolio fills the tablet in landscape; zoom out shows the whole page |
| V3 | Rounds: Send, round rows, resolve freezes the snapshot and moves the live frame right | two rounds side by side on the board |
| V4 | MCP v2 (`look`/`show`/`resolve`/`notes`), v1 tools deleted, `penwise inbox` | a full round takes ≤ 4 tool calls in the e2e test |
| V5 | Integrations: `penwise link` writes the Claude Code hook + skill; omp skill | "round 2 sent" appears in a Claude Code session on the next prompt |
| V6 | §8 edge-case pages in the e2e gate; docs (`design-harness.md`, `mcp.md`) rewritten for v2 | every case passes or is a written limit |

V0 runs now. V1 blocks V2. V4's server side can run in parallel with V2/V3; its overview images need V2's
snapshots. All of it is tested on the device in one session when the tablet is on **[owner]**.

## 10. Acceptance (v2)

1. The agent calls `show {frame: {route: "/"}}`: the portfolio opens at desktop, full length, and the tablet in
   landscape shows it edge to edge.
2. Panning down the frame scrolls the site with the marks stuck to it; no scroll box appears anywhere.
3. Zooming out shows the whole page as one long frame, with open board around it to sketch on.
4. The owner marks, taps Send; at the next prompt in Claude Code the round is in context; `look` returns the marks
   with file:line and one overview image.
5. `show {variants}` puts 3 variants to the right; the owner taps "Use this"; the agent edits; `resolve` freezes
   round 1 to the left and the live frame shows the edit.
6. A round costs at most 4 tool calls: `look`, `show`, `look` (the pick), `resolve`.

## 11. Defaults to confirm

1. Board layout: history to the left, live frame in the middle, variants to the right; phone frames one row below.
2. Rounds by server `seq`, no codec change.
3. `look` moves returned marks to `seen` (saves a call).
4. `show` moves the owner's view unless they drew in the last 10 s (then a chip).
5. `notes` is always registered, also in design projects (one tool).
6. The Claude Code hook fires on the next prompt; there is no wake-up of an idle session.
7. `extent: full` is the default; `viewport` stays for pages that cannot be unrolled.

## 12. Risks

| Risk | Plan |
|---|---|
| Scroll link lags a frame on the tablet; marks swim | V1 spike first, A vs B, bitmap fallback (§3.2) |
| Full-page snapshots are large for long pages (1440 × 10 000 px) | tile them, downscale in board view; the overview image in `look` crops to the marks |
| Fixed elements and scroll animations differ between snapshot and live window | accepted limit; the live window is the truth; covered in §8 |
| Frame height changes while drawing (lazy content) | area marks keep document coordinates (v1); bound marks follow elements |
