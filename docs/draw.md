# The browser viewer and pen client

`web/` is one TypeScript app (Vite, built to `web/dist`, served by `penwise-server`). It views **and** draws:
sidebar of notebooks/pages from the synced catalog, the page on a HiDPI canvas, live ink from other devices, a
pen/mouse tool, own undo/redo, a text panel (`/text`), search (`/search`) and a pairing panel (`/pair/code`,
`/pair.png`). On the server's own PC, run `just install-desktop` once: it installs the server as a login service
(`penwise.service`) and a **Penwise** launcher that opens the viewer as its own window at `http://127.0.0.1:7420/`;
the viewer fetches the token itself from `GET /pair/local` and follows the tablet's page. From another machine open
`http://host:7420/?token=…` (the token is kept in `localStorage`; without one the viewer asks for a pairing code
and exchanges it through `POST /pair`).

## How it works

- **One socket, binary only.** `/sync` carries postcard frames. `penwise-wasm` is the only code that encodes or
  decodes them: `Viewer.receive(frame)` applies a server frame to its `Replica` + live strokes and returns a
  summary (`Received`: pages to repaint, catalog changed, snapshot scope, …); `begin_session`, `poll_push`,
  `encode_watch` build what goes out. `web/src/net/connection.ts` only moves bytes: Hello/Welcome, `Watch` of the
  shown page, ping/pong, reconnect with backoff (a fresh catalog snapshot follows every `Welcome`; the page
  snapshot follows `Watch`).
- **Rendering.** `Viewer.render_page_paths` / `render_live_paths` return a flat `DrawList` (layout in
  `crates/penwise-wasm/src/draw_list.rs`) painted by `web/src/render/` on Canvas2D: paths, dab strokes with paper
  grain, erase groups, text, pictures (`/assets/{hash}`). The viewer never computes stroke geometry.
- **Two canvases.** `committed` (paper + all committed items) repaints only when the committed scene, the view or
  the size changes; `live` (in-progress strokes, other devices' and our own) is cleared and redrawn per frame, so a
  live frame never re-rasterizes the page (`PageView.stats` counts both; the e2e asserts it).
- **Drawing.** Pointer events (pen, mouse; touch pans) → `getCoalescedEvents()` with their real timestamps →
  `pointer_samples` (wasm; pressure and `tiltX/tiltY` → core samples through the same `DigitizerRanges::map` as the
  tablet's raw pen frames) → `Viewer.stroke_push` (returns a `StrokeDelta` frame, previews locally) → `stroke_end`
  builds the core `Add(StrokeItem)`, stamps it with the calibrated HLC and queues it; `poll_push` sends `PushOps`
  once welcomed and not mid-snapshot, one in flight, and unacknowledged ops are re-applied over every snapshot.
  `pointercancel` → `StrokeCancel`. Ctrl/Cmd+Z / Shift+Z (or the buttons) → `Viewer.undo/redo`: core `UndoStack`
  emits ordinary ops for this client's own edits only.
- **Device identity.** Token in `localStorage`; device id per tab (`sessionStorage`), because ops are stamped per
  device.

## Tests

- `npm test` (vitest): connection (real wasm, committed wire fixtures, fake socket/clock), pen input, sample
  packing, view maths, page selection, API client, painter helpers, paint counters.
- `wasm-pack test --node --release crates/penwise-wasm` and `cargo test -p penwise-wasm`: viewer, draw lists,
  authoring (live deltas, commit, push gate, ack/reject, snapshot re-apply, undo/redo, pointer mapping).
- `npm run e2e`: starts the real `penwise-server` binary (temp data dir, free port), serves the built viewer and
  drives headless Chromium (`playwright-core`, system Chromium or `$CHROME`): another device's stroke appears in
  the live canvas without repainting the committed layer, then committed; a server-authored `/annotate` stroke
  appears; a stroke drawn with the mouse lands in `/page.png` at the right place and undo/redo reach the server;
  page creation over HTTP; a stroke drawn offline survives a reload and is delivered after reconnecting. Needs `cargo build -p penwise-server`, `npm run wasm` and Chromium.

## Limits

- Pen and mouse only draw ink (no eraser, shapes or lasso in the browser yet); touch pans.
- Strokes drawn while disconnected survive reconnects, a reload and a closed tab: the outbox, dead letters and
  device id are saved in `localStorage` (`penwise.outbox.v1.<device>`, written on every change, removed when
  nothing is unsent or refused). Each tab holds a Web Lock `penwise-device-<id>`; a new tab with no device of its
  own adopts a saved device no live tab holds (and so delivers its ink), else mints one. Leaving with unsent ops
  asks for confirmation (`beforeunload`).
- Ops the server refuses are rebuilt out of the replica (the page reconnects and refills it), shown as "N refused"
  with a list in the sidebar, and can be exported (JSON of the op envelopes) or dismissed.
- Undo history is per tab and resets on reload (owner decision: undo is not persisted).
- Highlighter previews are opaque until committed (the live canvas does not multiply against the page).
