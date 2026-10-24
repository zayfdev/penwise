# Penwise — Stage 1 Plan

Pen-first notes and canvas on a Galaxy Tab S9 + Staedtler Noris digital (EMR), live-synced to a PC
browser view and readable/writable by agents over MCP.

## Decisions

| Area | Decision |
|---|---|
| Device | Galaxy Tab S9 family, EMR pen (pressure, tilt, hover, side button) |
| Tablet client | Native Android, Kotlin |
| Ink engine | Custom Rust core (`penwise-core`), one engine for tablet, browser and server PNG rendering |
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

### Repository layout

```
penwise/
  crates/
    penwise-core/      stroke model, smoothing, pressure→width, outline geometry, binary codec, raster
    penwise-proto/     change-log ops + wire messages (serde, versioned)
    penwise-server/    axum, ws sync, SQLite, pairing, MCP, recognizer worker, static viewer
    penwise-ffi/       uniffi bindings → Kotlin (cargo-ndk, arm64-v8a)
    penwise-wasm/      wasm-bindgen bindings → browser
  android/             Kotlin app (Gradle)
  web/                 TypeScript viewer (Vite), built into the server binary
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

- **Fixture recorder** (debug build): records raw pen samples from real writing into `fixtures/*.pwrec`.
- **Replay harness** (debug build only): `adb shell am broadcast` with a fixture path feeds recorded samples through the
  same input pipeline as the pen. `adb shell input stylus` can't set pressure, so it isn't enough on its own.
- **Screen checks:** `adb exec-out screencap -p` compared against goldens rendered by `penwise-core` natively.
- **End to end:** after replay, assert the stroke exists on the server (HTTP debug endpoint), in the browser view
  (browser automation), and in `get_page_png` (MCP test client).
- **Feel checks** stay human: latency and pressure response are judged by you on the device each milestone.

## Milestones

| # | Milestone | Done when |
|---|---|---|
| M0 | Toolchain | Android SDK/NDK, cargo-ndk, wasm-pack, uniffi set up; `aarch64-linux-android` + `wasm32-unknown-unknown` targets |
| M1 | `penwise-core` | Recorded sample streams → identical outlines/PNG across native and WebAssembly (golden tests); codec round-trips |
| M2 | Server + browser view | Op log in SQLite; browser renders pages and live `StrokeDelta`s; pairing QR |
| M3 | Android app | Low-latency pressure ink on the Tab S9; local store; LAN + Tailscale sync; offline drawing syncs on reconnect |
| M4 | MCP | All stage-1 tools usable from an agent; annotations appear live on tablet + browser |
| M5 | Recognition | Scan button + auto-scan produce text layer; `get_page_text` returns it |

## Toolchain status (checked 2026-10-24)

- Present: rustc 1.93, cargo, OpenJDK 21, bun, node 22, tailscale (this PC = `deskpc`), ollama (not running, no models listed).
- Missing: Android SDK/NDK, adb, gradle (wrapper will cover), cargo-ndk, wasm-pack, rust targets `aarch64-linux-android`, `wasm32-unknown-unknown`.

## Later stages (parking lot)

- ML Kit on-device recognition for fully offline moments
- Paper notes scanning (camera → page image + recognition)
- Search across notebooks, export (PDF/SVG/Markdown), templates, shape recognition
- Agent-driven pages (agent creates a page with a diagram for you to annotate)
- Open-source packaging: release binaries, APK, docs
