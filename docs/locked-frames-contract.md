# Locked frames, the overview strip and variant generations — contract

The fixed interface for the tablet's locked-frame UX (owner decisions of 2026-11-01). Builders code against it and
never change it; a needed change goes back to the orchestrator. File:line references are to `1f10e7a`.

The data model does not change: one board page per project, lanes, variants as grouped Frame items, history
pictures as Image items, rounds in the server table. The agent's four MCP tools keep their shape. What changes is
what the tablet shows, and what the server keeps when a round closes. The web viewer keeps the board view (out of
scope here).

## 1. The owner's model

- **A frame is the screen.** On a board page the camera is always locked to one frame's box. It never shows board
  space outside that box.
- **Zoom inside the frame.** Two fingers zoom in (up to `MAX_SCALE`) and pan, but never below the frame's fit-width
  scale and never past its box edges. One finger anywhere on the locked frame scrolls the page (on the live window, or on the still part below it, as a browser does); the pen draws.
- **The overview is the way out.** Pinching out while already at fit scale, past a dead zone, opens the overview;
  so does an overview button. The overview is a sideways strip of columns on a dark canvas, scrolled like a niri
  workspace (one column centred, its neighbours peeking): older generations to the left, the current one in the
  middle, then the project's other pages (sketch pages, other screens) to the right, then a `+` column. Tapping a
  card enters it.
- **Variants switch in place.** A floating switcher shows `OG · A · B · C` (the lane's live frame, then its variants
  by label). Tapping one shows that frame locked full screen; an edge swipe goes to the next or previous one.
  "Use this" picks the shown variant.
- **Generations.** Closing a round keeps the whole generation: the OG picture and one picture per variant, each with
  its ink, and which one was picked. The picked variant, once the agent has put it in code, is the new OG of the
  next generation. In the overview each generation is one column: its OG card on top and its variant cards stacked
  under it (collapsed to a deck, tap to expand). Entering a past card shows it full screen, read-only, with the same
  switcher.

## 2. Server: generations (Rust)

### 2.1 Freezing a generation
- `freeze_round` (`agent/archive/mod.rs`) also freezes **every variant frame** of each frozen lane's variant groups
  (`groups_beside`, `archive/places.rs:49-61`), before `clear_plan` deletes them. Each variant picture is made like
  the OG picture: snapshot with the variant's patch applied (`frame_snapshot?ink=false`) plus that round's strokes
  on that variant (`archive/ink.rs`, same "after Send stays off" rule).
- Placement keeps lanes readable: the generation's pictures sit left of the live frame in one row, OG first, then
  the variants in label order, each `css_w + COLUMN_GAP` apart. The column shift (`archive/lane.rs`) moves the live
  frame and everything right of it by `(1 + variant_count) × (css_w + COLUMN_GAP)` instead of one column.
