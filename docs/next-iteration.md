# Known gaps

State after the op-log rewrite (`docs/rewrite-design.md`; gates `just check`, `just android-check`, `just web-e2e`
green). The earlier notes in this file (page orientation, latency, the 2026-10-25 lost-ink recovery, pictures
work) described the pre-rewrite code and are superseded; landscape default pages, adaptive previews and the
picture surface are all in the current code.

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
  `notes {page}`).
- **No mDNS discovery.** The server does not advertise itself; the tablet is pointed at a URL or pairs by code/QR.

## Browser (`docs/draw.md`)

- **Outbox and undo are not persisted across reload.** Strokes drawn while disconnected survive reconnects but not
  a page reload; undo history is per tab (undo non-persistence is an owner decision, the outbox is not).
- **Highlighter preview is opaque until commit**: the live canvas does not multiply against the page.
- Pen and mouse draw ink only; no eraser, shapes or lasso in the browser.

## Sync

- **Eraser and shape live previews are not sent to other devices.** The live channel carries only freehand
  `StrokeDelta` / `StrokeCancel` (`penwise-proto/src/live.rs`); other devices see an erase or shape when it commits.

## Design harness v1 (`docs/design-harness.md`)

- **No Astro adapter.** Astro is detected by `penwise link`, but its marks carry no component source; source
  entries fall back to URLs.
- **Next CSS stays URL-only.** The Next adapter maps elements to component files; CSS rules carry the URL, not
  a file line.
- **Proxied HTML has no file line without an adapter.** The base HTML mapping parses the served document, and a
  dev origin serves no document. Vite CSS still maps `/src/x.css` and `/@fs/…` to files.
- **Vue and Svelte templates map only plain HTML elements.** A component tag (`<Card>`) gets no `data-pw-src`;
  resolution infers the nearest ancestor with the attribute.
- **No `penwise dev`.** The owner starts the dev server. `penwise status` reports whether it answers.
- **Dev server down shows no last snapshot.** The proxy answers 502 with `dev server <origin> unreachable`;
  the frame keeps its marks but has no page.
- **State replay failure is silent on screen.** The mark stays greyed; there is no "state not reproducible"
  label. A failed replay only logs on the tablet and returns `ok:false` to the bridge.
- **No orphaned/ambiguous flags in bundles.** A deleted element is `element: null`; the agent must say so.
- **Patches cannot run JavaScript.** Ghosts and variants are CSS plus DOM ops, not branches or worktrees.
- **Live public sites are out of scope.** The proxy dials a local dev origin only.

## Design harness v2 (`docs/design-harness-v2-spec.md`)

- **The scroll link is unverified on the device.** Full-length frames follow the page's own scroll (spec §3.2,
  contract §5), but how it feels on the Tab S9 and the PC window (marks swimming, the bitmap fallback during a
  pan) has only been checked in tests, not on hardware. Do this in the one device session the spec plans.
- **Fixed and sticky elements appear once in a snapshot**, at their position at scroll 0; the live window shows
  the truth. Reveal-on-scroll content is hidden in a snapshot until scrolled, and lazy images are loaded by a
  pre-roll, so a still can differ from the live page.
- **Infinite feeds fall back to `extent: viewport`.** A document that grows past 30 000 CSS px within a few
  seconds of load is switched to a viewport-sized frame by the server (with a note in the still), so such a
  page cannot be unrolled.
- **Deferring the agent's camera move cannot tell the tablet from the agent on the web viewer.** The camera
  holds an agent move back for 10 seconds after the owner drew (spec §5), but on the web viewer it has no way to
  tell whose input it saw, so the deferral can misfire.
- **"Use this" chips overlap the toolbar at fit view.** In focus view the variant chips sit under the top toolbar.
- **omp has the skill, not the hook.** `penwise link` writes `.omp/skills/penwise/SKILL.md`, but omp has no
  documented prompt hook, so a sent round reaches an omp session only when the owner says "look". The skill
  frontmatter (`name`, `description`) is the common layout; omp's own loader was not confirmed.
- **`penwise inbox` runs on a prompt, not on Send.** The Claude Code hook fires when the owner submits a prompt;
  it does not wake an idle session.
- **A frame taller than 40 Mpx is snapshotted scaled down.** History pictures keep the frame's full box size,
  but their pixels are fewer; a very long page is also stored at no more than 9000 px a side.
- **Rounds read only the project's own notebook.** Marks on a frame placed on a page outside the project's
  notebook are not counted in rounds, `look` or the inbox.

## Security

- **S7 hardening is pending.** One shared token (`$PENWISE_DATA/token`, accepted as `?token=` or
  `X-Penwise-Token`) authenticates every device, and the transport is cleartext HTTP/WS on the LAN. Planned in
  `docs/rewrite-design.md` §6 S7: per-device tokens with revocation, TLS with a pinned fingerprint, token never
  logged, data dir `0700`.
