# Agent access (MCP)

Agents read the Penwise page through a small stdio MCP server. It talks to the running Penwise server over
loopback HTTP, so start the server first:

```
cd ~/workspace/penwise && just serve        # :7420
```

Then point the agent at the binary:

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
| `penwise_page_status` | — | `{"ok":true,"strokes":N}` |
| `penwise_annotate` | `strokes: [{points:[[x,y],…], color?, width?}]` | `{"added":N}` — draws on the agent layer |
| `penwise_clear_agent_marks` | — | `{"removed":N}` — wipes the agent layer, keeps the user's ink |
| `penwise_list_pages` | — | `{"active":"pg_…","notebooks":[…]}` |
| `penwise_create_page` | `{title?, background?, kind?, notebook?}` | `{"page":{…},"meta":{…}}` |
| `penwise_create_notebook` | `{title}` | `{"notebook":{…},"meta":{…}}` |
| `penwise_page_text` | `{page?, refresh?}` | `{"page":…,"text":"…","cached":bool}` — handwriting as text |
| `penwise_switch_page` | `{page}` | `{"active":"pg_…"}` |

The MCP server finds the access token itself: `PENWISE_TOKEN`, else `~/.local/share/penwise/token` (where the
server writes it). Set `PENWISE_TOKEN` explicitly if the server uses a different data directory.

`penwise_annotate` is the only write path: polylines in page pixels (0–1600 × 0–2560) become ordinary strokes on a
separate `agent` layer, so they render in the browser and in the PNG, persist with the page, and clear as a set.
There is no text extraction yet: an agent that needs handwriting as text
should look at the PNG with a vision model.

## Endpoints the MCP server uses

| Route | Purpose |
|---|---|
| `GET /page.png?scale=…` | The page rendered through the same engine the tablet uses |
| `GET /health` | Stroke count |
| `POST /annotate` | Draw polylines on the agent layer (`{"strokes":[…]}`) |
| `POST /annotate/clear` | Remove every agent mark |
| `GET /` | Live mirror (browser tab) |
| `WS /ws` | Sync protocol: `hello` / `stroke` / `page` frames |

## Verified

- Handshake (`initialize`), `tools/list`, and both `tools/call`s answered over stdin/stdout.
- `penwise_page_png` returned a valid base64 PNG (magic bytes checked) matching `GET /page.png`.
- `penwise_page_status` returned the live stroke count.