- A variant still protected by `newer` (a mark drawn after Send) is not frozen and not deleted (today's rule).
- Variant pictures carry the same scale/doc-height handling as the OG picture (`archive/store.rs`).

### 2.2 Generation records
- New server table (schema step 6, numbered migration): `generations(project, round, route, preset, role, label,
  picked, item, asset, w, h)` with `role` = `og` | `variant`; one row per picture; written in the same per-project
  lock as the resolve. `picked` is the variant whose `chosen` was set at resolve time (none → no row picked).
- `GET /projects/{p}/history?route=<r>&preset=<p>` →
  ```json
  {"route":"/","preset":"desktop","generations":[
    {"round":3,"resolved_ms":1793550000000,
     "og":{"item":"<32hex>","asset":"<id>","w":1440,"h":7574},
     "variants":[{"label":"A","item":"<32hex>","asset":"<id>","w":1440,"h":7574,"picked":true}]}]}
  ```
  newest first; `variants` may be empty; `item` is the Image item on the board; the image bytes come from the existing
  asset route. Same auth rules as the other `/projects` routes.
- `look`'s text gains one line per lane with a past generation: `history: round 3 picked "A" (2 variants)`.
- Tests: freezing with 0, 1 and 3 variants; picked flag; a protected variant survives; lane shift width; history
  route ordering; schema migration from v5 fixtures.

### 2.3 Strip pages
- Server sidecar table (same schema step 6): `strip_pages(project, route, preset, page)`. `PUT
  /projects/{p}/strip-pages/{page}` with `{"route","preset"}` attaches a page to a strip (idempotent);
  `GET /projects/{p}/strip-pages` → `[{"page","route","preset"}]` in attach order. A page deleted from the library
  drops out of the GET. Same auth as the other `/projects` routes.

## 3. Tablet (Kotlin)

### 3.1 Lock
- New `FrameLock` state (plain class): `locked: String?` (frame id). On a board page with frames, the lock is always
  set; `FrameOpening.target` picks the initial one. `FocusRule` no longer decides who is live on the tablet: the
  locked frame is live, and the previous locked frame stays warm.
- Camera clamp: while locked, `ViewportState` gets a lock rect (the frame's box) and a minimum scale = the box's
  fit-width scale; `transformBy`, `fitRect` and pans clamp so the screen stays inside the box horizontally and
  between the box's top and bottom vertically. Agent `Focus(frame)` changes the lock (switching page first);
  `Focus(None)` opens the overview. Agent moves still defer while the owner drew in the last 10 s.
- Scroll follow: while a frame is locked, the camera follows **every** scroll of the locked live page (a finger, a fling,
  an anchor link, `scrollIntoView`, a smooth-scroll library, a restored position), so the window stays exactly on
  screen at the camera's in-frame zoom. Owner decision: "a frame is the screen, like a browser". The exception is the
  pen: while it is down or hovering the camera stays, and when it lifts it catches up with one short (120 ms) flight.
  The owner-only rule (`ScrollOwner`) applies only while the camera is not locked.
- Dead zone: during a two-finger gesture that starts at min scale, accumulate the pinch factor below 1; at
  `≤ GRID_PINCH = 0.82` the overview opens on release (or at once when it passes `0.70`). A pinch that does not reach it
  springs back to fit.
- Edge swipe: a one-finger horizontal swipe starting within 24 dp of the left or right screen edge (the app already
  excludes system gestures there) moves to the previous/next frame in the switcher order. Elsewhere one finger goes
  to the page as today.
- Still part: a one-finger drag that starts on the locked frame's box outside the live window scrolls the live page by the
  drag (converted with the camera scale) and flings with the WebView's own physics; a tap there stays a tap. It is
  ignored within 500 ms of pen input (a resting palm), and held while a touch from an edge strip leans sideways (it
  may be an edge swipe).

### 3.2 Switcher
- Shown when the locked frame's lane has a variant group. Items: `OG` (the lane's live frame), then variants by
  label. The shown item is highlighted; the chosen variant has a check. Tapping an item locks to it (a fast slide,
  ≤ 180 ms). A "Use this" button picks the shown variant (`setFrameChosen`); on `OG` it is hidden.
- Unchosen variants are no longer dimmed while shown full screen (the comparison is now in time, not side by side).

### 3.3 Overview: one strip per screen (owner decisions 2026-11-01)
- A full-screen overlay window (not a camera move), dark dotted canvas, cards with a small title above each (like a
  design canvas). It holds one **strip** per screen (route + preset): Desktop `/`, Phone `/`, …, stacked vertically
  like niri workspaces. Swipe up/down between strips, sideways within a strip; snap one column at a time; the focused
  column is centred and its neighbours peek in. It opens on the locked frame's strip, its "now" column centred.
- A strip is that screen's timeline, left to right: one column per past generation from
  `GET /projects/{p}/history` (oldest first), the "now" column (the lane's live frame as OG, its live variants under
  it), then the sketch pages attached to this strip (§2.3) in creation order, then `+`.
- A column's variant cards are collapsed into a deck under the OG ("3 variations · picked B"); a tap expands it in
  place (springy, ≤ 220 ms); the picked one has a check. Card images: a frame's still (`?w=360` strip), a history
  picture's asset, a sketch page's preview.
- Entering: a live frame or variant card locks to that frame on the board page (zoom-in animation from the card); a
  history card opens it full screen read-only (pen and fingers do not edit; two fingers zoom) with the switcher
  showing that generation and a "Back to now" control; a sketch card opens that page as a normal free canvas (the
  overview button returns to the overview).
- `+` at the end of a strip: "New sketch page" (an Infinite page in the board page's notebook, titled "Sketch n",
  attached to this strip). A separate "New screen" control at the bottom of the overview adds a frame (the existing
  add-frame flow: route + preset), which becomes a new strip.
- Opening animation: the locked frame shrinks into its card (≤ 260 ms, eased); reduced motion → crossfade.
- Refetch history and strip pages when the overview opens and when a round resolves.
- **Deferred (not in this slice):** forking a card into a new timeline. Owner decision: forks will be real git
  branches, each in its own worktree served on its own port, with the agent editing that worktree. Nothing here may
  assume one lane per route + preset forever, but no fork code is written now.

### 3.4 Chrome styling
- Graphite (owner pick, direction B): opaque `#16191F` panels, `#2A2F38` hairline, radius 18 (buttons 12), no blur.
  Fonts are not chosen yet: use the system sans for now, from one place so a later pick swaps it everywhere.

### 3.5 Chrome above the ink
- The ink surface is above the window while frames exist, so in-window controls sit under the ink. The switcher, the
  overview button, Send and the existing toolbar are hosted in their own small windows
  (Compose `Popup`, not focusable, sized to their content, touches outside pass through), which Android stacks above
  the surface. The overview is a full-screen window. Mark chips may stay in the window for now.

## 4. Seams and rules
- No core or proto change. Generation data reaches the tablet over HTTP only.
- Tablet files stay under the 500-line hard limit; `FrameLayer.kt` is at 340 lines: new behaviour goes in new
  feature files (`FrameLock`, `OverviewStrip`, `VariantSwitcher`, `ChromeWindow`, …), not into it.
- Every rule above that is pure (clamp maths, dead zone, switcher order, edge-swipe classification, overview column list)
  gets a JVM unit test. Server behaviour gets Rust tests. Then `scripts/device-qa` (when present) on the tablet.
