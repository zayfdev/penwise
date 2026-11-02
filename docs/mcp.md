# Agent access (MCP)

Agents work with Penwise through a small stdio MCP server with **four tools**: `look`, `show`, `resolve` and
`notes`. It talks to the running Penwise server over loopback HTTP, so start the server first:

```
cd ~/workspace/penwise && just serve        # :7420
```

Then point the agent at the installed binary (the server's data directory must be a v4 or v5 library; run
`penwise-server migrate --db <path>` once on a pre-rewrite one):

```json
{
  "mcpServers": {
    "penwise": {
      "command": "/home/user/.local/bin/penwise-mcp",
      "env": { "PENWISE_PORT": "7420", "PENWISE_PROJECT": "prj_site" }
    }
  }
}
```

`scripts/install-desktop.sh` builds the release binary and installs it to `~/.local/bin/penwise-mcp`.
From source: `cargo build -p penwise-mcp` (or `just mcp` to run it by hand).

`penwise link` in a project writes this config for you: `.mcp.json` (Claude Code) and `.omp/mcp.json` (omp) in the
project root, both with the same `penwise` entry. It sets `PENWISE_PROJECT` to the registry id, and **no tool takes
a project argument**: `look`, `show` and `resolve` always act on that project. Without `PENWISE_PROJECT` they answer
with an error that says so; `notes` does not need it. Existing servers in both files are kept; a broken existing
file is reported and left untouched.

A failed tool call comes back as an `isError` result whose text carries the server's reason (`server replied: HTTP/1.1
409 Conflict: the open round has no marks to send`), so the agent can read it and recover; only a malformed request
is a JSON-RPC error. A call may wait up to 180 s (a look opens headless pages; a resolve snapshots every frame).

The MCP server finds the access token itself: `PENWISE_TOKEN`, else `~/.local/share/penwise/token` (where the
server writes it). Set `PENWISE_TOKEN` explicitly if the server uses a different data directory.

The v1 tool surface is gone, with no compatibility layer. Its schemas cost about 2.6k tokens on every
request and a round took eight or more calls with two images per mark; the four tools below do a round in four.

## A round in four calls

The owner draws marks on the project's board and taps **Send** (or just tells the agent to look). Then:

| Call | What it does |
|---|---|
| `look` | Reads the round: text plus one overview image per frame. |
| `show` | Puts something on the board: a frame, a ghost, variants, a sketch. |
| `look` | Reads the owner's pick. |
| `resolve {round: n}` | Closes round `n` (the number `look` shows): the frame and its variants freeze into history and the live frame moves right by the row of pictures. |

A prompt hook can tell the agent a round is waiting: `penwise inbox` prints one line such as
`Penwise: round 3 sent — 4 marks on Desktop / (call look)` and nothing otherwise (see
`docs/design-harness.md`).

## Tools

### `look`

`{mark?, frame?, images?: "overview" | "none"}`

Returns the marks of the **latest sent round that is not resolved yet, else the open round**, as compact text and,
unless `images` is `none`, one overview image per frame the marks sit on. The image is the region around the
marks (their union plus 48 px), each mark numbered with the same number as in the text, the long side at most
1280 px. Marks that were `new` become `seen`; a `reopened` mark stays reopened until the agent resolves it. The
agent's checkpoint advances and the round counts as looked at, which silences `penwise inbox`; a `look` with
`frame` or `mark` is a look at part of the round and does neither. Done marks are not listed. Every mark line shows
the mark's status, and **every reopened mark of any round** is listed with the round's marks, because the owner is
asking again. Marks drawn after the round was sent, on frames the round touched, are listed apart under
`since send (next round):`; they keep their status.

```
Penwise: site (prj_site) | board 01M66PDM2TPRMQN70GJ4FJ02FG | round 3 sent | 5 mark(s), 2 open
frames:
  f1 01a18d66d05b491761f51ca2ec3d4498 Desktop / 1440x900 (box 1440x3200)
  f2 01a18d66d05b491761f51ca2ec3d4499 Phone /work 390x844 (box 390x2100)
marks:
  1 018bcfe568029f52581ce1ff0e4ae394 f1 new arrow red/problem "less heavy" h1.hero-title → section#work · src/Hero.tsx:12 · styles.css:118 {font-size:72px; font-weight:800; color:rgb(0, 0, 0)}
  2 018bcfe568029f52581ce1ff0e4ae395 f2 reopened circle ink nav > a.cta · index.html:42
since send (next round):
  3 018bcfe568029f52581ce1ff0e4ae39a f1 new free blue/idea @120,640 200x60
picks:
  vg_2c0e…: picked "bold" 01a1…c3 | "calm" 01a1…c2, "bold" 01a1…c3
