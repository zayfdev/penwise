# Chrome extension: capture a page onto the canvas

"Annotate a live page": screenshot the tab (or one element), and you get a page on the tablet sized to the
picture, ready for ink. The element you picked is stored as an **anchor**, so an agent can tell what an
annotation is about, not just where it is.

## Install

1. `chrome://extensions` → enable *Developer mode* → **Load unpacked** → pick `penwise/extension`.
2. Open the popup once, set the server (`http://127.0.0.1:7420`, or the LAN/Tailscale address shown by
   `just serve`) and paste the token from `~/.local/share/penwise/token`.
3. Open any page, click **Capture tab** — or **Pick element** and click one element.

The extension needs host permissions for `http://*/*` so it can reach a server on any LAN or Tailscale address;
it only talks to the URL you configure.

## What a capture does

| Step | Call |
|---|---|
| Upload the PNG | `POST /assets` → `{asset, width, height}` |
| Create the page | `POST /pages` with `kind: {t:"fixed", width, height}` — one page px per screenshot px |
| Place it | `POST /pages/<id>/images` at `0,0,w,h`, `label` = the page URL |
| Element mode only | `POST /pages/<id>/anchors` with `{selector, rect, role, text, url}` |
| Show it | `POST /pages/<id>/select` — the tablet follows |

Element mode crops the screenshot to the element's CSS rect × `devicePixelRatio`, so the tablet shows exactly
that component.

## Files

| File | Role |
|---|---|
| `manifest.json` | MV3 manifest: `activeTab`, `scripting`, `storage` |
| `background.js` | service worker: capture, crop, publish; owns the pick handshake |
| `server.js` | the Penwise calls, Chrome-free so they are testable (`bun test extension`) |
| `picker.js` | injected picker: highlight, selector (`#id`, else a 4-level `nth-of-type` path), rect, text |
| `popup.html` / `popup.js` | settings + the two buttons |

## Verified

- `bun test extension`: the request sequence, page sizing, anchor body and error surfacing against a fake
  server (4 tests).
- Crop math in Chromium: `[10,20,40,30]` at dpr 2 → an 80×60 PNG.
- The real flow against the running server: `published {page, asset, anchors: 1}`, then
  `GET /pages/<id>/anchors` returned `#hero .cta`.

## Not yet

- Anchors are stored, not resolved: a stroke is not yet attributed to the selector it falls inside.
- Only the visible viewport is captured (no full-page stitching).
