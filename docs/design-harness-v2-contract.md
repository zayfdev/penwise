# Design harness v2 — V1/V2 contract

The fixed interface for the full-length frame work (spec v2 §3, milestones V1 + V2). Implementers code against
it and never change it; a needed change goes back to the orchestrator. File:line references are to `18f166f`.

## 1. One idea: the box is the document, the live view is a sliding window

- A frame's **box** on the board is `pos + [css_w, box_h]`, where for `extent: full`
  `box_h = max(css_h, doc_h)` and for `extent: viewport` (v1) `box_h = css_h`.
- `css_w × css_h` stays the **viewport** (preset) size. A live view is always exactly that size, so `100vh` and
  breakpoints see the preset.
- Every frame-local coordinate in core is a **document** coordinate. The **view origin** is the document point
  shown at the box's top-left: `[0,0]` for `full`, `layout.scroll` for `viewport`. Everything placed in a
  frame goes to `pos + doc_point − view_origin`. This one rule replaces both v1 formulas
  (`place.rs:309-317`).
- The **live window** (one WebView/iframe) sits at board position `pos + scroll − view_origin`; for `full` that
  is `pos + scroll`. It is positioned by the page's **actual** scroll, never by the camera's wish, so the content
  inside it is always at its true board position and marks cannot swim. A scroll that has not landed yet only
  shows as a strip of still (§4) at the window's edge.

## 2. Core (`penwise-core`) — serial contract step

### 2.1 Frame item
- New field `extent: FrameExtent { Viewport = 0, Full = 1 }` (frozen byte tags). New items default to `Full`;
  items decoded from item codec v3 are `Viewport` (they were built for it).
- New field `doc_h: u32` — the document height in CSS px last measured by the **server**; `0` = unknown.
  Range `0..=MAX_FRAME_SIDE`.
- `fn box_size(&self, layout_doc_h: Option<f32>) -> [f32; 2]`: `full` → `[css_w, max(css_h, layout_doc_h or doc_h)]`;
  `viewport` → `[css_w, css_h]`. Item bounds, hit test, lasso, `validate` and `Placement.clip` use it with
  `None` (the synced hint). Render-time clipping (`Placement.clip`) uses `box_size(Some(layout.doc[1]))` when the
  frame has a layout (live or still), so marks below the first screen show before the server's `doc_h` arrives.
  The result is clamped to `MAX_FRAME_SIDE`.
- LWW fields: `Field::FrameExtent` (21), `Field::FrameDocHeight` (22).
- Codec: `ITEM_VERSION = 4` appends `extent u8; doc_h uv` to a Frame; `EDIT_VERSION = 5` carries fields 21–22.
  v3 items and v4 edits still decode (compat tests with checked-in fixtures of the previous versions, AGENTS.md).

### 2.2 Marks
- `FrameLayout` semantics are unchanged on the wire (bridge sends viewport rects + scroll). Core converts on
  merge: a stored rect becomes **document** coords (`rect + scroll`). Doc comment on `FrameRect` says so.
- Placement: `Area → pos + local − view_origin`; `Bound → pos + rect_doc − view_origin` (+ the stroke's
  binding-local offset, unchanged).
- `add_bound_stroke` / `binding_origin`: the origin is `pos − view_origin` (+ `rect_doc` for bound), so samples
  are stored in document coordinates for area strokes in both extents.
- New pure helper `live_window(frame, layout) -> FrameRect` (board coords): `pos + scroll − view_origin`,
  size `css_w × css_h`. Clients place the live view with it; no client computes this itself.
- New pure helper `doc_point(frame, layout, board_xy) -> [f32;2]` and its inverse, for hit requests and the
  server's crops.
- Tests: every v1 placement test still passes for `Viewport` frames; new tests for `Full` (area mark at
  doc y = 3000 on a 900-high viewport places at `pos.y + 3000` at any scroll; bound mark follows a rect that the
  bridge reports at different scrolls to the same board point).

### 2.3 Proto
- `ServerMsg::Focus { page: PageId, item: Option<ItemId>, by: DeviceId }` appended as the **last** variant (older
  variant indices unchanged). `ClientMsg::Focus { page, item }` likewise. Server relays like `SwitchPage`
  (`sync/session.rs:243-255`). Envelope version bump + compat test decoding the previous fixtures.
