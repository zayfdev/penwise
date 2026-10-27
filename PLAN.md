# Penwise — Stage 1 Plan

Pen-first notes and canvas on a Galaxy Tab S9 + Staedtler Noris digital (EMR), live-synced to a PC
browser view and readable/writable by agents over MCP.

## Decisions

| Area | Decision |
|---|---|
| Device | Galaxy Tab S9 `SM-X710`, Android 16 (API 36), One UI 8.5, 1600×2560 @ 340 dpi, 120 Hz. Pen digitizer `sec_e-pen`: 14752×23603, pressure 0–4095, tilt X/Y ±63, hover distance 0–255 |
| Tablet client | Native Android, Kotlin |
| Ink engine | Custom Rust core (`penwise-core`): one engine computes stroke geometry, bit-identical on tablet, browser and server. Each surface rasterizes that geometry natively (Android GPU, browser Canvas, server `tiny-skia`) |
| Server | Rust single binary: axum + tokio, SQLite, `rmcp` (MCP over streamable HTTP), `tiny-skia` raster |
| PC view | TypeScript UI + `penwise-core` compiled to WebAssembly |
| Layout | Notebooks → pages; each page is fixed-size or infinite |
| Layers | `ink` (user), `agent` (show/hide, erase, or keep → promote to ink), `text` (recognition output) |
| Network | LAN (mDNS discovery) and Tailscale together; tablet picks the reachable endpoint; QR pairing token |
| Offline | Tablet is local-first (own SQLite), syncs the change log on reconnect |
| Recognition | Agents via MCP (PNG + text). Plus a PC recognition service with a pluggable local model: on-demand "scan" from the tablet, optional auto-scan after a writing pause |
| License | Kept open-source-compatible (MIT/Apache deps only; no tldraw) |

## Architecture

```
Tablet (Kotlin)                          PC: penwise-server (Rust)
 pen MotionEvents ─► penwise-core (JNI)   ws sync ─► SQLite (notebooks, pages, change log)
 front-buffered renderer                  penwise-core native ─► PNG
 local SQLite + outbox  ◄── LAN/Tailscale ──► MCP (HTTP) ◄─► agents
                                          recognizer worker ─► text layer
                                          static web viewer ─► browser (core via WebAssembly)
```

### Code layout

Module names are the planned split; each file stays one concept and under the size limits in `AGENTS.md`.

```
penwise/
  crates/
    penwise-core/      pure: no I/O
      sample.rs          raw pen sample, units, normalization from digitizer range
      builder.rs         incremental StrokeBuilder: push samples, emit stable + tentative geometry
      smoothing.rs       input filtering / resampling
      dynamics.rs        pressure curve, velocity, tilt → size/opacity/flow
      brush/             params, presets, family dispatch
      outline/           outline brushes: centerline + widths → closed polygon (pen, fountain, calligraphy, marker, highlighter)
      stamp/             stamp brushes: deterministic dab placement, scatter, seeded PRNG (pencil, crayon, airbrush, spray, soft, solid, shading)
      shape/             rectangle, ellipse, line, arrow, polygon geometry + handle math
      fill/              flood region at fixed resolution, contour trace, simplify → polygon
      edit/              hit testing, lasso, precise-erase split, transforms
      bounds.rs          bounding boxes, spatial index
      codec/             quantize, delta, varint, versioned blobs
      raster/            geometry + stamps → pixels (server/native only, feature-gated)
    penwise-proto/     ids (ULID), hlc, op, message, version
    penwise-server/
      main.rs, config.rs
      storage/           migrations, op log, notebooks/pages queries
      sync/              websocket sessions, broadcast, live StrokeDelta fan-out
      pairing/           tokens, QR payload, mDNS advertisement
      mcp/               one file per tool group (navigation, render, annotate, text)
      render.rs          page → PNG via penwise-core
      recognize/         Recognizer trait, worker, ollama backend
      web.rs             serves embedded viewer assets
    penwise-ffi/       hand-written JNI (`jni` crate, MIT/Apache) → Kotlin; batch calls, types only
    penwise-wasm/      wasm-bindgen bindings → TypeScript (types only)
  android/app/src/main/kotlin/dev/penwise/
    ink/  tools/  sync/  pairing/  notebooks/  debug/ (recorder + replay, debug builds only)
  web/src/             main.ts, connection.ts, viewer.ts, ui/
  fixtures/            real pen recordings + golden outputs
  assets/brushes/      brush textures and dab shapes, shared by app, web and server
  scripts/             check-sizes, record-pen, device-check
  docs/gates/          one gate record per phase
```

