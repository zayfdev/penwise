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

## Component cut-outs (the grabber)

`scripts/grab.mjs` cuts components out of a running page and lays them on one page, ready to be rearranged and
finished with ink — the second workflow:

```
bun scripts/grab.mjs --url http://localhost:3000 --selectors ".hero,.card" --all --title "Pricing ideas"
```

- Drives Chrome over the DevTools protocol (`Page.captureScreenshot` with a clip), so it needs no automation
  package — only a browser binary (`CHROME=` overrides the search).
- `--all` cuts every match of each selector; labels become `.card[0]`, `.card[1]`.
- Tiles are scaled to fit 640×480, laid out in a grid (3 columns past 4 tiles), uploaded as assets, and placed
  with the selector as the label; the page is then selected so the tablet follows.
- Verified against a fixture page: `cut .hero (1360x208)`, `cut .card …`, `page …: 2 cut-out(s), 1424x294`, and
  `GET /pages/<id>/images` reported labels `[".card[0]", ".card[1]"]`.

## Anchors, resolved

- **Tab mode** stores the element map: every element with an `id` or `data-testid` plus headings, sections and
  controls that are on screen, rects in screenshot px, capped at 400.
- **Element mode** stores one anchor covering the whole crop (the picture *is* the element).
- The server resolves ink on every `annotations` read (nothing is cached): a stroke belongs to the **smallest** anchor containing its centre,
  and `GET /pages/<id>/annotations` (MCP: `penwise_page_annotations`) reports strokes per selector plus how many
  fall outside all of them. Verified: three strokes over a two-element map produced `1 / 1 / unanchored: 1`.

## Not yet

- No full-page stitching: only the visible viewport is captured.
- Stale selectors are not detected; attribution is recomputed from the page's current items on every read, so ink
  is never mis-attributed from a cache, but a map from an older layout is not flagged.
