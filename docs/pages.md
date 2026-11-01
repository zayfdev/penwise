# Pages, items and ops (schema v4)

The page model is `penwise-core`'s `Item`/`Scene`; everything that changes it is an **op** in an append-only log
(`penwise-proto`), stamped with a hybrid logical clock and resolved last-writer-wins per field. Design record and
owner decisions: `docs/rewrite-design.md`. This file describes what the code does today.

## Notebooks, pages

- **Notebook** `{id, title, position, deleted}` and **page** `{id, notebook, title, position, background, deleted,
  kind}` are catalog rows (`crates/penwise-proto/src/notebook.rs`, `catalog.rs`). Ids are 26-character ULIDs
  (`ids.rs`). `title`, `position`, `background` (pages) and `deleted` are LWW registers; `kind` and the notebook a
  page belongs to are fixed at creation.
- `PageKind` is `Fixed {width, height}` or `Infinite`. New pages through `POST /pages` default to fixed
  2560 × 1600 (landscape, the tablet's screen; `api/pages.rs` `DEFAULT_WIDTH/HEIGHT`), title `Page N`, background
  `blank`, in the notebook of the current page (a library with no notebook gets one called `Notebook`).
- Deleting is a tombstone. The last live page cannot be deleted (`400`); a notebook that still holds pages or is
  the last one cannot be deleted either.
- The "current page" the tablet and MCP follow is `server_meta` state, set by `POST /pages/<id>/select`
  (`storage::Storage::set_current_page`); it is not an op.

## Items (`penwise-core::item`)

An `Item` is `{id, layer, z, transform, kind}`; geometry is always re-derived from the data.

| Kind | Data |
|---|---|
| `Stroke` | brush params, seed (`u64`), raw samples, optional visible-range `Trim` (precise-erase pieces) |
| `Erase` | precise-eraser mask attached to one stamp stroke or shape |
| `Shape` | rectangle, rounded rectangle, ellipse, line, arrow, polygon, optional fill |
| `Fill` | polygons with holes, colour |
| `Text` | content, box, font size, source stroke ids |
| `Image` | asset hash + box |

- **Layers**: `Ink`, `Agent`, `Text` (`item/layer.rs`). Agent marks are ordinary strokes on the `agent` layer;
  promoting one is an `Update{layer}`.
- **Order**: items paint by `(z, id)` ascending; `z` is a plain `i64` with `Z_STEP = 65536` between consecutive
  values (`item/mod.rs`).
- **Edits** (`edit/mod.rs`): `Add(Item)`, `Delete(ItemId)` (a tombstone, never a hard remove) and
  `Update{id, patch}` with absolute values. Undo (`edit/undo.rs`) is local, in-memory state that emits ordinary
  edits; it is never synced and resets when the app or tab restarts (owner decision 2).

## Ops, HLC, LWW

- `AnyOp` = `Item(Op)` | `Page(PageOp)` | `Notebook(NotebookOp)` (`proto/src/op.rs`); every op carries
  `{op_id, device, hlc}` and the server assigns each accepted op a strictly increasing `seq`.
- `Hlc` = `{wall_ms, counter, device}`, totally ordered; a remote stamp more than `MAX_SKEW_MS` (5 min) ahead is
  refused (`hlc.rs`).
- Item registers (`lww.rs`): one per core `Field` (`Deleted, Layer, Z, Transform, Brush, ShapeFill, Paint,
  Polygons, ShapeKind, ShapeSize, Text, TextBox, FontSize`); a write wins iff its stamp is greater. Applying any
  set of ops in any order, with duplicates, converges.
- The server refuses (`OpRejection`): another device's op, clock skew, invalid payload, unknown page / notebook /
  item, duplicate id under a different op id, item on the wrong page.
- Wire: binary postcard frames over `/sync` (`envelope.rs`, `message.rs`); `docs/rewrite-design.md` §4.

## Server storage (`crates/penwise-server/src/storage`)

`penwise.db` (SQLite, WAL), `PRAGMA user_version = 6` (`schema.rs`; step 5 added `rounds`, step 6 `generations` and `strip_pages`):

```
server_meta(key, value)            devices(id, name, kind, created_ms, last_seen_ms, acked_seq)
ops(seq, op_id, batch, device, hlc, kind, notebook, page, item, body, received_ms)
notebooks(...registers + hlcs)     pages(...registers + hlcs)
items(id, page, kind, layer, z, deleted, body, patch, stamps, added_seq, updated_seq)
id_alias(legacy, kind, id)         image_labels(item, label)
rounds(project, n, status, sent_seq, resolved_seq, sent_ms, resolved_ms, looked_ms)   -- design-harness rounds
generations(project, round, route, preset, role, label, picked, item, asset, w, h)   -- pictures a resolve kept
strip_pages(seq, project, route, preset, page)   -- sketch pages attached to a screen's strip in the overview
```

- One writer thread (`storage/writer.rs`) validates, logs, materializes and commits; reads use separate WAL
  connections.
- A v1–v3 library is refused until `penwise-server migrate --db <path>` has converted it (`storage/migrate/`); the
  migration gives legacy strokes deterministic ids so a tablet importing its own old files converges with it.
- **Retention** (`storage/compact.rs`, constants in `storage/types.rs`): ops older than `RETENTION_DAYS = 7` are
  dropped and tombstoned items past the cutoff purged; a device unseen for `DEVICE_IDLE_DAYS = 30` stops holding
  the floor; browsers (`kind = 1`) never hold it, and connected tablets/agents re-record their delivery cursor as
  their ack every minute. Compaction runs once a day (and once when the server starts, off the listening path) in
  slices of 5,000 rows, one transaction each, so pushes and acks interleave; after a slice it runs
  `PRAGMA incremental_vacuum`, which only shrinks the file if the database was created with
  `auto_vacuum = INCREMENTAL` (existing files are `NONE` and would need a one-off `VACUUM`). Compaction raises `log_floor`; a client that has seen less than the floor receives a snapshot
  (`SnapshotBegin/Chunk/End`, `SNAPSHOT_CHUNK_ITEMS = 512`) instead of a diff.

## Restore

`GET /pages/<id>/history?limit=N` lists the page's op batches newest first
(`{seq, last_seq, device, received_ms, ops, summary}`, default 50, max 500). `POST /pages/<id>/restore?to_seq=N`
(`N` = a batch's `last_seq`) appends **compensating ops** from the server device (`storage/restore.rs`), so a
restore is an ordinary logged change and itself restorable. Items added after `N` are deleted; items whose whole
history is still retained are rolled back field by field; items added at or before the log floor cannot be
rolled back (their `Add` is gone): if one also changed after `N` it is listed in the answer's `skipped` array (ids,
empty when the restore was complete) and keeps its current state.
`404` for an unknown page or a seq never logged, `410` below the retained log. This replaces the pre-rewrite
`object_history` archives.

## Pictures

An `Image` item names an asset: `assets/<sha256>.png` under the data directory, content-addressed, so the same
screenshot placed twice is one file (`assets.rs`). PNG only, up to 24 MB and 10 000 px a side, never re-encoded;
ids are digests, nothing else is served. A placed picture is an `Add(Image)` from the server device on the `ink`
(default) or `agent` layer; its label lives in `image_labels` (`pictures.rs`). `GET /page.png` paints paper, then
the scene (pictures sit in their z band under ink).

## Text layer

Recognized handwriting is stored as `Add(Text)` items on `Layer::Text` whose `sources` are the strokes they were
read from (`ocr.rs`, `textlayer.rs`). A page's text is the union of its live text items in reading order; a page
whose ink strokes are not all covered by some text item's sources is read again (or when `refresh=true`; `1`/`0` work too). The model
is a local Ollama (`PENWISE_OCR_URL`, default `http://127.0.0.1:11434`; `PENWISE_OCR_MODEL`, default
`qwen2.5vl:3b`); with no model reachable `GET /text` answers `503` and everything else keeps working. Only the
ink is sent, cropped to its bounds + 48 px, at most 1400 px a side. `GET /search?q=` scans the live text items of
every live page (case-insensitive, 60 chars of context, at most 50 hits). Typed text items are searched too.

## Backgrounds

Drawn under the items, identical in the browser, the tablet and the PNG (`BACKGROUND_SPACING_PX = 93.7`,
`proto/src/notebook.rs`; colours in `penwise-server/src/render/paper.rs`):

| API key | Pattern | Colour |
|---|---|---|
| `blank` | none | — |
| `ruled` | horizontal lines | `#C9D6E8` |
| `grid` | squares | `#DCE3EC` |
| `dots` | dots at intersections | `#C6CEDA` |

## HTTP (token: `?token=` or `X-Penwise-Token`; `POST /pair`, `GET /pair/local` and the static viewer are public; the `/projects`, `/agent`, `/frames` and `/marks` routes also accept a request from this machine that carries no foreign `Origin`)

| Route | Purpose |
|---|---|
| `GET /pages` | `{"version":4,"active":"<id>","notebooks":[{id,title,pages:[{id,title,notebook,kind,background}]}]}` |
| `POST /pages` `{title?, kind?, background?, notebook?}` | create (`kind`: `{"t":"fixed","width","height"}` or `{"t":"infinite"}`) |
| `PATCH /pages/<id>` `{title?, background?}`, `DELETE /pages/<id>` | rename / re-ground, tombstone |
| `POST /pages/<id>/select` | set the current page |
| `POST /notebooks`, `PATCH/DELETE /notebooks/<id>` | notebook catalog |
| `GET /page.png?page=&scale=` | PNG (`scale` 0.05–4, default 1; sides capped at 8000 px). Infinite pages: content bounds + 200 px, at least 1200 × 900 |
| `POST /assets`, `GET /assets/<hash>` | picture bytes |
| `GET/POST/DELETE /pages/<id>/images` | list / place / remove pictures |
| `POST /annotate`, `POST /annotate/clear` | agent marks (`{"strokes":[{points,color?,width?}]}`) |
| `GET /pages/<id>/history`, `POST /pages/<id>/restore?to_seq=` | restore |
| `GET /text?page=&refresh=`, `GET /search?q=` | handwriting as text |
| `GET /health?page=` | `{ok, page, strokes, pages, head_seq}` |
| `POST /pages/<id>/focus {item?}` | every connected device switches to the page and fits the frame `item` (none = the page's content) |
| `GET /frames/<id>/still` | the frame's still: `{version, doc, viewport, strips:[{y,h,url}], layout}` (ETag; for clients with no live view) |
| `GET /frames/<id>/strip/<hash>.png` | one content-addressed strip of a still (cached forever) |
| `GET /frames/<id>/hit?x=&y=` | the element under a document point (pen-down outside the live window) |
| `GET /frames/<id>/snapshot.png?ink=` | the frame's whole box as PNG, scaled down past 40 Mpx (`X-Penwise-Doc-Height`, `X-Penwise-Scale`) |
| `POST /projects/<id>/rounds/send {n?}` | closes the open round (`{n, marks}`); 409 if round `n` is no longer open or has no marks |
| `GET /projects/<id>/rounds` | every round with status and mark counts |
| `POST /projects/<id>/rounds/<n>/resolve` | freezes the round into history (picture, frame moved right, variants cleared, marks done) |
| `GET /projects/<id>/history?route=&preset=` | the lane's kept generations, newest first: `{route, preset, generations:[{round, resolved_ms, og:{item, asset, w, h}, variants:[{label, item, asset, w, h, picked}]}]}`. `item` is the Image item on the board, `asset` the id to fetch with `GET /assets/<asset>`. Every picture of every variant group is listed (labels may repeat across groups), and each group's chosen variant has `picked: true`; a variant spared because it was marked after Send is not pictured and so never picked. `route` is required, `preset` defaults to `desktop`; only resolved rounds are listed |
| `PUT /projects/<id>/strip-pages/<page>` `{route, preset}` | attaches the sketch page to that screen's strip (idempotent; to another strip it moves); 404 unknown project or page, 400 bad preset, empty route, the board page itself or a page outside the project's board notebook |
| `GET /projects/<id>/strip-pages` | `[{page, route, preset}]` in attach order; pages deleted from the library drop out |
| `GET /projects/<id>/inbox` | `{"line": "Penwise: round 3 sent — …"}` or `{"line": null}` |
| `POST /agent/look`, `POST /agent/show`, `POST /agent/resolve` | the MCP tools' server side (`docs/mcp.md`) |
| `GET /pair/local` | `{token}` only to a loopback peer with a loopback `Host` (the desktop launcher's viewer); else 403 |
| `POST /pair`, `GET/POST /pair/code`, `GET /pair.png` | pairing |
| `WS /sync` | binary sync protocol (authenticates in its handshake) |

All writes are ops from the server device, so tablets and browsers receive them through the normal broadcast.

## Pairing

The server mints a six-character code (alphabet `A–Z` without `I`/`O`, `2–9`), valid 10 minutes
(`pairing.rs`: `CODE_TTL`, `ALPHABET`). `POST /pair {"code"}` → `{"ok":true,"token","urls":[…]}`; eight wrong
attempts lock the exchange for a minute. `GET /pair/code` (and `POST` to rotate) and `GET /pair.png` need the
token; the QR carries `penwise://pair?code=XXXXXX&url=http://<lan>:7420`, which the system camera opens in the
app. The token is one shared secret in `$PENWISE_DATA/token` (or `PENWISE_TOKEN`); transport is cleartext on the
LAN (security hardening is pending, see `docs/next-iteration.md`).

## Clients

- **Tablet** (`android/`): Kotlin shell; page model, undo, erase, shapes, lasso and sync live in Rust through
  `penwise-ffi` (`SceneHandle`, `SyncCore`). The whole sync state (clock, outbox, replica, server pin) is one blob
  written atomically by `StatePersister` (`sync/SyncStateFile.kt`). Pre-rewrite page files (`pages/<id>.bin`,
  magic `PWST`) are read once by `penwise-ffi/src/sync_core/legacy.rs` and imported as ops; they are no longer a
  live format. A foreground service keeps the socket alive while the screen sleeps (`ink/SyncService.kt`).
- **Browser** (`web/`, `penwise-wasm`): `docs/draw.md`.
- **Agents**: `docs/mcp.md`.
