# Next iteration — notes from the user

Recorded 2026-10-25 while testing on the tablet. **Not started**; listed here so the next pass can pick them up.

## 1. Page shape differs between tablet and PC

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