## Data model

- **Notebook** `{id, title, created, updated}`
- **Page** `{id, notebook_id, index, kind: fixed{w,h} | infinite, background: blank|lined|grid|dot}`
- **Item** `{id (ULID), page_id, layer: ink|agent|text, kind, data, z, author: device|agent:<name>, deleted}`
  - `stroke`: brush params + seed + raw samples `(x, y, pressure, tilt_x, tilt_y, t)`, quantized + delta-encoded
  - `erase`: precise-eraser path for stamp brushes and fills (masks what it covers); outline strokes are split instead
  - `shape`: kind, transform, stroke style (brush params), optional fill color
  - `fill`: polygon(s) with holes, color, opacity
  - `text {content, box, font_size}`, `image` (later)
- Raw samples + brush params + seed are the source of truth; geometry is derived, so brush improvements re-render old notes.

## Brushes & tools

Two brush families in `penwise-core`, both deterministic on every surface:

| Family | How it renders | Presets |
|---|---|---|
| Outline | centerline + widths → one filled polygon | ballpoint pen, fineliner, fountain pen (pressure + direction width), calligraphy (fixed nib angle), marker (broad, flat opacity), highlighter (translucent, sits under ink) |
| Stamp | dabs placed along the path, each with size/angle/opacity, textured | pencil (grain; tilt → wide soft side shading), crayon (coarse paper tooth), airbrush (soft dab, pressure → flow, builds up), spray (scattered particles), soft brush, solid brush, shading brush (wide, very soft, low opacity) |

- **Brush params:** size min/max, opacity, flow, color, hardness, spacing, scatter, texture, pressure curve, velocity
  and tilt response, blend mode. Presets are editable and saved; custom brushes are params, not code.
- **Determinism:** stamp positions/jitter come from a seeded PRNG inside core (seed stored per stroke); textures are
  shared files in `assets/brushes/`.
- **Eraser** (the Noris classic has no button or eraser end, so erasing is a tool with a quick toggle):
  stroke eraser (removes whole items), precise eraser (splits outline strokes; masks stamp strokes/fills), size control.
- **Fill bucket:** tap a closed area → region found at a fixed resolution, traced to a vector polygon, stored as a
  `fill` item; gap-tolerance setting closes small gaps.
- **Shapes:** rectangle, rounded rectangle, ellipse, line, arrow, polygon. Drag to create; handles to stretch, rotate,
  move; constrain toggle (square/circle); stroke style from any outline brush; optional fill.
- **Select:** lasso or tap; move, scale, rotate, delete, duplicate, recolor any item.
- **Color & size:** color wheel, swatches, recent colors; per-brush size and opacity sliders.
- **Undo/redo** per page, built on the op log.
- **No-button pen ergonomics:** toolbar quick-swap between the last two tools, plus finger gestures (e.g. two-finger
  tap undo, three-finger tap redo); final mapping decided at the M4 feel check.

## Sync protocol

- Strokes are immutable; all edits are ops in an append-only log:
  `AddItem`, `DeleteItem`, `UpdateItem` (transform/style/promote layer), `PageOp`, `NotebookOp`.
- Each op: `{op_id, device_id, hlc (hybrid logical clock), page_id, payload}`; last-writer-wins per item field.
- Client keeps `last_seen_seq`; on connect it pushes its outbox and pulls ops since that seq.
- Live drawing: in-progress stroke samples are streamed as ephemeral `StrokeDelta` messages
  (browser shows ink as you write); committed as `AddItem` on pen-up.
