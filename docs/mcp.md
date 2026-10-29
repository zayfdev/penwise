# Agent access (MCP)

Agents read the Penwise page through a small stdio MCP server. It talks to the running Penwise server over
loopback HTTP, so start the server first:

```
cd ~/workspace/penwise && just serve        # :7420
```

Then point the agent at the installed binary (the server's data directory must be a v4 library: run
`penwise-server migrate --db <path>` once on a pre-rewrite one):

```json
{
  "mcpServers": {
    "penwise": {
      "command": "/home/user/.local/bin/penwise-mcp",
      "env": { "PENWISE_PORT": "7420" }
    }
  }
}
```

`scripts/install-desktop.sh` builds the release binary and installs it to `~/.local/bin/penwise-mcp`.
From source: `cargo build -p penwise-mcp` (or `just mcp` to run it by hand); the debug build lives at
`target/debug/penwise-mcp` if you want to point the config there instead.

## Tools

| Tool | Arguments | Returns |
|---|---|---|
| `penwise_page_png` | `scale` (0.05–4, default 1; 0.5 halves the image) | PNG image content + a one-line text summary |
| `penwise_page_status` | `{page?}` | `{"ok":true,"page":"<id>","strokes":N,"pages":N,"head_seq":N}` |
| `penwise_annotate` | `strokes: [{points:[[x,y],…], color?, width?}]` | `{"added":N}` — draws on the agent layer |
| `penwise_clear_agent_marks` | — | `{"removed":N}` — wipes the agent layer, keeps the user's ink |
| `penwise_list_pages` | — | `{"version":4,"active":"<page id>","notebooks":[{id,title,pages:[{id,title,notebook,kind,background}]}]}` (ids are 26-char ULIDs; `active` is empty until a page exists) |
| `penwise_create_page` | `{title?, background?, kind?, notebook?}` | `{"page":{…},"meta":{…}}` |
| `penwise_create_notebook` | `{title}` | `{"notebook":{…},"meta":{…}}` |
| `penwise_page_text` | `{page?, refresh?}` | `{"page":…,"text":"…","cached":bool}` — handwriting as text |
| `penwise_search` | `{query}` | `{"query":…,"hits":[{page,title,snippet}]}` over pages already read |
| `penwise_switch_page` | `{page}` | `{"active":"<page id>"}` — also broadcasts `SwitchPage` so tablet and browser follow |
| `penwise_place_image` | `{path? \| asset?, x, y, w?, h?, label?, page?}` | the placed picture `{id, asset, x, y, w, h, label}` |
| `penwise_list_images` | `{page?}` | `{"page":"<page id>","images":[…]}` |
| `penwise_remove_image` | `{id, page?}` | `{"removed":1}` |
| `penwise_page_annotations` | `{page?}` | `{"elements":[{selector, strokes, rect, text}],"unanchored":n}` |
| `penwise_projects` | — | `[{id, name, root, kind, origin?, static?, port, notebook}]` — linked design projects |
| `penwise_frame_open` | `{project, route, preset, page?}` | `{"page":"<page id>","frame":"<item id>"}` — a live page as a frame (default page: the project's `<name> review`, created if missing) |
| `penwise_frame_snapshot` | `{frame, ink?}` | PNG image content of the frame at its preset; `ink` includes the owner's marks (default true) |
| `penwise_marks` | `{project?, page?, frame?, status?, since?, agent?, images?}` | mark bundles (§7.2) as a text block, plus `with_ink` and `context` image blocks per mark |
| `penwise_mark_status` | `{id, status: seen\|done\|reopened\|new, note?}` | `{"id":…,"status":…}` |
| `penwise_ghost` | `{frame, patch \| clear:true}` | `{"ok":bool,"failed":[{index, reason}]}` — a temporary patch live on the frame; failed lists the ops whose selector matched nothing |
| `penwise_variants` | `{frame, variants:[{label, patch}] (2–4)}` | `{"group":"vg_…","frames":[ids]}` — variant frames beside the base, one undo step |
| `penwise_choice` | `{group}` | `{"group","frames":[{id,label,chosen,patch}],"chosen":id\|null,"marks":[…]}` as a text block, plus the variants' mark crop images |
| `penwise_variants_clear` | `{group}` | `{"ok":true,"removed":N,"marks":M}` — frames gone, their marks kept as `done` |

The MCP server finds the access token itself: `PENWISE_TOKEN`, else `~/.local/share/penwise/token` (where the
server writes it). Set `PENWISE_TOKEN` explicitly if the server uses a different data directory.

`penwise_place_image` puts a picture on the page — the reference an agent draws or writes about. Give it a local
`path` (uploaded, so the same file twice costs one asset) or an existing `asset`, plus the page rectangle. With
`path`, `w`/`h` default to the picture's own pixel size.

`penwise_annotate` is the other write path: polylines in page pixels (0–1600 × 0–2560) become ordinary strokes on a
separate `agent` layer (ordinary stroke items authored by the server device), so they sync to the tablet and
browser, render in the PNG, persist with the page, and clear as a set. Promoting a mark to ink on the tablet
(`Update{layer}`) takes it out of the set `penwise_clear_agent_marks` removes.

## Design harness

`penwise_projects` → `penwise_frame_open` puts a linked project's route on a canvas page as a frame; the owner
draws on the frame; `penwise_marks` returns each mark as a bundle (spec §7.2): the kind guess (`circle`, `arrow`
with `from`/`to` elements, `cross`, `wavy`, `box_new`, `strike`, `underline`, `free`, with a confidence), the
colour meaning (`problem` red, `idea` blue, `good` green), OCR text of the ink, the element (`selector`, `role`,
`text`, `styles`, `rect`, and `source` `file:line` entries including the matching CSS rules) and three crop URLs.

`penwise_marks` returns the bundle JSON as one text block, then one image content block per mark for `with_ink`
(default) and `context` (`images: "all"`); `images: "none"` skips the pictures when only the facts are needed.
`since: "last"` (the default) returns marks created or changed after this agent's previous call, so repeated
checks do not repeat themselves. The checkpoint is stored per `agent` name (default `"agent"`), because two
agents — a terminal session and a review session — each need their own "since my last check". A call with no
`status` returns `new` plus `reopened` marks.

`penwise_mark_status` moves a mark along: `seen` after reading it, `done` once the edit landed (done marks hide
on the tablet and PC window), `reopened` when the owner questions a finished edit. Status changes are ordinary
synced ops, so they undo like any other edit.

`penwise_ghost` and `penwise_variants` are the proposals before any file edit. A patch is
`{css, dom:[{op, selector, html?, name?, value?}]}` (≤ 64 KiB, ≤ 200 ops, selectors ≤ 512 chars, no JS); the
server validates and sanitizes it (no `<script>`, `on*` attributes, `javascript:` URLs, `<iframe>`, `<object>`,
`<embed>`) and the bridge sanitizes again in the page. A frame with a patch shows a Ghost chip on the tablet and
the PC window; `failed` names the ops the headless browser could not apply, so a stale selector is visible in the
tool result instead of only on the canvas.

`penwise_variants` creates the frames beside the base frame, all in one edit batch (one undo step), and returns
the group id. The owner taps "Use this" on one of them, which is an ordinary `FrameChosen` edit from that device;
`penwise_choice` reads the scene afterwards, so the pick reaches the agent without polling. Bundles, frame
snapshots and crops render the frame **with its patch applied**, so the images match what the owner sees.
`penwise_variants_clear` removes the frames and moves the marks drawn on them to `done` first, so the ink stays
in the page's log and history even though the frames are gone.

`penwise_switch_page` selects a page for the HTTP tools *and* tells every connected tablet and browser to follow
(`ServerMsg::SwitchPage`), so the owner's screen changes with the agent's.

## Endpoints the MCP server uses

| Route | Purpose |
|---|---|
| `GET /page.png?scale=…` | The page rendered through the same engine the tablet uses, pictures included |
| `POST /assets` | Upload a PNG; returns its digest, which becomes the page object's `asset` |
| `POST /pages/<id>/images` | Place a picture at `x, y, w, h` in page px |
| `GET/DELETE /pages/<id>/images` | List or remove placed pictures |
| `GET /health?page=…` | Stroke/page counts and the log head |
| `POST /annotate` | Draw polylines on the agent layer (`{"strokes":[…]}`) |
| `POST /annotate/clear` | Remove every agent mark (`?page=` or body `{"page"}`) |
| `GET/POST/PATCH/DELETE /pages…`, `POST /pages/<id>/select`, `POST /notebooks`, `PATCH/DELETE /notebooks/<id>` | Page and notebook catalog (ops from the server device) |
| `GET /pages/<id>/history` | Op batches of the page, newest first (`last_seq` is a restore target) |
| `POST /pages/<id>/restore?to_seq=N` | Compensating ops back to the page as of seq `N` (404 unknown page / never-logged seq, 410 below the retained log) |
| `GET /projects` | Linked projects: id, name, root, kind, origin/static dir, port, notebook |
| `POST /frames/open` | `{project, route, preset, page?}` → `{"page","frame"}`: a frame on the project's review page, right of the existing ones |
| `GET /frames/<id>/snapshot.png?ink=…` | The frame's page at its preset with its patch applied, with or without the owner's ink |
| `POST /frames/<id>/ghost` | `{"patch": P1 \| null}`: sets/clears the frame's patch, validated in the headless browser (`{"ok","failed"}`) |
| `POST /frames/<id>/variants` | `{"variants":[{label, patch}]}` (2–4) → `{"group","frames"}` |
| `GET /variants/<group>` | `{"group","frames":[{id,label,chosen,patch}],"chosen","marks"}` — the picked variant and the ink drawn on the variants |
| `DELETE /variants/<group>` | Deletes the variant frames; their marks become `done` |
| `GET /marks?project=&page=&frame=&status=&since=last&agent=…` | Mark bundles plus the agent's `checkpoint`; `since=last` is that agent's previous call |
| `POST /marks/<id>/status` | `{"status"}` — a synced, undoable status edit |
| `GET /marks/<id>/image/<with_ink\|without_ink\|context>.png` | The bundle's crops |
| `GET /` | Static viewer files from `PENWISE_WEB` (`web/dist`, see `docs/draw.md`) |
| `WS /sync` | Binary postcard sync protocol (tablet, browser): `docs/rewrite-design.md` §4 |

## Verified

- Handshake (`initialize`), `tools/list`, and `tools/call`s answered over stdin/stdout.
- `penwise_page_png` returned a valid base64 PNG (magic bytes checked) matching `GET /page.png`.
- `penwise_page_status` returned the live stroke count.
- `crates/penwise-mcp/tests/tools.rs` drives every documented tool against an in-process server.
- `crates/penwise-mcp/tests/design.rs` links a static project, opens a frame with `penwise_frame_open`, adds a
  mark through the sync protocol, reads it back with `penwise_marks` (kind, colour, element source lines, three
  crop URLs, base64 PNG blocks), moves it to `seen` with `penwise_mark_status` and fetches the frame with
  `penwise_frame_snapshot`. It skips when Chromium is not installed (the bundle needs the inspect browser).
- The same file's `ghost_variants_and_choice_work_end_to_end` shows a ghost (`ok`, a stale selector reported as
  `no match`, then `clear`), creates three labelled variants, waits for a tablet to see them, pushes the
  `set_chosen` edit and a mark on the picked frame through the sync protocol, reads both back with
  `penwise_choice` (chosen id, one mark bundle, inline crop image) and clears the group.
- `crates/penwise-server/src/frames/patch/tests.rs` covers the P1 sanitizer directly: `<script>` (with its body),
  `<iframe>`/`<object>`/`<embed>`, `on*` attributes, `javascript:` URLs (spaces, tabs and character references),
  the size/op/selector limits, and ordinary markup left verbatim.
- `crates/penwise-server/src/frames/bridge/tests.rs` drives the bridge's patch engine in headless Chromium on the
  harness fixture: css + replace + insert + remove apply, revert restores the exact `outerHTML`, a patch is
  re-applied after a reload, unmatched/invalid selectors land in `failed`, and layout, keys and hits keep working
  on the patched DOM.
