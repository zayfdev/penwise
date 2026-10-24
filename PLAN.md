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
      brush.rs           tool, color, base width, pressure curve
      builder.rs         incremental StrokeBuilder: push samples, emit stable + tentative geometry
      smoothing.rs       input filtering / resampling
      width.rs           pressure/velocity/tilt → width
      outline.rs         centerline + widths → closed outline polygon
      bounds.rs          bounding boxes, hit testing (eraser, lasso)
      codec/             quantize, delta, varint, versioned stroke blob
      raster.rs          outline → pixels (server/native only, feature-gated)
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
    penwise-ffi/       uniffi bindings → Kotlin (types only)
    penwise-wasm/      wasm-bindgen bindings → TypeScript (types only)
  android/app/src/main/kotlin/dev/penwise/
    ink/  sync/  pairing/  notebooks/  debug/ (recorder + replay, debug builds only)
  web/src/             main.ts, connection.ts, viewer.ts, ui/
  fixtures/            real pen recordings + golden outputs
  scripts/             check-sizes, record-pen, device-check
  docs/gates/          one gate record per phase
```

## Data model

- **Notebook** `{id, title, created, updated}`
- **Page** `{id, notebook_id, index, kind: fixed{w,h} | infinite, background: blank|lined|grid|dot}`
- **Item** `{id (ULID), page_id, layer: ink|agent|text, kind, data, z, author: device|agent:<name>, deleted}`
  - `stroke`: brush `{tool: pen|highlighter|eraser, color, base_width}` + raw samples
    `(x, y, pressure, tilt, orientation, t)`, quantized + delta-encoded binary
  - `shape` (line, rect, ellipse, arrow, polyline), `text {content, box, font_size}`, `image` (later)
- Raw samples are the source of truth; geometry is derived, so brush improvements re-render old notes.

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
- Tools: pen, highlighter, stroke eraser, lasso select (move/delete), undo/redo, page/notebook navigation,
  agent-layer toggle, scan button.

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
| M1 | `penwise-core` | Recorded sample streams → bit-identical outlines on native and WebAssembly; server PNG goldens; codec round-trips |
| M2 | Server + browser view | Op log in SQLite; browser renders pages and live `StrokeDelta`s; pairing QR |
| M3 | Android app | Low-latency pressure ink on the Tab S9; local store; LAN + Tailscale sync; offline drawing syncs on reconnect |
| M4 | MCP | All stage-1 tools usable from an agent; annotations appear live on tablet + browser |
| M5 | Recognition | Scan button + auto-scan produce text layer; `get_page_text` returns it |

## Build process

| Part | Build | Output |
|---|---|---|
| Rust crates | Cargo workspace | libraries + `penwise-server` binary |
| Browser viewer | `wasm-pack build penwise-wasm` → Vite + TypeScript (bun as runner) | static assets embedded into the server binary (`rust-embed`) |
| Android app | Gradle wrapper (Kotlin DSL, version catalog); a Gradle task runs `cargo ndk -t arm64-v8a` for `penwise-ffi` before packaging; `uniffi-bindgen` generates Kotlin into `build/generated` (never committed) | debug/release APK, compileSdk 37, targetSdk 36 |
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
7. **Gate D (human, only where feel matters):** M3 ink feel, M5 recognition quality.
8. **Close:** gate record in `docs/gates/M<n>.md`, one commit per green phase.

### Phase map

| Phase | Research | Contract (serial) | Parallel slices | Extra gates |
|---|---|---|---|---|
| M1 core | stroke smoothing/width methods (perfect-freehand et al.), `tiny-skia` determinism | `sample`, `brush`, `builder` API, outline type, codec signatures | (a) smoothing + width + outline, (b) codec, (c) raster + PNG goldens, (d) fixture tooling: `record-pen` + golden harness + wasm test runner | — |
| M2 server + web | `rmcp`, axum websockets, mDNS crates, `rust-embed` | `penwise-proto` ops/messages + storage schema | (a) storage + migrations, (b) sync + live deltas, (c) pairing + mDNS + auth, (d) `penwise-wasm` + web viewer | security review |
| M3 Android | `androidx.graphics` front-buffer, `MotionEventPredictor`, uniffi + Gradle + cargo-ndk, Samsung stylus specifics | `penwise-ffi` API + Gradle Rust build | (a) input + low-latency render, (b) local store + sync client, (c) pairing/discovery UI, (d) debug recorder/replay + `device-check` | human feel check |
| M4 MCP | `rmcp` tool/image result shapes | tool schemas | (a) navigation tools, (b) PNG + text tools, (c) annotate → agent layer on tablet, (d) agent layer on web, (e) MCP e2e test client | security review |
| M5 recognition | local vision models for handwriting (Ollama) | `Recognizer` trait + text-layer item shape | (a) worker + triggers, (b) Ollama backend + model evaluation on fixtures, (c) tablet scan button + text layer view | human quality check |

M2 and M3 overlap once `penwise-proto` and `penwise-ffi` contracts are committed.

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
