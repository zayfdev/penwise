# Pages contract (this pass)

The long-term model lives in `crates/penwise-proto/src/notebook.rs` (op log, HLC, SQLite). This pass
implements the same *shapes* in the running server's plain-JSON style, so the surfaces agree today and the
op log can replace the transport later without changing the wire shapes.

## On disk (`$PENWISE_DATA`)

`meta.json`:

```json
{
  "version": 1,
  "active": "pg_0123456789abcdef",
  "notebooks": [
    {
      "id": "nb_0123456789abcdef",
      "title": "Notebook 1",
      "pages": [
        {
          "id": "pg_0123456789abcdef",
          "title": "Page 1",
          "kind": { "t": "fixed", "width": 1600.0, "height": 2560.0 },
          "background": "blank"
        }
      ]
    }
  ]
}
```

- `kind`: `{"t":"fixed","width":W,"height":H}` or `{"t":"infinite"}`. Default `fixed 1600×2560` (A4 portrait at
  the current render scale).
- `background`: `"blank" | "ruled" | "grid" | "dots"`.
- IDs: `nb_` / `pg_` plus 16 lowercase hex characters.
- Strokes stay per page in `pages/<page id>.json`, existing shape: `{"version":1,"strokes":[…]}`.
- Migration: if `meta.json` is missing and the old `page.json` exists, create `Notebook 1 / Page 1` (fixed A4,
  blank), move its strokes to `pages/<id>.json`, write `meta.json`, keep `page.json` untouched.

## Backgrounds

Drawn under the strokes, identical in the browser and in the PNG:

| Key | Pattern | Spacing | Colour | Stroke / size |
|---|---|---|---|---|
| `blank` | none | — | — | — |
| `ruled` | horizontal lines | 93.7 px | `#C9D6E8` | 1 px |
| `grid` | squares | 93.7 px | `#DCE3EC` | 1 px |
| `dots` | dots at intersections | 93.7 px | `#C6CEDA` | radius 1.5 px |

Spacing and sizes are page pixels (before the `scale` multiplier). The PNG has no border; the browser draws the
canvas shadow.

## HTTP (all token-authenticated as today)

| Route | Body | Response |
|---|---|---|
| `GET /pages` | — | `{"version":1,"active":"pg_…","notebooks":[…]}` |
| `POST /pages` | `{"title"?, "kind"?, "background"?, "notebook"?, "after"?}` | `{"page":{…},"meta":{…}}` |
| `PATCH /pages/<id>` | `{"title"?, "background"?}` | `{"page":{…},"meta":{…}}` |
| `DELETE /pages/<id>` | — | `{"ok":true,"meta":{…}}`; `400` when it is the last page |
| `POST /notebooks` | `{"title"}` | `{"notebook":{…},"meta":{…}}` |
| `POST /pages/<id>/select` | — | `{"active":"pg_…"}` |
| `GET /page?page=<id>` | — | `{"version":1,"strokes":[…]}` for that page |
| `GET /page.png?page=<id>&scale=<n>` | — | PNG of that page; `page` defaults to active |

`POST /pages` defaults: title `Page N` (N = notebook page count + 1), `kind` fixed A4, `background` `blank`,
`notebook` = the notebook holding `active`.

Infinite pages in `/page.png`: bounds are the stroke bounding box padded 200 px, at least 1200×900, each side
capped at 8000 px after scaling.

## WebSocket

Existing frames gain `"page":"pg_…"`. New client frame `{"t":"select","page":"pg_…"}`.

Server → client:

| Frame | Meaning |
|---|---|
| `{"t":"meta","meta":{…}}` | sent on connect and whenever pages/notebooks change |
| `{"t":"active","page":"pg_…"}` | the active page changed |
| `{"t":"page","page":"pg_…","strokes":[…]}` | full snapshot, sent on select/connect and after bulk edits |
| `{"t":"stroke",…,"page":"pg_…"}` | committed stroke on that page |
| `{"t":"live",…,"page":"pg_…"}` | in-progress stroke sample batch on that page |

A session receives stroke/live/page frames only for its selected page; `meta` and `active` go to everyone.
Frames without a page are applied to the sender's selected page (back-compat with the current app build).

## MCP

| Tool | Args | Result |
|---|---|---|
| `penwise_list_pages` | — | `{"active":"pg_…","notebooks":[…]}` |
| `penwise_create_page` | `{"title"?, "background"?, "kind"?}` | `{"page":{…}}` |
| `penwise_switch_page` | `{"page":"pg_…"}` | `{"active":"pg_…"}` |
| `penwise_page_png` | `{"page"?, "scale"?}` | image |
| `penwise_page_status` | `{"page"?}` | `{"ok":true,"strokes":N}` |
| `penwise_annotate` | `{"page"?, "strokes":[…]}` | `{"added":N}` |
| `penwise_clear_agent_marks` | `{"page"?}` | `{"removed":N}` |

`page` defaults to the active page everywhere.

## App

- Page strip above the canvas: one chip per page in the active notebook plus `＋` (new page).
- New page dialog: title field, template picker (Blank / Ruled / Grid / Infinite), Create.
- Selecting a chip swaps the local store, clears undo history, sends `select`, repaints.
- Each page persists to `pages/<page id>.json` under app files; the old `penwise-strokes.pwst` migrates to
  the first page.
- Chips show a background glyph (Blank ▢, Ruled ☰, Grid ▦, Infinite ∞) and the page title.
