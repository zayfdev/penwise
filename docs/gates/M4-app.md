> **Historical gate record; the code has since been rewritten — see docs/rewrite-design.md and docs/pages.md for current behavior.**

# Gate record: M4 Android app

Branch merged to `master`. Verdict: **PASS for the app slices done so far**; tools that need the item model
(selection, fill, transforms) are not built.

## Shipped

| Piece | Where | Notes |
|---|---|---|
| Pen input | `ink/StylusStrokeController.kt`, `ink/SampleBuffer.kt` | Stylus-only (palm rejection), historical samples, `requestUnbufferedDispatch`, pen-up commit |
| Rendering | `ink/InkSurfaceView.kt`, `GeometryPainter.kt` | `CanvasFrontBufferedRenderer` front layer for the live stroke, double-buffered page behind it |
| Brushes | `dev.penwise.core.*` via JNI (`crates/penwise-ffi`) | All 13 presets from the shared engine, colour swatches, size slider |
| Persistence | `ink/StrokeFile.kt` | `PWST` page file: preset, colour, size, seed, raw samples, timings; rebuilt on launch |
| Eraser | `ink/EraseHit.kt`, toolbar toggle | Whole-stroke erase, radius from the size slider |
| Shapes | `ink/ShapeSamples.kt`, toolbar chips | Line, rectangle, ellipse, arrow; drag to size, live preview, committed as normal ink |
| Sync | `ink/SyncClient.kt`, `ink/Wire.kt` | Pushes strokes, page rewrites and live previews to the PC server; queues offline; LAN then Tailscale |
| Live preview | `ink/LiveOut.kt`, server `ws.rs`, `web/app.js` | The in-progress stroke is relayed (never stored) so the browser draws it before the pen lifts |

## Verified

- `just android-check` (Android Lint, ktlint, detekt, JVM unit tests) green.
- Host unit tests: page-file codec (round trip, torn tail, bad tag) and shape geometry (bounds, closure, ellipse
  points on the ellipse, arrow barbs, timing starts at 0 and increases).
- App installs and launches on the SM-X710 with no crash in logcat; `PenwiseSync` logs the connect attempt, the
  success, and rotates LAN → Tailscale on failure.
- Shape output rendered through the server (`/page.png`) matches expectations: rectangle, ellipse, arrow, and a
  highlighter rectangle in the multiply band.

## Not verified (needs the pen)

Latency and feel, pressure response per brush, palm rejection in practice, eraser accuracy under the hand, and
whether a reloaded page matches the live one. These are the Gate D items.

## Gaps

- No selection, fill bucket or shape re-editing: those need the M2 item model over JNI (`Scene`, `Edit`).
- Undo covers add, erase and clear (one step per gesture) but the history is in-memory only: it resets on restart.
- The sync URL list is hardcoded in `res/values/strings.xml`; there is no pairing UI or token yet.