- Transport: WebSocket, binary frames (postcard), versioned envelope.

## Networking & pairing

- Server binds LAN + Tailscale interfaces; advertises `_penwise._tcp` via mDNS.
- Browser view shows a QR code `{lan_addr, tailscale_name, port, pairing_token, server_pubkey_fp}`.
- Tablet stores the endpoint list; tries mDNS/LAN first, falls back to the Tailscale MagicDNS name.
- Every connection presents a per-device token; MCP endpoint is bound to localhost/Tailscale with its own token.

## MCP tools (stage 1)

| Tool | Purpose |
|---|---|
| `list_notebooks` / `list_pages(notebook)` | navigation |
| `create_notebook(title)` / `create_page(notebook, kind, background)` | organization |
| `switch_page(page)` | moves the tablet + browser to a page |
| `get_page_png(page, region?, scale?, layers?)` | rendered image; infinite pages default to content bounds |
| `get_page_text(page)` | text layer from the recognizer |
| `request_scan(page, region?)` | run the PC recognizer now |
| `annotate(page, items[])` | shapes/text/paths onto the `agent` layer, live on tablet + browser |
| `clear_agent_layer(page)` | remove agent marks |

## Recognition service

- Runs inside the server as a worker; model backend behind a trait (`Recognizer`).
- First backend: local vision model through Ollama (model choice decided during M5 by testing on real handwriting).
- Triggers: tablet "scan" button, MCP `request_scan`, optional auto-scan after N seconds of no ink on a page.
- Output: `text` layer items with bounding boxes linked to source stroke IDs.

## Tablet app essentials

- Input: `MotionEvent` historical samples, `AXIS_PRESSURE`, `AXIS_TILT`, `AXIS_ORIENTATION`,
  hover events, `BUTTON_STYLUS_PRIMARY`; palm rejection = stylus-only drawing, fingers pan/zoom.
- Latency: `androidx.graphics` front-buffered renderer + `androidx.input` motion prediction.
- Tools: everything in "Brushes & tools", plus page/notebook navigation, agent-layer toggle, scan button.

## Verification harness

- **Fixture recording before the app exists:** `adb shell getevent` on `sec_e-pen` captures raw digitizer events
  (x, y, pressure, tilt, hover distance, kernel timestamps) while you write in any app; `scripts/record-pen` converts
  them into `fixtures/*.pwrec`. The app's debug recorder (M3) later records through the real `MotionEvent` path.
- **Replay harness** (debug build only): `adb shell am broadcast` with a fixture path feeds recorded samples through the
  same input pipeline as the pen. `adb shell input stylus` can't set pressure, so it isn't enough on its own.
- **Screen checks:** `adb exec-out screencap -p` compared against server-rendered goldens with a perceptual tolerance
  (GPU antialiasing differs from `tiny-skia`; geometry identity is checked exactly in core tests).
- **End to end:** after replay, assert the stroke exists on the server (HTTP debug endpoint), in the browser view
  (browser automation), and in `get_page_png` (MCP test client).
- **Feel checks** stay human: latency and pressure response are judged by you on the device each milestone.

## Milestones

| # | Milestone | Done when |
|---|---|---|
| M0 | Toolchain ✅ | Android SDK/NDK, adb authorized, cargo-ndk, wasm-pack, `aarch64-linux-android` + `wasm32-unknown-unknown` targets, `just check` gate |
| M1 | Ink engine | Both brush families with all presets; dynamics tuned from your recordings; bit-identical geometry/stamps on native and WebAssembly; server PNG goldens; codec round-trips; brush sheet PNG in the gate record |
| M2 | Editing engine | Stroke + precise eraser, fill bucket, shapes with handles, lasso select + transforms, undo model; all in core with tests |
| M3 | Server + browser view | Op log in SQLite; browser renders every item kind and live `StrokeDelta`s; pairing QR |
| M4 | Android app | Low-latency ink with every brush and tool; toolbar, brush picker, color/size; local store; LAN + Tailscale sync; offline drawing syncs on reconnect |
| M5 | MCP | All stage-1 tools usable from an agent; annotations appear live on tablet + browser |
| M6 | Recognition | Scan button + auto-scan produce text layer; `get_page_text` returns it |