```

- The header names the project, the board page, the round number and status (`open`, `sent`) and how many marks it
  has.
- `frames:` lists the board's frames (id, preset, route, label, size, and the box on the board: the whole document
  for a full-length frame). `f1`, `f2` tag the marks.
- One line per mark: its number, id, frame tag, status, kind guess (`circle`, `arrow`, `cross`, `wavy`, `box_new`,
  `strike`, `underline`, `free`), colour meaning (`red/problem`, `blue/idea`, `green/good`, else `ink`: only the three pinned colours carry a meaning), the
  handwriting in quotes, the element selector (or `@x,y WxH` for an area mark with no element), `→` the element an
  arrow points at, up to three `file:line` sources (the component first), and up to three key computed styles.
- `picks:` lists every variant group with the frame the owner chose (`no pick yet` before a choice).
- A lane that has a past generation gets one more line under its first listed frame: `    history: round 3 picked "A" (2 variants)` (the newest resolved round of that screen; `no pick` when the owner chose none, `(no variants)` when it had none). A variant kept on the board because it carries a mark drawn after Send is not pictured, so even when it was the owner's pick the generation records no pick for it (`no pick`).

`{mark: "<id>"}` returns that one mark's full bundle (JSON: kind with `from`/`to` elements, colour, element with
`selector`, `role`, `text`, `styles`, `rect`, `source`, OCR text and `text_status`) with its crops (`with_ink`,
`context`) as image blocks. `{frame: "<id>"}` limits the answer to the marks on one frame.

### `show`

Exactly one of:

| Key | Arguments | Effect |
|---|---|---|
| `frame` | `{route, preset?="desktop", extent?="full"}` | Opens the route on the board; an existing frame with the same project, route and preset is reused. Presets `desktop` 1440×900, `tablet` 834×1194, `phone` 390×844. `full` = the box is the whole document; `viewport` keeps the preset height. |
| `ghost` | `{frame, patch}` or `{frame, clear: true}` | A temporary patch live on the frame. Returns `{ok, failed:[{index, reason}]}`; `failed` lists ops whose selector matched nothing. |
| `variants` | `{frame, options: [{label, patch}]}` (2–4) or `{frame, clear: true}` | Variant frames to the right of the base frame, one undo step. `clear` removes the groups beside the frame; their marks become `done`. |
| `sketch` | `{strokes: [{points, color?, width?}], near?}` | Polylines on the agent layer. With `near` (a frame id) the points are relative to that frame's top-left corner. |
| `image` | `{path, x?, y?, label?}` | Uploads a local PNG and places it on the board (right of the content when `x`/`y` are missing). |
| `focus` | `{frame}` or `{page}` | Moves the owner's view to a frame or a page. |

A patch is `{css?, dom?: [{op, selector, …}]}` (≤ 64 KiB, ≤ 200 ops, no JavaScript); the server sanitizes it and
the bridge sanitizes it again in the page. The reply names what was created: `{page, frame, reused}` for `frame`,
`{group, frames, page}` for `variants`, `{ok, failed}` for `ghost`, `{added}` for `sketch`, the picture for
`image`.

The owner's devices follow the result: after `frame`, `sketch near`, `focus` they fit that frame; after
`variants`, `image` and a page `focus` they fit the whole board page. (The tablet holds the move back while the
owner is drawing and shows a chip instead.)

### `resolve`

`{marks?: [{id, status: "done" | "reopened" | "seen", note?}], round?: n}`

Sets the statuses (synced, undoable edits) and returns them. The round closes when `round: n` names it **or
when the last open mark of the sent round becomes `done`**. `n` is the round number `look` printed in its header.
`round: true` means the latest *sent* round and is refused when none is waiting: **an open round is only ever
closed by its number**, so a stray `round: true` cannot freeze what the owner is still drawing. Resolving an open
round sends it first, so marks drawn while the freeze runs belong to the next round. One resolve runs per project at
a time; a second one for the same round finds it done and changes nothing. Closing a round:

1. snapshots every frame the round touched with **only the round's ink** and places that picture where the live
   frame stood, labelled `Round 3 — desktop /` (marks drawn after the round was sent stay off it), and does the
   same for **every variant** beside it (its patch applied, only its own round ink), labelled
   `Round 3 — desktop / — A`, in one row to the right of the OG picture, variants by label; it also keeps them as
   a generation (`GET /projects/<id>/history`, `docs/pages.md`);
2. moves the live frame right by the whole row (one column per picture, a column being its width plus 40 px), and
   **every item to its right in the same lane** (the items whose box starts right of the frame and overlaps its
   rows) by the same distance, so it never lands on
   the next route's frame; other lanes do not move;
3. clears the frame's ghost and removes the variant groups beside it, the picked variant included (their marks
   become `done`), except a variant that carries a mark drawn after the round was sent;
4. sets the round's marks to `done`.

Every snapshot of every page is taken first; if any fails (the headless browser is down) nothing changes and the
call fails with the reason, so it can be retried. Then each page commits one batch (one undo step). A frame that
already has this round's `Round n — …` picture is skipped, so a retry never freezes a frame twice.

### `notes`

`{query?, page?, png?}` — the owner's plain notes, not the design board. A `query` searches the pages already
read; `png: true` returns a page as a PNG image (`page`, default the active page); a `page` alone returns its
handwriting as text; no arguments list the notebooks and pages.

## Rounds

A round is the marks drawn since the previous one was sent (spec v2 §4). Marks belong to a round by the op-log
`seq` that added them, so there is no change to the page format: round *n* holds the marks added after round
*n−1*'s `sent_seq` up to its own; the open round holds everything after the last `sent_seq`. The `rounds` table
(schema step 5) keeps `status` (open, sent, resolved), `sent_seq`, `resolved_seq` and the times.

| Route | Purpose |
|---|---|
| `POST /projects/<id>/rounds/send {n?}` | Closes the open round → `{n, marks}`. A compare-and-set in the storage writer: 409 when round `n` (default the open one) is no longer open or has no marks, so a double tap sends once. The tablet and PC Send buttons call it. |
| `GET /projects/<id>/rounds` | Every round with status and mark counts (the open one included). |
| `POST /projects/<id>/rounds/<n>/resolve` | The resolve above, for one round. |
| `GET /projects/<id>/inbox` | `{"line": "Penwise: round 3 sent — 4 marks on Desktop / (call look)"}`, or `{"line": null}` when nothing is waiting. Reopened marks the agent has not looked at since are counted (`— 4 marks (+1 reopened) on …`) and announced alone when no round is waiting. |

These sit in the project route group: the token, or a request from this machine (the rule of `GET /pair/local`)
**from a client that is not a web page of another origin**: a request that carries an `Origin` other than the
server's own, or a `Sec-Fetch-Site` of `cross-site` or `same-site`, needs the token. The PC viewer (same origin),
the CLI and the MCP server (no `Origin`) are unaffected. The ledger reads only the project's own pages (its
notebook) and only their frame and mark rows, so the inbox stays quick on a large library.

## Endpoints the MCP server uses

| Route | Tool |
|---|---|
| `POST /agent/look` `{project, frame?, mark?, images?, agent?}` | `look` → `{text, images:[{label, png}], round, marks}` (a POST: it has effects) |
| `POST /agent/show` `{project, <frame\|ghost\|variants\|sketch\|image\|focus>}` | `show` |
| `POST /agent/resolve` `{project, marks, round}` | `resolve` (`round`: a number, or `true` for the latest sent round) |
| `POST /assets` | `show image` (the PNG upload; the file must be a regular file of at most 20 MB) |
| `GET /pages`, `GET /text?page=`, `GET /search?q=`, `GET /page.png?page=` | `notes` |

`POST /pages/<id>/focus {item?}` (token) makes every connected device switch to the page and fit the frame
`item`, like `POST /pages/<id>/select` for page switches. The older routes the tools compose (`POST
/frames/open`, `/frames/<id>/ghost`, `/frames/<id>/variants`, `DELETE /variants/<group>`, `GET /marks`, `POST
/marks/<id>/status`, `GET /frames/<id>/snapshot.png`) stay as they were.

## Verified

- `crates/penwise-mcp/tests/tools.rs` lists exactly four tools with their schemas (none takes a project), drives
  `show` (frame reuse, sketch near a frame, image from a file, focus), `look` and `resolve` on an empty board,
  and `notes` (list, text, search, PNG).
- `crates/penwise-mcp/tests/round.rs` runs a whole round against an in-process server and headless Chromium: show
  a frame, a tablet draws a mark and the owner sends, `look` (text with `file:line`, one overview image no longer
  than 1280 px, the mark moves to `seen`), `show variants`, the owner picks one, `look` (the pick), `resolve`.
  The agent makes **four tool calls** after the owner's first action. Afterwards the board holds the history
  picture at the old position, the live frame one column right, no variants, the mark `done`. A second test closes
  a round by resolving its last open mark. Both skip when Chromium is not installed.
- `crates/penwise-server/src/rounds/tests.rs` covers Send, the 409, the round windows and the inbox line;
  `storage/tests/rounds.rs` covers the v4 → v5 step.
- `crates/penwise-cli/tests/inbox.rs` runs `penwise inbox` against a stand-in server, a dead port and a hung one.
