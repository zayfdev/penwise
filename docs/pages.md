# Pages contract (this pass)

The long-term model lives in `crates/penwise-proto/src/notebook.rs` (op log, HLC, SQLite). This pass
implements the same *shapes* in the running server's plain-JSON style, so the surfaces agree today and the
op log can replace the transport later without changing the wire shapes.

## State on disk (`$PENWISE_DATA`)

Pages live in **SQLite** (`penwise.db`), one row per object, so an append writes a row instead of rewriting a
page. The old `meta.json` + `pages/<id>.json` layout is read once and imported.

Why: the JSON layout stored a stroke as raw float arrays (**11 bytes per stored float**, measured: 0.41 MB for
110 strokes) and rewrote the whole page file on every pen stroke, which is quadratic over a session and cannot
carry pictures.

```
notebooks(id, title, position, deleted)
pages(id, notebook, title, kind, background, position, deleted)
objects(page, id, kind, layer, z, codec, payload)   PRIMARY KEY (page, id), INDEX (page, z, id)
anchors(page, selector, rect, role, text, url, stale)   -- DOM map, for anchoring annotations
state(key, value)                                       -- e.g. the active page
```

- `kind`: `"stroke"` or `"image"`; ordering is `(z, id)` ascending (bottom → top).
- `codec`: `"stroke_v1"` ([`library::stroke_codec`]) or `"image_v1"` (JSON: `asset, x, y, w, h, label`).
- `stroke_v1` carries the same numbers losslessly: header (`preset, argb, scale, seed, count`) then x/y, then
  pressure/tilt/azimuth, then times — 24 bytes per sample instead of ~56 in JSON. Quantizing to 16-bit was
  considered and rejected: it buys ~2× more and costs exactness.
- Pictures are never stored in the database: `image_v1` names an asset in `assets/<sha256>.png`.
- The in-memory copy of every page is decoded at startup (a few MB); the database is the source of truth.
- The index (`kind`, titles, background, active page) upserts rather than replace-all, because deleting a
  `pages` row cascades to its objects.

### Stroke identity (seeds)

A stroke's seed is an opaque **string**: the tablet sends `Long.toString()`, so seeds above 2^63 arrive negative.
The server stores the seed as the object id and reads it back from there — the `stroke_v1` blob's `u64` copy
cannot represent those, and using it collapsed every such stroke to seed `0` (127 strokes looked like one, and
removals hit all of them). Old strokes with no seed are given one by the tablet on open.

### The tablet's stroke files

`SavedStrokeCodec` (Android) reads and writes one layout: per sample, the five floats
(`x, y, pressure, tilt, azimuth`) then its `time` int. No writer ever produced another, and a "detect the layout
from the data" heuristic misread 2- and 3-sample strokes (Line tool, tiny arrows) with zero tilt, so it is gone.

Page and undo files are rewritten atomically (temp file, fsync, rename), and every read, append and rewrite runs on
one single-threaded writer (`PageIo`), so a rewrite can never overtake an append.

### Pictures

`assets/<sha256>.png`, content-addressed: the same screenshot placed twice is one file. Ids are 64 hex
characters; anything else is refused (no traversal). PNG only, up to 24 MB and 10 000 px a side, never
re-encoded.

### Export

`GET /page.png` draws paper, then pictures, then ink, so an agent sees exactly what the page looks like.

## Backgrounds## Backgrounds

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
| `GET /page.png?page=<id>&scale=<n>` | — | PNG of that page (paper, pictures, ink); `page` defaults to active |
| `POST /assets` | PNG bytes | `{"asset":"<sha256>","width":W,"height":H}` |
| `GET /assets/<id>` | — | the PNG, immutable (long cache) |
| `GET /pages/<id>/images` | — | `{"page":"pg_…","images":[…]}` |
| `POST /pages/<id>/images` | `{"asset","x","y","w","h","label"?,"id"?,"layer"?}` | the placed picture, with its id |
| `DELETE /pages/<id>/images` | `{"ids":[…]}` | `{"removed":n}` |
| `GET /pages/<id>/anchors` | — | `{"page":"pg_…","anchors":[{selector, rect, role?, text?, url?}]}` |
| `POST /pages/<id>/anchors` | `{"anchors":[…]}` | `{"stored":n}` — replaces the element map, then re-attributes ink |
| `GET /pages/<id>/annotations` | — | `{"page":…,"elements":[{selector,rect,role,text,strokes}],"unanchored":n}` |
| `GET /pages/<id>/history` | — | `{"page":"pg_…","archives":[{"at":MS,"objects":n}]}`, newest first |
| `POST /pages/<id>/restore` | `{"at"?}` | `{"restored":n}`; newest archive when `at` is absent, `404` when none |

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
| `{"t":"page","page":"pg_…","strokes":[…],"images":[…]}` | full snapshot, sent on select/connect and after bulk edits |
| `{"t":"image","page":"pg_…","image":{…}}` | a picture was placed on that page |
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