**Status** (as of the op-log rewrite, HEAD 48d30e4; details in `docs/rewrite-design.md`, `docs/next-iteration.md`):

- **M0 — done.** Toolchain and `just check` gate.
- **M1 — done.** Both brush families, deterministic geometry, codec round-trips; gate record `docs/gates/M1.md` (historical).
- **M2 — done in `penwise-core`** (erasers, fill, shapes, lasso, undo). The tablet and browser do not expose the fill bucket; the tablet has no shape-resize UI.
- **M3 — done, rewritten.** Server on schema v4: op log + HLC + per-field LWW, one writer thread, binary `/sync`, snapshots and 7-day retention, restore, pairing code/QR, static viewer; `penwise-wasm` + `web/` viewer. No mDNS advertisement. Single shared token, cleartext (S7 pending).
- **M4 — done, rewritten.** Thin Kotlin shell over `penwise-ffi` (`SceneHandle`, `SyncCore`); persisted outbox, offline drawing syncs on reconnect. Gaps: state file is the whole replica per commit, lasso move shows only a frame, no fill/shape-resize UI.
- **M5 — done with a different shape.** `penwise-mcp` is a stdio server over the HTTP API (not `rmcp`); tools are `penwise_*` (`docs/mcp.md`), not the names in "MCP tools (stage 1)" above. Annotations reach tablet and browser live as ops from the server device.
- **M6 — partial.** Server-side handwriting → text-layer items via a local Ollama model, `GET /text`, `GET /search`, MCP `penwise_page_text` / `penwise_search`. Not built: tablet scan button, auto-scan, a `Recognizer` trait / worker, `request_scan`.
- **Browser drawing (post-M3, not in the original plan)** — pen/mouse ink with undo, text panel, search, pairing panel (`docs/draw.md`).

## Build process

| Part | Build | Output |
|---|---|---|
| Rust crates | Cargo workspace | libraries + `penwise-server` binary |
| Browser viewer | `wasm-pack build penwise-wasm` → Vite + TypeScript (bun as runner) | static assets embedded into the server binary (`rust-embed`) |
| Android app | Gradle wrapper (Kotlin DSL, version catalog); a Gradle task runs `cargo ndk -t arm64-v8a` for `penwise-ffi` before packaging; Kotlin side of the JNI bridge is hand-written in `dev.penwise.core` (decision 2026-10-24: uniffi's MPL-2.0 license rejected to keep the license policy permissive) | debug/release APK, compileSdk 37, targetSdk 36 |
| Release | `just release` | one server binary per OS + one APK |

`just` recipes: `check`, `android`, `install`, `device-check`, `web`, `serve`, `release`. Heavy builds run under `batch`.

## Orchestration

### Roles

| Role | Agent | Owns | Never does |
|---|---|---|---|
| Orchestrator | main session | plan, contracts, todo list, gates, commits, gate records | substantial implementation |
| Researcher | `scout` (read-only) | external API facts before a phase: writes `docs/research/<topic>.md` | edits code |
| Implementer | `task` | one slice with an explicit file list and a fixed contract | lint, format, run gates, touch files outside its list |
| Reviewer | `reviewer` | per-phase diff review against `AGENTS.md` and the phase acceptance | edits code (findings become corrective tasks) |
| Security reviewer | `security-reviewer` | M2 (pairing, tokens, network binding), M4 (MCP exposure), pre-open-source | edits code |
| Mechanical | `sonic` | renames, fixture regeneration, bulk mechanical updates | design decisions |
| You | human | approvals (sudo, licenses, publishing), fixture writing sessions, feel checks | — |

### Phase loop

1. **Research (pipelined):** scouts for phase N+1 run while phase N is implemented.
2. **Contract:** one small serial step fixes the shared interface (types, traits, wire messages) and is committed
   before fan-out. Parallel implementers code against it and never change it; contract changes go back to the orchestrator.
3. **Fan-out:** one implementer per disjoint slice, all dispatched together.
4. **Gate A (automated):** `just check`; from M3 also `just device-check`.
5. **Gate B (review):** reviewer verdict PASS / FAIL with findings → corrective tasks → re-run Gate A.
6. **Gate C (evidence):** the milestone's "done when" is exercised for real (replay a fixture, open the browser view,
   call MCP tools) and the output is recorded.
