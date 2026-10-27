# Known gaps

State after the op-log rewrite (`docs/rewrite-design.md`; gates `just check`, `just android-check`, `just web-e2e`
green). The earlier notes in this file (page orientation, latency, the 2026-10-25 lost-ink recovery, pictures
work) described the pre-rewrite code and are superseded; landscape default pages, adaptive previews and the
picture/anchor surface are all in the current code.

## Tablet

- **State file is the whole replica per commit.** `StatePersister` writes the complete `SyncCore` blob (clock,
  outbox, dead letters, every item) atomically after each change (`sync/StatePersister.kt`,
  `penwise-ffi/src/sync_core/state.rs` — its header notes a row store was the alternative). Cost grows with the
  library, and writes coalesce, so a kill can lose roughly the last ~100 ms of strokes. The expensive half runs
  outside the core lock: `SyncCore::snapshot_state` copies the rows under the lock (3000 strokes × 120 samples:
  ~3 ms release, ~4 ms debug) and `StateSnapshot::encode` serialises them without it (~15 ms release, ~200 ms
  debug); `syncRevision` tells the persister when something changed.
- **Shape resize has no UI.** `PageScene.resizeShape` / `Native.sceneResizeShape` exist, but nothing in the
  toolbar or gestures calls them; shapes can be drawn and, through the lasso, moved or deleted.
- **Lasso move shows only a frame.** While dragging, the front buffer draws the selection frame shifted
  (`ink/SelectionGesture.kt`); the items themselves jump on pen-up.
- **Fill bucket has no UI** on the tablet or in the browser: the algorithm is in `penwise-core` (`fill/`) but
  `penwise-ffi` and `penwise-wasm` do not expose it.
- **No scan button / auto-scan.** Recognition runs only when something reads `GET /text` (viewer ☰ panel, MCP
  `penwise_page_text`).
- **No mDNS discovery.** The server does not advertise itself; the tablet is pointed at a URL or pairs by code/QR.

## Browser (`docs/draw.md`)

- **Outbox and undo are not persisted across reload.** Strokes drawn while disconnected survive reconnects but not
  a page reload; undo history is per tab (undo non-persistence is an owner decision, the outbox is not).
- **Highlighter preview is opaque until commit**: the live canvas does not multiply against the page.
- Pen and mouse draw ink only; no eraser, shapes or lasso in the browser.

## Sync

- **Eraser and shape live previews are not sent to other devices.** The live channel carries only freehand
  `StrokeDelta` / `StrokeCancel` (`penwise-proto/src/live.rs`); other devices see an erase or shape when it commits.

## Security

- **S7 hardening is pending.** One shared token (`$PENWISE_DATA/token`, accepted as `?token=` or
  `X-Penwise-Token`) authenticates every device, and the transport is cleartext HTTP/WS on the LAN. Planned in
  `docs/rewrite-design.md` §6 S7: per-device tokens with revocation, TLS with a pinned fingerprint, token never
  logged, data dir `0700`.

## Extension

- Only the visible viewport is captured (no full-page stitching).
- Stale selectors are not detected; anchor attribution is recomputed per read instead (`docs/pages.md`).
