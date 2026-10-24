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

Both are read-only. There is no write-back and no text extraction yet: an agent that needs handwriting as text
should look at the PNG with a vision model.

## Endpoints the MCP server uses

| Route | Purpose |
|---|---|
| `GET /page.png?scale=…` | The page rendered through the same engine the tablet uses |
| `GET /health` | Stroke count |
| `GET /` | Live mirror (browser tab) |
| `WS /ws` | Sync protocol: `hello` / `stroke` / `page` frames |

## Verified

- Handshake (`initialize`), `tools/list`, and both `tools/call`s answered over stdin/stdout.
- `penwise_page_png` returned a valid base64 PNG (magic bytes checked) matching `GET /page.png`.
- `penwise_page_status` returned the live stroke count.
