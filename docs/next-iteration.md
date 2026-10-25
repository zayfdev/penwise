# Next iteration — notes from the user

Recorded 2026-10-25 while testing on the tablet. **Not started**; listed here so the next pass can pick them up.

## 1. Page shape differs between tablet and PC — **resolved: landscape**

Done: pages are created 2560 × 1600 (landscape, the tablet's own screen) and every existing fixed page was
migrated on load. The PC sheet is now wide, matching the tablet. Also fixed in the same pass: the tablet never
drew paper at all — ruling, grid and dots existed only in the server's PNG. `PaperPainter` now draws them under
the ink with the same colours, spacing and widths as `render.rs`.

What is still open from this item: no view→page transform. The surface is the screen, so landscape is ~1:1, but
the toolbar overlays the top of the page instead of the page starting below it.

## 1a. (original diagnosis, kept for reference)

**Symptom (user):** the page on the PC is "a long thin vertical", while on the tablet it is "a wide infinite
canvas". Nothing had been drawn yet, so this is layout, not ink.

**What the code currently does:**

- Pages are created as `fixed 1600 × 2560` (A4 **portrait**) by default — the PC viewer therefore draws a tall
  narrow sheet (`pages` → `kind` in `meta.json`). `Clean test` and `Page 1` are both this kind; only `Sketch` is
  `infinite`.
- The tablet's canvas is an `InkSurfaceView` that fills the window, so it looks wide and unbounded, but it is
  drawing into the *same* 1600 × 2560 page space. Nothing tells the user the page edge is out of view.

**Likely causes to check, in order:**

1. The server's *active* page and the tablet's *active* page were different at the time (the tablet was
   disconnected, so the PC followed `Clean test`). First question: do they match once both are connected?
2. Orientation: the tablet is used in landscape (2560 wide × 1600 tall) but the page model is portrait. Either
   the page should be created in the display's orientation, or the canvas should letterbox the page so the sheet
   edges are visible.
3. The canvas has no page-boundary affordance at all: a fixed page and an infinite one look identical on the
   tablet. That is probably the real complaint — "wide infinite canvas" is what the surface looks like, not what
   the page is.

**Decide:** page orientation should follow the device at creation time (and be switchable), plus a visible page
edge or a background that makes the sheet obvious on the tablet.

## 1b. Latency (deferred by the user, but diagnosed)

Measured: the **server relay is 0–1 ms** (p50 0 ms, p95 1 ms over 40 frames), so the delay is not the server. It
was **queueing**: every motion event became its own WebSocket frame, so a 120 Hz pen queued frames faster than
the link drained them and the mirror replayed the backlog (same failure as the shape previews, which visibly
"kept growing after the pen lifted").

Landed in the hardening pass: live samples are batched into one frame per 16 ms tick (`LiveBatcher`), shape
previews keep only the newest frame, and the viewer coalesces geometry rebuilds. Remaining levers, in order of
expected gain:

1. Viewer `LIVE_REBUILD_MS = 50` — rebuilding the preview at ~20 Hz caps how fresh the mirror can look; drop to
   ~16 ms and measure the wasm rebuild cost per frame.
2. Rebuild cost grows with stroke length (the engine re-runs the whole stroke). Incremental geometry, or
   rebuilding only the tail, would let the mirror run at 60 Hz on long strokes.
3. JSON per frame is heavy for 5 floats × samples; a binary frame (postcard) would cut bytes and allocation.
4. Browser draw path: `drawDabs` iterates every dab per frame; cache to an offscreen canvas between commits.

## 1c. Long strokes stalled the tablet (fixed)

Holding a stroke makes the live preview rebuild the **whole** stroke natively per frame (`Native.strokeSnapshot`),
on the UI thread inside the touch handler. Cost grows with stroke length, so a long stroke eventually spends more
time rebuilding than the pen takes to produce events: frames drop, the front buffer clears, and the stroke
flashes white and stutters.

Landed: the preview is drawn from a **thinned** copy of the stroke (a point every 3.5 px, `PreviewThinner`) and is
rebuilt at most every 12 ms, both independent of the committed stroke, which keeps every sample. A rebuild slower
than 8 ms logs `slow preview: Nms for M samples` (`adb logcat -s PenwiseInk`), so the next long-stroke test gives
hard numbers instead of guesses.

If it still stutters, the next step is true incremental ink: bake the head of the live stroke into the page layer
and preview only the tail, instead of rebuilding the whole preview each frame.

## 2. Toolbar should collapse

The app's toolbar is permanently pinned across the top: 13 brush previews, colours, size, shapes, eraser,
undo/redo/clear, sync and home. The user wants a dropdown / expand-minimise control so the canvas is not
permanently reduced — collapsed to a small handle, expanded on demand (and probably auto-collapse while writing).

## 3. Liked as-is

The PC viewer's look is approved — sidebar, page cards, paper backgrounds. Don't redesign it.

## Kept for reference

- Page 1 still holds junk on the **tablet** only (server side is empty): tap the toolbar's trash while that page
  is open, or reconnect USB so `scripts/device-check.sh`'s sibling wipe can clear the local file.
- Still unverified with a real pen stroke: the sample-accumulation fix (`StrokeAccumulator`). Replayed ink carries
  hundreds of samples correctly; a pen stroke has not been through it yet.
