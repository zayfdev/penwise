# Agent access (MCP)

Agents read the Penwise page through a small stdio MCP server. It talks to the running Penwise server over
loopback HTTP, so start the server first:

```
cd ~/workspace/penwise && just serve        # :7420
```

Then point the agent at the binary (the server's data directory must be a v4 library: run
`penwise-server migrate --db <path>` once on a pre-rewrite one):

```json
{
  "mcpServers": {
    "penwise": {
      "command": "/home/user/workspace/penwise/target/debug/penwise-mcp",
      "env": { "PENWISE_PORT": "7420" }
    }
  }
}
```

Build it with `cargo build -p penwise-mcp` (or `just mcp` to run it by hand).

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
| `penwise_switch_page` | `{page}` | `{"active":"<page id>"}` |
| `penwise_place_image` | `{path? \| asset?, x, y, w?, h?, label?, page?}` | the placed picture `{id, asset, x, y, w, h, label}` |
| `penwise_list_images` | `{page?}` | `{"page":"<page id>","images":[…]}` |
| `penwise_remove_image` | `{id, page?}` | `{"removed":1}` |
| `penwise_page_annotations` | `{page?}` | `{"elements":[{selector, strokes, rect, text}],"unanchored":n}` |

The MCP server finds the access token itself: `PENWISE_TOKEN`, else `~/.local/share/penwise/token` (where the
server writes it). Set `PENWISE_TOKEN` explicitly if the server uses a different data directory.

`penwise_place_image` puts a picture on the page — the reference an agent draws or writes about. Give it a local
`path` (uploaded, so the same file twice costs one asset) or an existing `asset`, plus the page rectangle. With
`path`, `w`/`h` default to the picture's own pixel size.

`penwise_annotate` is the other write path: polylines in page pixels (0–1600 × 0–2560) become ordinary strokes on a
separate `agent` layer (ordinary stroke items authored by the server device), so they sync to the tablet and
browser, render in the PNG, persist with the page, and clear as a set. Promoting a mark to ink on the tablet
(`Update{layer}`) takes it out of the set `penwise_clear_agent_marks` removes.

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
| `GET /` | Static viewer files from `PENWISE_WEB` (the viewer is rebuilt on `/sync` in the browser milestone) |
| `WS /sync` | Binary postcard sync protocol (tablet, browser): `docs/rewrite-design.md` §4 |

## Verified

- Handshake (`initialize`), `tools/list`, and both `tools/call`s answered over stdin/stdout.
- `penwise_page_png` returned a valid base64 PNG (magic bytes checked) matching `GET /page.png`.
- `penwise_page_status` returned the live stroke count.
