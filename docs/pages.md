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
| `PATCH /notebooks/<id>` | `{"title"}` | `{"ok":true,"meta":{…}}` |
| `DELETE /notebooks/<id>` | — | `{"ok":true,"meta":{…}}`; `400` when it holds pages or is the last one |
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

### Op frames

Edits that are not a whole new stroke travel as ops, so the mirror applies them without a page resend:

| Frame | Effect |
|---|---|
| `{"t":"op","page":…,"op":{"k":"add","stroke":{…}}}` | append one stroke |
| `{"t":"op","page":…,"op":{"k":"remove","seeds":["…"]}}` | drop strokes by seed |
| `{"t":"op","page":…,"op":{"k":"replace","seeds":[…],"strokes":[…]}}` | swap strokes for fragments (precise erase) |
| `{"t":"op","page":…,"op":{"k":"clear"}}` | empty the page |

The server replies `{"t":"ack",…}` to the sender and fans the op to the other sessions watching that page.

### Live previews

`{"t":"live",…}` frames are relayed, never stored:

| `kind` | Fields | Meaning |
|---|---|---|
| absent | `id`, `preset`, `argb`, `scale`, `seed`, incremental `samples`/`times` | in-progress freehand stroke |
| `shape` | `id`, `preset`, `argb`, `scale`, `seed`, whole `samples`/`times` | shape drag; each frame replaces the previous |
| `erase` | `x`, `y`, `r` | eraser position, drawn as a fading mark |

A frame naming a page the server does not know is refused (`{"t":"error"}`), so a stale client can never write to
another page.
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

- **Home screen** first: notebooks with their pages as cards (glyph + name + template), `＋` for a new page
  inside each notebook, a pencil to rename (and, when a notebook is empty, delete) it, `＋ New notebook`, and the
  sync settings. Opening a card enters the canvas; the home icon (or system back) returns.
- Toolbar is icon-only: the 13 brushes are previews drawn by the engine itself, colours are swatches, and the
  eraser, precise-erase, shapes, undo, redo, clear, sync and home are path icons with screen-reader labels.
- Page strip above the canvas: one chip per page in the active notebook plus `＋` (new page).
- New page dialog: title field, template picker (Blank / Ruled / Grid / Infinite), Create.
- Selecting a chip swaps the local store, clears undo history, sends `select`, repaints.
- Each page persists to `pages/<page id>.json` under app files; the old `penwise-strokes.pwst` migrates to
  the first page.
- Chips show a background glyph (Blank ▢, Ruled ☰, Grid ▦, Infinite ∞) and the page title.

## Handwriting → text

The page PNG is read by a **local** vision model (Ollama) and the transcription cached beside the page:

| Route | Purpose |
|---|---|
| `GET /text?page=<id>&refresh=1` | `{"page":…,"text":"…","cached":false}`; the second call is a cache hit |
| `GET /page.png` | unchanged — agents can still look at the ink themselves |

- Model: `PENWISE_OCR_MODEL` (default `qwen2.5vl:3b`), server: `PENWISE_OCR_URL` (default
  `http://127.0.0.1:11434`). Nothing leaves the machine; when no model is running the route answers `503` and
  the rest of the surface keeps working.
- Only the **ink** is sent, cropped to the stroke bounds with a 48 px margin and capped at 1400 px per side: a
  full A4 render is slower to read and no clearer, and the crop took a page from >120 s to ~29 s on CPU.
- The result is cached in `pages/<id>.txt` and dropped when the page is deleted; `refresh` re-reads.
- Quality note: a 3B model reads upright handwriting closely but not perfectly (it dropped a leading `0` and
  merged one word in testing). Set `PENWISE_OCR_MODEL=qwen2.5vl:7b` for accuracy at roughly double the time.

## Pairing

Copying the 43-character token by hand is the clumsy part of setting up a tablet, so the server also mints a
**pairing code**: six characters from an unambiguous alphabet (`A–Z` without `I`/`O`, `2–9`), valid for 10
minutes, single use.

| Route | Auth | Body / result |
|---|---|---|
| `POST /pair` | none — the code is the credential | `{"code":"6WSGM4"}` → `{"ok":true,"token":"…","urls":["http://192.168.1.19:7420",…]}` |
| `GET /pair/code` | token | `{"code":"…","expires_in":599}` |
| `POST /pair/code` | token | mints a new code (the old one dies) |

The exchange is rate limited: eight wrong codes inside a minute lock it for a minute. The viewer's ⇄ button shows
the current code (and rotates it); in the tablet's Sync dialog, six characters in the token field are treated as a
code, exchanged for the real token and the server's LAN URL before connecting.

Still missing from this item: a QR image and an in-app scanner (so a phone could pair the tablet), and TLS — the
LAN is still plaintext HTTP.

## Undo history

Each page keeps its undo/redo stacks beside its strokes: `pages/<id>.undo` (`PWUN` v1). A step stores the exact
strokes it needs — added, removed with their positions, or swapped (precise erase) — using the same binary stroke
records as the page file, so reopening the app lets you undo edits from the previous session. Writes are
debounced (400 ms) and atomic; the stacks are capped at 200 steps.

Steps address strokes by seed, not by object identity, so a history loaded from disk still applies to strokes
read back from the page file.

## Precise erase

The eraser has two modes, toggled from the toolbar while the eraser is selected:

| Mode | Behaviour | Mirrors as |
|---|---|---|
| whole stroke (default off) | any touched stroke disappears | `op remove` |
| **Precise** (default on) | strokes are cut at the sweep; each piece survives with the same brush, colour and size | `op replace` (old seeds out, fragments in) |

The cut itself runs in Rust (`crates/penwise-ffi/src/erase.rs`): for one stroke it returns the sample runs whose
centreline stays at least the eraser radius away from the sweep, and the app slices its own arrays. One drag, one
undo step, one op.

Not built yet: fill bucket, lasso/selection, moving or reshaping items after they are drawn.