7. **Gate D (human, only where feel matters):** M4 ink + tools feel, M6 recognition quality.
8. **Close:** gate record in `docs/gates/M<n>.md`, one commit per green phase.

### Phase map

| Phase | Research | Contract (serial) | Parallel slices | Extra gates |
|---|---|---|---|---|
| M1 ink engine | stroke/brush rendering methods (perfect-freehand, stamp brushes in open painting apps), `tiny-skia` | `sample`, brush params, `builder` API, outline + stamp output types, codec signatures | (a) smoothing + dynamics, (b) outline brushes, (c) stamp brushes + PRNG + textures, (d) codec, (e) raster + PNG goldens, (f) fixture converter + golden harness + wasm test runner | — |
| M2 editing engine | vector stroke splitting, flood fill + contour tracing | item kinds, edit API | (a) hit test + lasso + transforms, (b) erasers, (c) fill, (d) shapes + handles, (e) undo model | — |
| M3 server + web | `rmcp`, axum websockets, mDNS crates, `rust-embed` | `penwise-proto` ops/messages + storage schema | (a) storage + migrations, (b) sync + live deltas, (c) pairing + mDNS + auth, (d) `penwise-wasm` + web viewer | security review |
| M4 Android | `androidx.graphics` front-buffer, `MotionEventPredictor`, `jni` crate + Gradle + cargo-ndk, Samsung stylus specifics | `penwise-ffi` JNI API + Gradle Rust build | (a) input + low-latency render, (b) toolbar + brush/color/size UI, (c) eraser/fill/shape/select tools, (d) local store + sync client, (e) pairing/discovery UI, (f) debug recorder/replay + `device-check` | human feel check |
| M5 MCP | `rmcp` tool/image result shapes | tool schemas | (a) navigation tools, (b) PNG + text tools, (c) annotate → agent layer on tablet, (d) agent layer on web, (e) MCP e2e test client | security review |
| M6 recognition | local vision models for handwriting (Ollama) | `Recognizer` trait + text-layer item shape | (a) worker + triggers, (b) Ollama backend + model evaluation on fixtures, (c) tablet scan button + text layer view | human quality check |

M2 starts once M1's output types are committed. M3 and M4 overlap once `penwise-proto` and `penwise-ffi` contracts are committed.

## Toolchain (installed 2026-10-24)

- Rust 1.93.0 pinned, targets `aarch64-linux-android` + `wasm32-unknown-unknown`; cargo-ndk, wasm-pack, just, cargo-deny.
- Android SDK at `~/Android/Sdk` (env in `~/.zshenv`): cmdline-tools, platform-tools r37.0.1, `platforms;android-37.0`,
  build-tools 37.0.0, NDK r30 (30.0.16248370). NDK `cdylib` link for arm64-v8a verified.
- USB access: `/etc/udev/rules.d/51-android.rules` (Samsung `04e8`, uaccess). Tablet serial `R00000000000`.
- Also present: OpenJDK 21, bun, node 22, tailscale (this PC = `deskpc`), ollama (no models yet). Gradle via wrapper at M3.

## Later stages (parking lot)

- ML Kit on-device recognition for fully offline moments
- Paper notes scanning (camera → page image + recognition)
- Search across notebooks, export (PDF/SVG/Markdown), templates, shape recognition
- Agent-driven pages (agent creates a page with a diagram for you to annotate)
- Open-source packaging: release binaries, APK, docs