- `Focus` means: switch to `page` if needed, then fit `item` (a frame) to the screen (spec §3.1). `item: None`
  = fit the page's content. Receivers treat an unknown or non-frame `item` as `None`.
- `PROTOCOL_VERSION = 2`. A client sends `ClientMsg::Focus` only to a server whose `Welcome` reports ≥ 2.
- The bridge resends a watched rect when its **document** position (`rect + scroll`) changes, not only its
  viewport position; otherwise fixed/sticky elements and scroll anchoring leave stale document rects. The wire
  still carries viewport rects.

## 3. Camera (tablet + web, local only)

- `fitRect(rect, mode)`: `FitWidth` (focus view: frame width = screen width, top of the visible part kept) and
  `FitAll` (board view). Animated 220 ms ease-out-cubic; any touch cancels the animation.
- Tablet `MIN_SCALE` for infinite pages drops to 0.05 (`Viewport.kt:156`) so a long frame fits whole.
- **Focus rule:** the live frame is the frame whose box covers the most of the screen, once its width on screen
  is ≥ 60 % of the screen width. One-finger double-tap on a frame = `FitWidth` on it. Explicit `Focus` messages
  win. At most **one** live view per device; the previous one is kept warm (hidden, paused) for fast return.
- The tablet starts honouring `SwitchPage` (today ignored, `Outcome.kt:91`) and `Focus`. Camera moves from
  the agent are deferred while the owner drew in the last 10 s: the client shows a chip "Agent: <what> →" that
  performs the move on tap.

## 4. Stills (server) — what a non-live frame shows

- `GET /frames/{id}/still` → `{version, doc:[w,h], viewport:[w,h], strips:[{y,h,url}], layout:{rects, state}}`.
  `layout.rects` are **document** coords for the frame's watched keys (bound marks) — clients feed them to core
  as the frame's layout while it is not live.
- `GET /frames/{id}/still/{version}/{y}.png` — strips of at most 4096 CSS px tall, `captureBeyondViewport`,
  viewport kept at `css_w × css_h` (never resized to the document), no ink (clients draw ink themselves).
- Made on: frame created, live reload/HMR settled, patch changed, route changed — debounced 500 ms. Each new
  still writes `FrameDocHeight` (server device op) when the height changed by more than 1 px, clamped so that
  `pos.y + doc_h ≤ MAX_FRAME_SIDE` (validation rejects anything beyond).
- JNI: `Native.kt` is at the 500-line hard limit. The tablet slice splits it into feature objects
  (`core/Native*.kt`) and makes `scripts/check-jni-symbols.sh` read all of them; the limit is not raised.
- The server's mark crops, `context` image and area-mark `element_at` work in document coords: to look up a
  point below the first screen, scroll the headless page so the point is in view, then `getNodeForLocation`
  (`entry.rs:195-205`). `MAX_VIEWPORT`/crop caps apply per strip, not per document.
- Server pen-down hit for points outside a client's live window: `GET /frames/{id}/hit?x&y` (doc coords) →
  same payload as the bridge's `pw:hit`.

## 5. Input

- Pen: always ink. Pen-down inside the live window → bridge `pw:hit?` (viewport px = doc − scroll). Outside it,
  on the still → server `hit`. Binding stays pending until the answer, as in v1.
- One finger on the live window: tablet forwards to the WebView (native scroll, clicks, inner scroll boxes).
  The window follows the page's scroll in the same UI frame by reading the WebView's own scroll
  (`View.getScrollY()` / `onScrollChanged`, device px ÷ density ÷ page zoom), not the bridge message. The camera
  pans by the same delta, so the page appears fixed to the board and the window slides.
- Two fingers: canvas pan/zoom. After a pan settles, the client asks the live view to scroll so the window
  covers the visible part of the frame: tablet `WebView.scrollTo` (View API, synchronous), web `pw:scrollTo`.
- Web: wheel over the live window scrolls the page (the window follows its reported scroll); wheel elsewhere
  pans the board.

## 6. Defaults

- `show {frame}` / `frame_open`: preset `desktop`, extent `full`.
- Variants: full length, `+ css_w + 40` to the right as today; their stills come from the server's patched
  `FrameView`.
- A frame whose document grows past 30 000 CSS px within 5 s of load (infinite feed) is switched to `viewport`
  by the server with a note in the still.
