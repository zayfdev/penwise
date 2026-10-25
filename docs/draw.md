# Drawing in a browser (iPad, Surface, Chromebook)

`/draw.html?token=…` is a pen client that needs no install: it draws with the same wasm engine as the mirror and
sends the same `stroke` frames as the tablet, so the tablet, the mirror and agents see its ink identically.

## What it does

- **Pointer Events**: pressure from `PointerEvent.pressure`, tilt and direction from `tiltX`/`tiltY`; a mouse or
  finger draws at constant pressure. `getCoalescedEvents()` keeps fast strokes smooth.
- **Live preview** through the engine, rebuilt at most once per display frame; the committed stroke goes out on
  `pointerup` (or after 4 s idle, so a lost pen does not strand it).
- **Page following**: loads `/pages`, selects the active page, then follows `active`/`page`/`image`/`op` frames.
- **Tools**: three presets, colour, size; undo removes the newest stroke via an `op remove` frame (two-finger tap
  also works).
- Pictures placed on the page (screenshots, cut-outs) draw underneath the ink.

## Verified

- Synthetic stylus events (Pointer Events, pressure 0.3→0.8, 25 samples) on `/draw.html`: the stroke reached the
  server (`GET /page` count 11 → 12, seed `1792953340103000`) and was removed again through an `op remove`.
- Found and fixed while testing: a colour literal above `i32::MAX` made the server reject the stroke as
  malformed (`argb` is an `i32` on the wire); the default colour is now signed, and server `{"t":"error"}` frames
  are surfaced in the status line instead of being ignored.

## Limits

- No pressure curve editing and no eraser yet; precise erase stays on the tablet.
- No offline queue: a stroke drawn while disconnected is lost (the tablet keeps its own store).
- Fixed pages are fitted to the window; infinite pages are shown at their natural 2560×1600 extent.