### Server-side archives (the safety net)

Every destructive edit — clear, erase, stroke replace, picture delete — first copies the page's objects into
`object_history`, keeping the newest 10 archives per page. `POST /pages/<id>/restore` puts one back and pushes a
fresh page frame to every client.

This exists because a client can lose ink the server never had a copy of: on 2026-10-25 a page's saved strokes
were wiped on both the tablet and the server while only the tablet's undo history survived. A destructive frame
is now always reversible from the server, independently of any client's undo stack.

## Pictures and the design surface

Pictures are what turn the page into a design surface: a webpage screenshot to mark up, a component cut out of
a running app, a reference image. They sit on the paper under every stroke, so ink annotates them and the page
PNG an agent receives shows both.

- **Sources:** a Chrome extension captures the visible tab or a chosen element (with its selector and rect); a
  grabber CLI renders components from a dev server; agents place pictures with `penwise_place_image`.
- **Anchoring (landed):** a capture stores the page's element map (`anchors`: selector, rect in page px, role,
  text). Every ink change re-resolves strokes to the **smallest element containing the stroke's centre**, so an
  agent asking `GET /pages/<id>/annotations` learns "one mark on `.hero__cta`" instead of only coordinates.
  Rewriting the map (a newer capture) re-attributes existing ink.
- Still open: stale selectors when the page has changed (the `stale` column exists, nothing sets it).

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

- **Paper**: the tablet draws the page's ruling/grid/dots itself (`PaperPainter`), with the same colours, spacing
  (93.7 page px) and line widths as the server's PNG. Default sheet is **landscape 2560 × 1600** — the tablet's
  own screen, so ink maps 1:1; older portrait pages are migrated on load.
- **Foreground sync**: while the app runs it holds a `dataSync` foreground service with a Wi-Fi lock and pings the
  socket every 20 s. Without them Android suspends the app when the screen sleeps and the socket dies within
  seconds; with them the connection stayed established through 75 s of screen-off in testing. No CPU wake lock:
  the pings wake the processor briefly through the Wi-Fi chip, which is enough.
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
| `GET /search?q=…` | `{"query":…,"hits":[{page,title,snippet}]}` over every page whose text has been read |

- Model: `PENWISE_OCR_MODEL` (default `qwen2.5vl:3b`), server: `PENWISE_OCR_URL` (default
  `http://127.0.0.1:11434`). Nothing leaves the machine; when no model is running the route answers `503` and
  the rest of the surface keeps working.
- Only the **ink** is sent, cropped to the stroke bounds with a 48 px margin and capped at 1400 px per side: a
  full A4 render is slower to read and no clearer, and the crop took a page from >120 s to ~29 s on CPU.
- The result is cached in `pages/<id>.txt` and dropped when the page is deleted; `refresh` re-reads.
- The viewer's ☰ button shows the active page's text, reading it on demand (cached reads are instant).
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
| `GET /pair.png?size=8` | token | the deep link below as a QR PNG |

The QR carries `penwise://pair?code=XXXXXX&url=http://<lan>:7420`. Scanning it with the **system camera** opens
Penwise already pointed at that server — the app exchanges the code for the token itself, so there is no custom
scanner screen. The same URL works with `adb shell am start -a android.intent.action.VIEW -d …`.

Sync accepts either form of address: a viewer URL (`http://host:7420`) or a socket URL (`ws://host:7420/ws`)
is normalised to the socket before dialling.

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
