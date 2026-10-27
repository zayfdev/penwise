> **Superseded by docs/rewrite-design.md (owner decisions at its top override); kept for history.**

# M3 contract: `penwise-proto`, `penwise-server`, `penwise-wasm`, browser viewer

Status: committed contract for the M3 fan-out (PLAN "Phase loop" step 2, phase map row "M3 server + web").
Implementers code against it and never change it; contract changes go back to the orchestrator. Sources:
`PLAN.md` ("Sync protocol", "Networking & pairing", "Code layout", "Build process"),
`docs/research/server-stack.md` (adopted unless a decision below overrides it), `docs/contracts/m2-editing.md`
(core item/edit model, used only through its `pub` API), `docs/contracts/m1-core.md`.

## Rules for every slice

- Edit only the files your slice owns (table below). Contract-owned: the three crate roots
  (`penwise-proto/src/lib.rs`, `penwise-server/src/lib.rs`, `penwise-wasm/src/lib.rs`), every new
  `Cargo.toml`, the workspace `Cargo.toml`, `storage/migrations/v1.sql` (frozen), this doc. Need a new
  dependency, module, pub item or a signature change? Ask the orchestrator. `penwise-core` is out of scope:
  M3 uses only its `pub` API (M2 bodies land in parallel; code against signatures, test once they land).
- **Frozen:** every `pub` item (names, fields, signatures, documented behaviour), every wire/storage format
  (postcard enum variant order, `FRAME_VERSION`, `PROTOCOL_VERSION`, `Hlc::to_bytes` layout, `STAMPS_VERSION`,
  code tables of `Background`, `ClientKind`, `AnyOp` kinds, `DrawKind`, `MessageKind`, the draw-list layout,
  the QR URI grammar, the v1 schema) and the constants that define behaviour across devices (`MAX_SKEW_MS`,
  timeouts, limits).
- **Slice-internal:** private / `pub(crate)` items and private fields in your files (`HubInner`, `Pairing`
  fields, `Storage` internals, `NotebookEntry`/`PageEntry` in `catalog.rs`, …). You may add private submodules
  under a directory you own (e.g. `sync/handshake.rs`, `sync/catchup.rs`, `pairing/routes.rs`,
  `storage/stamps.rs`, `draw_list/outline.rs`) to stay under the size limits; declare them in your own `mod.rs`.
- Each file starts with `//! Owner: slice <x>` (TS: in the module doc comment); replace each `todo!()` /
  `throw new Error("unimplemented: …")` with the documented behaviour.
- Tests: Rust in sibling `tests.rs` or `crates/<crate>/tests/m3_<slice>_*.rs`; TS in `web/src/**/*.test.ts`
  (vitest). Test behaviour: round trips, convergence under shuffled/duplicated delivery, protocol sessions
  against an in-process server, rendered output.
- AGENTS.md limits: ≤ 300 lines per source file (hard 500), ≤ 80 lines per fn, ≤ 7 params; no
  `unwrap`/`expect`/`panic!` outside tests; `thiserror` errors (`anyhow` only in `penwise-server/src/main.rs`);
  `tracing` only (server); proto and wasm stay pure (no I/O, clocks, randomness, logging).
- Stub allow-list: each crate root carries `#![allow(clippy::todo, …, reason = "M3 contract stubs")]`
  (`main.rs` too). The orchestrator removes them after the last slice lands; finished code must be clean
  without them.
- Do not run gates mid-flight; the orchestrator runs `cargo fmt`, `just check`, `bun run lint/typecheck/test`
  once all slices land.

## Decisions fixed by the contract

| Topic | Decision |
|---|---|
| Ids | `DeviceId`, `OpId`, `NotebookId`, `PageId`, `ServerId`, `ItemRef` = ULID `[u8; 16]` (big-endian, `Ord` = time). Proto builds them with `from_parts(ms, random)`; callers inject time + 80 random bits. Text form = 26-char Crockford base32. The `ulid` crate is **not** used (proto's ~40 lines replace it; generation at the edges = `from_parts` + `getrandom`) |
| HLC | Hand-rolled `Hlc { wall_ms, counter: u16, device }`, `Ord` = field order; send/recv per Kulkarni; counter overflow bumps `wall_ms`; remote stamps > `MAX_SKEW_MS` (5 min) ahead are rejected. Storage form 24 bytes whose `memcmp` order = `Ord` |
| Ops | `Op { op_id, device, hlc, page, edit: core Edit }` (item ops), `PageOp`, `NotebookOp`, wrapped in append-only `AnyOp`; logged ops are `SeqOp { seq, op }` |
| LWW | One register per (target, field): items per core `Field` (13), notebook `title`/`deleted`, page `position`/`background`/`deleted`. Write wins iff its `Hlc` is strictly greater. `Add`/`Create` stamp every register; `Delete` = `Deleted := true` |
| Ordering guarantee | **Create-before-reference**: the server only logs an op whose target already exists (earlier seq) and rejects the rest (`OpRejection`). Replicas therefore never buffer orphans |
| Wire | WebSocket binary frames: `[FRAME_VERSION = 1] ++ postcard(ClientMsg | ServerMsg)`; `PROTOCOL_VERSION = 1` negotiated in `Hello`/`Welcome`; enums append-only; core edits travel as `PWED` blobs inside postcard byte strings |
| Live ink | `StrokeDelta { stroke, page, brush (encoded params), seed, start, samples: grid samples }`, every chunk self-contained; gap → drop preview until commit; producer session end → server-sent `StrokeCancel` |
| Storage | rusqlite **0.40** (`bundled`; pinned: `rusqlite_migration` 2.6 needs `^0.40`), WAL, one connection behind a mutex, blocking API used via `spawn_blocking`. Schema v1 below |
| Auth | Pairing code (16 B, 120 s, single use, memory only) → device token (32 B, base64url) stored as SHA-256; tokens in the first WS frame; `Authorization: Bearer` for `/api` |
| Network | One listener `0.0.0.0:7420` (sync + API + viewer + loopback-only pairing). `7421` reserved for MCP (M5). mDNS `_penwise._tcp` on the LAN NIC only. Tailscale identity from `tailscale status --json` every 60 s |
| Viewer | `penwise-wasm` exposes `Viewer` (proto `Replica` + `LiveStrokes`) and a flat `DrawList`; the browser rasterizes with Canvas2D (nonzero outlines, radial-gradient dabs with a baked grain pattern, `multiply` for highlighter) |
| Web tooling | Vite 8.3.5, TypeScript 7.0.2 (strict + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes`), Biome 2.6.3, vitest 5.0.5; bun as runner; wasm output in `web/src/generated/` (git-ignored, size-check exempt) |
| MCP | Not in M3 (no `mcp/` module, no `rmcp` dependency); arrives in M5 on port 7421 |

Workspace dependency pins (all from `server-stack.md` unless noted): `serde 1.0.229`, `postcard 1.1.3`
(`alloc`), `axum 0.8.9` (`ws`), `tokio 1.54.1`, `tower-http 0.7.1` (`trace`, `limit`), `futures-util 0.3.34`,
`rusqlite 0.40.2` (`bundled`), `rusqlite_migration 2.6.0`, `getrandom 0.4.3`, `base64 0.23.1`, `sha2 0.11.1`,
`subtle 2.6.1`, `mdns-sd 0.21.6`, `if-addrs 0.15.0`, `qrcode 0.14.1` (svg only), `rust-embed 8.13.0`
(`mime-guess`), `serde_json 1.0.154`, `tracing 0.1.44`, `tracing-subscriber 0.3.23` (`env-filter`),
`wasm-bindgen 0.2.129` (matches the locked `wasm-bindgen-test 0.3.79`), dev `tokio-tungstenite 0.30.0`.
`cargo deny` must pass after the fan-out (new transitive licenses are expected to be MIT/Apache/BSD/ISC/
Unicode-3.0).

## Ownership map

| Slice | Owns | Scope |
|---|---|---|
| a | proto `notebook.rs`, `op.rs`, `lww.rs`, `error.rs`; server `storage/{mod,migrations,ops,items,pages}.rs`, `render.rs`, `api.rs` | op/notebook model, LWW item merge, op log, validation, materialized items/pages/notebooks, PNG render, HTTP API |
| b | proto `ids.rs`, `hlc.rs`, `envelope.rs`, `message.rs`; server `sync/{mod,session,broadcast}.rs` | ids, clock, frames, messages, WebSocket sessions, catch-up, push/ack, broadcast, live `StrokeDelta` fan-out, resync, keep-alive |
| c | server `main.rs`, `app.rs`, `config.rs`, `pairing/{mod,token,qr,mdns,tailscale,lan,local,limiter}.rs`, `storage/devices.rs` | config, wiring/serve, pairing codes, tokens, bearer + loopback guards, QR, mDNS, Tailscale + LAN discovery, rate limiting, device management routes |
| d | proto `live.rs`, `catalog.rs`, `replica.rs`; `crates/penwise-wasm/src/{draw_list,received,viewer,encode,grain}.rs`; `web/**`; server `web.rs` | client replica + live strokes, wasm bindings, Canvas2D viewer, pairing/page UI, embedded assets |
| contract | crate roots, all `Cargo.toml`, `storage/migrations/v1.sql`, this doc | shared types, formats, schema |

## Module tree

```
crates/penwise-proto/src/
  lib.rs        module tree + re-exports (contract)
  ids.rs        DeviceId, OpId, NotebookId, PageId, ServerId, ItemRef, IdParseError (b)
  hlc.rs        Hlc, HlcClock, HlcError, MAX_SKEW_MS, MAX_WALL_MS (b)
  envelope.rs   FRAME_VERSION, PROTOCOL_VERSION, MIN_PROTOCOL_VERSION, MAX_FRAME_BYTES, encode/decode (b)
  message.rs    ClientMsg, ServerMsg, AuthToken, PairingCode, ClientKind, RejectReason, NotebookInfo, PageInfo,
                OpRejected, timeouts (b)
  notebook.rs   Notebook, Page, PageKind, Background, NotebookChange/Patch, PageChange/Patch (a)
  op.rs         Op, PageOp, NotebookOp, AnyOp, SeqOp, OpRejection, edit_blob serde adapter (a)
  lww.rs        Register, FieldStamps, MaterializedItem, LwwError (a)
  error.rs      ProtoError, ReplicaError (a)
  catalog.rs    Catalog (d)
  replica.rs    PageReplica, Replica, Applied (d)
  live.rs       LiveSample, StrokeDelta, StrokeCancel, LiveStroke, LiveStrokes, LiveError (d)
crates/penwise-server/src/
  main.rs       binary: tracing, config, storage, App::start/serve, signals (c)
  lib.rs        module tree (contract)
  app.rs        App, AppError, now_ms (c)
  config.rs     Config, ConfigError, ports (c)
  storage/      mod.rs Storage + StorageError; migrations.rs + migrations/v1.sql; ops.rs; items.rs; pages.rs (a);
                devices.rs (c)
  sync/         mod.rs SyncHub + routes + origin check; session.rs; broadcast.rs (b)
  pairing/      mod.rs Pairing + Endpoints + routes; token.rs; qr.rs; mdns.rs; tailscale.rs; lan.rs; local.rs;
                limiter.rs (c)
  render.rs     page → PNG (a)
  api.rs        /api routes (a)
  web.rs        embedded viewer (d)
crates/penwise-wasm/src/
  lib.rs        module tree (contract)
  viewer.rs     Viewer; draw_list.rs DrawList + DrawKind; received.rs Received + entries; encode.rs client
                frames + new_device_id; grain.rs paper grain (d)
web/
  package.json, tsconfig.json, biome.json, vite.config.ts, index.html, .gitignore
  src/main.ts, connection.ts, viewer.ts, style.css, ui/pages.ts, ui/pairing.ts (d)
  src/generated/penwise-wasm/   wasm-pack output (generated, ignored)
```

## Data flow

```
Tablet (M4)                      penwise-server                                   Browser viewer
core tool → Vec<Edit>
HlcClock::send → Op ──PushOps──► session ─► Storage::append_ops (1 txn) ─► Ack ──► tablet
StrokeBuilder chunks             │   validate · log (seq) · materialize items/pages
 ──StrokeDelta──────────────────►│   └─► Broadcast::ops (OpsSince, 1 frame/push) ──► Viewer::receive
                                 └─► Broadcast::stroke_delta (no echo) ───────────►   Replica::apply_seq
                                                                                       LiveStrokes::apply_delta
GET /api/pages/{p}/png ─► Storage::page_items ─► PageReplica::from_items ─► render_page_png (tiny-skia)
Viewer::render_page_paths / render_live_paths ─► DrawList ─► web/src/viewer.ts (Canvas2D)
```

## Wire protocol

Frame = `FRAME_VERSION (u8 = 1)` + postcard body, nothing after; `≤ MAX_FRAME_BYTES` (4 MiB). Stored op bodies
(`ops.body`) use the same envelope around `AnyOp`. Variant numbers are postcard discriminants (frozen).

### Client → server (`ClientMsg`)

| # | Variant | Fields | Server reaction |
|---|---|---|---|
| 0 | `Hello` | `protocol_version, device, token, last_seen_seq, client` | `Welcome` + catch-up from `last_seen_seq`, or `Reject` + close 4001 |
| 1 | `Pair` | `protocol_version, code, device, name, client` | `Paired { token, server }` (then expects `Hello`) or `Reject` |
| 2 | `PushOps` | `ops: Vec<AnyOp>` (1..=512) | one `Ack`; logged ops broadcast as one `OpsSince` |
| 3 | `StrokeDelta` | `StrokeDelta` | relayed to every other session |
| 4 | `StrokeCancel` | `StrokeCancel` | relayed to every other session |
| 5 | `SwitchPage` | `page` | stored as current page; relayed |
| 6 | `ListNotebooks` | — | `NotebookList` |
| 7 | `ListPages` | `notebook` | `PageList` |
| 8 | `Resync` | `from_seq` | `Resync` + catch-up |
| 9 | `Ping` | `nonce` | `Pong` |
| 10 | `Pong` | `nonce` | — |

### Server → client (`ServerMsg`)

| # | Variant | Fields | Meaning |
|---|---|---|---|
| 0 | `Welcome` | `protocol_version, server, device, head_seq, current_page` | handshake ok |
| 1 | `Reject` | `reason` (`BadToken`, `UnsupportedProtocol{min,max}`, `BadPairingCode`, `Malformed`, `RateLimited`) | then close 4001 |
| 2 | `Paired` | `token, server` | pairing ok; token shown once |
| 3 | `Ack` | `op_ids, seqs, rejected: Vec<OpRejected>` | every pushed op in exactly one list; duplicates report their original seq |
| 4 | `OpsSince` | `from_seq, ops: Vec<SeqOp>, head_seq, more` | catch-up batches (≤ 512 ops) and live broadcast (one per push, echoed to the author) |
| 5 | `StrokeDelta` | `device, delta` | live ink (never echoed) |
| 6 | `StrokeCancel` | `device, cancel` | live stroke abandoned |
| 7 | `SwitchPage` | `page, by` | follow mode |
| 8 | `NotebookList` | `notebooks: Vec<NotebookInfo>` | snapshot |
| 9 | `PageList` | `notebook, pages: Vec<PageInfo>` | snapshot |
| 10 | `Resync` | `from_seq` | lag/requested: drop live state; batches follow |
| 11/12 | `Ping`/`Pong` | `nonce` | keep-alive (`PING_INTERVAL_MS` 15 s, `IDLE_TIMEOUT_MS` 45 s, handshake 5 s) |

### Ops (`AnyOp`, `ops.kind`)

| Kind | Variant | Payload | Registers written |
|---|---|---|---|
| 0 | `Item(Op)` | `page`, core `Edit` (`Add` / `Delete` / `Update{id, patch}`) as `PWED` blob | `Add`: all 13 item fields; `Delete`: `Deleted`; `Update`: the patch's fields |
| 1 | `Page(PageOp)` | `page`, `PageChange::{Create{notebook, position, kind, background}, Update(PagePatch)}` | `position`, `background`, `deleted` |
| 2 | `Notebook(NotebookOp)` | `notebook`, `NotebookChange::{Create{title}, Update(NotebookPatch)}` | `title`, `deleted` |

`OpRejection` (in `Ack.rejected`): `NotOwnDevice`, `ClockSkew`, `Invalid`, `UnknownPage`, `UnknownNotebook`,
`UnknownItem`, `Duplicate`, `WrongPage`. Rejected ops are never logged; the tablet surfaces them (clock skew →
"fix device time") and drops them from its outbox.

### Versioning rules

- Append-only enums; new shapes = new variants used only when both sides' `protocol_version` allows it.
- `Hello`, `Pair`, `Welcome`, `Reject`, `Paired` are frozen forever (negotiation depends on them).
- `FRAME_VERSION` changes only with a postcard major or envelope change. Core blobs (`PWED`, `PWIT`) carry their
  own versions. `items.stamps` carries `STAMPS_VERSION`.
- Each proto change ships fixtures `crates/penwise-proto/tests/fixtures/v<N>/*.bin` and a compat test decoding
  the previous version's fixtures (AGENTS.md). Slice b creates the v1 fixtures (one frame per variant).

## LWW resolution

Per (target, field) register `(value, stamp)`; a write `(v, h)` wins iff `h > stamp` in `Hlc` order
(`wall_ms`, `counter`, `device` bytes). Equal stamp = same write (no change). Every write is absolute, so any
delivery order, with duplicates, converges. Items: `MaterializedItem::merge` returns the winning sub-edit
(`ItemPatch::restrict(winners)`) that is then fed to `Scene::apply`, keeping the scene equal to the registers.
Notebook/page registers: proto `Catalog` (replicas) and SQL `… WHERE x_hlc < ?` updates (server); both must
follow the same rule. Immutable data (item kind data, page kind/notebook) comes from the creating op only.
Causality: a device stamps new ops after observing (`HlcClock::recv`) every op it has seen, so a reference's
stamp exceeds its target's creation stamp.

## Storage schema (v1, `storage/migrations/v1.sql`)

| Table | Key | Purpose |
|---|---|---|
| `server_meta` | `key` | `server_id` (16 B), `created_ms` |
| `devices` | `id` | paired clients: name (1..=64 B), kind, created/last seen |
| `tokens` | `hash` (SHA-256, 32 B) | device tokens (never the token), created/last used/revoked; index `(device)` |
| `ops` | `seq INTEGER PRIMARY KEY AUTOINCREMENT` | append-only log: `op_id UNIQUE`, `device`, `hlc` (24 B), `kind`, `notebook`, `page`, `item`, `body` (envelope), `received_ms`; partial indexes `(page, seq)`, `(item, seq)` |
| `notebooks` | `id` | registers `title`/`deleted` + `*_hlc`, `created_hlc`, `updated_ms` |
| `pages` | `id` | immutable `notebook`, `kind`/`width`/`height`; registers `position`/`background`/`deleted` + `*_hlc`; index `(notebook, deleted, position, id)` |
| `items` | `id` | winning state: `page`, `kind`, `deleted`, `data` (PWIT v1), `stamps` (v1 + postcard `FieldStamps`), `added_seq`, `updated_seq`; index `(page, deleted)` |

Pragmas on every open: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`.
The task sketch's `ops.edit` column is `ops.body` (it also carries page/notebook ops). STRICT tables throughout.

## Network

| Port | Bind | Serves | Auth |
|---|---|---|---|
| 7420 | `0.0.0.0` (`--listen`) | `/sync` WS, `/api/*`, `/pair*`, viewer assets | first-frame token (`/sync`), bearer (`/api` except `/api/health`), loopback + Host/Origin check (`/pair*`), none (assets, health) |
| 7421 | — (M5: `127.0.0.1` + Tailscale IPv4) | MCP | per-agent bearer (M5) |
| 5353/udp | mDNS | `_penwise._tcp` (`v`, `sid` TXT only) | — |

QR payload: `penwise://pair?v=1&sid=<26>&code=<22>&port=7420[&lan=<IPv4>][&ts=<MagicDNS>]` (≤ 160 bytes,
SVG, ECC M). Tablet order: LAN (mDNS-resolved or `lan`) first, then `ts` (WireGuard-encrypted).

## Security notes (for the security reviewer)

1. **Token storage.** Tokens (256-bit) and pairing codes (128-bit) come from `getrandom`. Only
   `SHA-256(token text)` is stored (`tokens.hash`); the clear token exists once in the `Paired` frame and on the
   device. Lookup is by hash primary key, then a `subtle` constant-time compare. Unsalted SHA-256 is adequate
   for 256-bit random secrets (no dictionary). Revocation: `revoked_ms` + live sessions closed
   (`SyncHub::disconnect_device`, close 4003). `AuthToken`/`PairingCode` `Debug` print `<redacted>`; never log
   frames, headers or query strings. Browser tokens live in `localStorage` (origin-scoped); CSP blocks script
   injection (`web.rs` `CONTENT_SECURITY_POLICY`).
2. **Pairing page is local-only.** `/pair*` require all of: loopback TCP peer, `Host` ∈ {`localhost`,
   `127.0.0.1`, `[::1]`} (defeats DNS rebinding), same-set `Origin` if present, no `X-Forwarded-For`/`Forwarded`
   (a reverse proxy would make remote peers look local); otherwise 404. Codes: single use (consumed even when
   the pair attempt then fails), 120 s, ≤ 8 live, held in memory as hashes. Any local process of the user can
   obtain a code: local processes are inside the trust boundary.
3. **Handshake hardening.** First frame within 5 s or close; failed handshakes rate-limited per IP (10 / 60 s,
   loopback exempt); identical `BadToken` for unknown/revoked/other-device tokens and identical
   `BadPairingCode` for unknown/used/expired codes. Frames ≤ 4 MiB, `PushOps` ≤ 512 ops, deltas ≤ 256 samples,
   ≤ 4 live strokes per device; HTTP bodies ≤ 64 KiB; decoding never panics.
4. **Cross-site WebSocket hijacking.** `/sync` refuses a present `Origin` whose host is not loopback, the LAN
   address or the Tailscale name/IP (403); no CORS anywhere. Auth is in-band anyway, so a hostile page without
   a token gets nothing.
5. **Authorization scope.** Every paired device has full read/write on all notebooks (single-user system).
   Ops must be authored by the authenticated device (`op.device == hlc.device == session device`), so a device
   cannot forge another's writes or win LWW ties with another's id. Clock-skewed ops (> 5 min ahead) are
   rejected so a wrong clock cannot pin LWW forever.
6. **LAN plaintext `ws://` (main open risk).** On the LAN path the device token, pairing exchange and all ink
   travel unencrypted: a LAN eavesdropper can capture a token and replay it, or read notes; an active attacker
   can inject frames. Tailscale traffic is WireGuard-encrypted. **Recommendation:** (a) M3 ships plaintext LAN
   documented as trusted-home-LAN only, with `--listen 127.0.0.1:7420` + Tailscale as the hardened setup;
   (b) before M4 sync goes to daily use, add TLS on 7420 with a self-signed certificate generated on first start
   (`rustls` + `rcgen`, both MIT/Apache) and pin its SHA-256 fingerprint via a new trailing `fp=` QR parameter
   (PLAN's `server_pubkey_fp`); the browser on loopback keeps `http://localhost`. Decision for the security
   review; M3 code must not preclude it (`ServerId`/QR grammar already allow appending `fp`).
7. **mDNS** advertises only `v` and `sid` on the physical LAN interface (no docker/bridge/VPN interfaces); no
   secrets in TXT. Disable with `--no-mdns`.
8. **Storage integrity.** Ops are validated before logging (stateless `AnyOp::validate` incl. core
   `Item::validate`, plus create-before-reference); corrupt stored rows surface as `StorageError::Corrupt`,
   never panics. The data dir should be `0700` (created by `main` with `DirBuilderExt::mode(0o700)`).

## Invariants

1. **Convergence.** Any two replicas that applied the same set of ops (any order consistent with seq, plus
   duplicates) have identical catalogs, item registers and scenes; the server's tables equal a replica's state.
2. **Idempotency.** Re-pushing an op id returns its original seq and changes nothing; re-applying a `SeqOp`
   with `seq ≤ head_seq` is a no-op.
3. **Create-before-reference** (see Decisions) holds for every logged op.
4. **Seq.** Strictly increasing, gap-free from the client's view after catch-up (sessions fill gaps from
   storage), never reused.
5. **Live ink is ephemeral.** Never logged; may be dropped; the committed `Add` always replaces the preview.
6. **No panics, no secrets in logs**, decoding of every frame/blob is total.
7. **Bindings stay thin.** `penwise-wasm` and `web/` never decide protocol rules or geometry.

## Known limitations (accepted for M3)

- Text items are not drawn in server PNGs (no font rasterizer until M6); the browser draws them with canvas text.
- Dab strokes use the same per-dab falloff, grain (per dab, before the union) and minimum fade on the server, the tablet and the browser: each client ports `raster::stamps` and replays `crates/penwise-core/tests/goldens/stamp_coverage.txt` in its tests.
- A client that joins mid-stroke sees that stroke only from its next `start == 0` delta or its commit.
- LAN path is plaintext (security note 6).
- The browser viewer is read-only (no drawing, no op push) in M3.

## Cross-slice dependencies

- b lands `ids.rs` + `hlc.rs` first (small) and messages a, c, d; a lands `op.rs` accessors + `edit_blob`
  early (b's envelope tests and d's replica need them).
- a's storage uses b's `Hlc::to_bytes` and `AnyOp::validate` (a); c's `storage/devices.rs` uses a's
  `Storage::with_conn`.
- b's session calls a's `append_ops`/`ops_since`/`notebook_list`/`page_list`, c's `Pairing::{redeem,
  authenticate, limiter}`.
- a's `render.rs` uses d's `PageReplica::from_items` (and core raster).
- c's `App::router` merges `sync::routes` (b), `pairing::routes` (c), `api::routes` (a), `web::routes` (d).
- d's `Replica` uses a's `MaterializedItem::merge` and `Register`; the wasm `Viewer` uses d's proto files and
  b's `decode_server`/`encode_client`.
- Everything that touches scenes needs M2's `Scene`/codec bodies; tests that need them run after M2 lands.

## justfile recipes to add (orchestrator)

```
# Build the wasm bindings and the viewer bundle (web/dist, embedded by penwise-server).
web:
    cd web && bun install --frozen-lockfile && bun run wasm && bun run build

# Run the server against ./data (debug build serves web/dist from disk).
serve:
    cargo run -p penwise-server -- --data-dir data
```

`check` additionally gains `web-check: cd web && bun run wasm && bun run typecheck && bun run lint && bun run test`
and `cargo build -p penwise-wasm --target wasm32-unknown-unknown`. `release` builds `web` before
`cargo build --release -p penwise-server`. Build `penwise-wasm` with `-p` only (never `--workspace`) so the
server's `raster` feature is not unified into it.

## Per-slice `todo!()` list

### a — op model, LWW items, storage, render, API
- proto `notebook.rs`: `PageKind::to_scene_kind`, `is_valid`; `Background::code`, `from_code`;
  `NotebookChange::is_valid`; `PageChange::is_valid`
- proto `op.rs`: `AnyOp::op_id`, `device`, `hlc`, `page`, `item`, `kind_code`, `validate`;
  `edit_blob::serialize`, `deserialize`
- proto `lww.rs`: `Register::new`, `set`; `FieldStamps::new`, `get`, `set`; `MaterializedItem::from_add`, `merge`
- server `storage/mod.rs`: `Storage::open`, `open_in_memory`, `with_conn`
- `storage/migrations.rs`: `migrations`, `migrate`
- `storage/ops.rs`: `append_ops`, `ops_since`, `head_seq`, `max_hlc`
- `storage/items.rs`: `page_items`, `item_summaries`, `materialize_item_op`
- `storage/pages.rs`: `notebook_list`, `page_list`, `page`, `materialize_notebook_op`, `materialize_page_op`
- `render.rs`: `RenderOptions::whole_page`, `render_page_png`, `draw_background`
- `api.rs`: `routes`
- Tests: LWW merge commutes (all permutations of 3 concurrent updates + duplicates); `edit_blob` round trip;
  migration `validate()` + table set; append idempotency (same op twice → same seq); every `OpRejection`
  path; a page create + strokes in one batch; materialized items equal a proto `Replica` fed the same log;
  fixture stroke → PNG decodes with ink pixels (perceptual), highlighter under ink, masked pencil stroke has a
  hole; `/api` 401 without token.

### b — ids, clock, frames, sessions, broadcast
- proto `ids.rs` (each id type): `from_parts`, `timestamp_ms`, `to_bytes`, `from_slice`, `Display`, `Debug`,
  `FromStr`; `From<ItemId> for ItemRef`, `From<ItemRef> for ItemId`
- proto `hlc.rs`: `Hlc::zero`, `pack`, `to_bytes`, `from_bytes`; `HlcClock::new`, `resume`, `last`, `send`, `recv`
- proto `envelope.rs`: `encode_client`, `decode_client`, `encode_server`, `decode_server`, `encode_op`,
  `decode_op`
- proto `message.rs`: `ClientKind::code`, `from_code`
- server `sync/mod.rs`: `SyncHub::new`, `broadcast`, `current_page`, `switch_page`, `register`, `unregister`,
  `disconnect_device`, `shutdown`, `session_count`; `routes`; `origin_allowed`
- `sync/broadcast.rs`: `Broadcast::new`, `subscribe`, `ops`, `stroke_delta`, `stroke_cancel`, `switch_page`
- `sync/session.rs`: `SessionHandle::new`, `close`; `run`
- Tests: ULID text round trip + Crockford aliases; HLC send/recv monotonic, overflow, skew rejection,
  `to_bytes` order = `Ord` (property test over random pairs); every message round trip; v1 frame fixtures +
  compat test; truncated/garbage frames → errors; in-process server (`tokio-tungstenite`): handshake
  success/reject/timeout, push → ack → broadcast to a second client, duplicate push, catch-up across batches,
  forced lag → `Resync` → convergence, `StrokeDelta` relayed but not echoed, session drop → `StrokeCancel`,
  bad Origin → 403.

### c — config, wiring, pairing, discovery
- `config.rs`: `Config::from_args`, `db_path`, `port`
- `app.rs`: `now_ms`, `App::start`, `router`, `serve`; `main.rs`: `main`
- `storage/devices.rs`: `ensure_server_id`, `pair_device`, `token_device`, `touch_token`, `devices`,
  `revoke_device`
- `pairing/mod.rs`: `Pairing::new`, `limiter`, `issue_code`, `redeem`, `authenticate`, `device_for_bearer`;
  `spawn_discovery`; `routes`
- `pairing/token.rs`: `generate_token`, `generate_code`, `random_server_id`, `hash_secret`, `secrets_match`,
  `require_bearer`
- `pairing/qr.rs`: `PairingPayload::to_uri`, `parse`; `render_svg`
- `pairing/mdns.rs`: `MdnsAdvertiser::start`, `shutdown`
- `pairing/tailscale.rs`: `parse_status`, `query`
- `pairing/lan.rs`: `pick_lan_ipv4`, `lan_ipv4`
- `pairing/local.rs`: `is_local_request`, `require_local`
- `pairing/limiter.rs`: `FailureLimiter::new`, `allowed`, `record_failure`
- Tests: flag parsing incl. errors and data-dir precedence; code single-use + expiry + eviction; redeem →
  authenticate ok, revoked → `BadToken`, other device's token → `BadToken`; tokens never stored in clear (scan
  the db file); QR URI round trip + length ≤ 160; `parse_status` on recorded JSON (running, stopped, missing
  fields); LAN pick ignores docker/bridge/tailscale; local guard matrix (peer × Host × Origin × forwarded);
  limiter window; `/pair` 404 from a non-loopback peer.

### d — client replica, live ink, wasm, viewer
- proto `catalog.rs`: `Catalog::new`, `apply_notebook`, `apply_page`, `notebook`, `page`, `notebooks`, `pages`
- proto `replica.rs`: `PageReplica::new`, `from_items`, `apply`, `scene`, `item`; `Replica::new`, `apply`,
  `apply_seq`, `head_seq`, `catalog`, `page`
- proto `live.rs`: `LiveSample::from_grid`, `to_grid`, `to_sample`; `LiveStroke::device`, `page`, `geometry`;
  `LiveStrokes::new`, `apply_delta`, `cancel`, `commit`, `clear`, `on_page`
- wasm `draw_list.rs`: `DrawList::{points, runs, dabs, styles, commands, mask_widths, texts, text_sizes,
  is_empty, from_scene, push_live}`
- wasm `received.rs`: `NotebookEntry::from_info`; `PageEntry::from_info`, `from_page`; `Received::empty`;
  `reject_name`
- wasm `viewer.rs`: `Viewer::new`, `receive`, `head_seq`, `render_page_paths`, `render_live_paths`,
  `content_bounds`, `notebooks`, `pages`, `page`, `clear_live`
- wasm `encode.rs`: `protocol_version`, `new_device_id`, `encode_hello`, `encode_pair`, `encode_switch_page`,
  `encode_list_notebooks`, `encode_list_pages`, `encode_resync`, `encode_ping`, `encode_pong`
- wasm `grain.rs`: `grain_period_px`, `bake_grain_tile`, `grain_coverage_lut`
- server `web.rs`: `routes`
- web `connection.ts`: `loadCredentials`, `saveCredentials`, `clearCredentials`, `syncUrl`, `Connection`
  (`constructor`, `start`, `pair`, `send`, `stop`)
- web `viewer.ts`: `fitView`, `GrainCache` (`pattern`, `clear`), `PageRenderer` (`constructor`, `resize`,
  `setView`, `drawPaper`, `drawInk`, `drawLive`)
- web `ui/pages.ts`: `PageList` (`constructor`, `render`), `choosePage`
- web `ui/pairing.ts`: `fetchPairing`, `parsePairingInfo`, `isLoopbackHost`, `PairingPanel` (`constructor`,
  `showQr`, `showCodeEntry`, `showError`, `hide`)
- web `main.ts`: `main`
- Tests: replica convergence (shuffle item updates between creates, duplicates) equals a sequential
  application; live chunk rule (in-order, gap, restart, cancel, commit); `DrawList` layout for one outline,
  one stamp stroke, one masked stroke (group + mask), one fill (even-odd), one text item; wasm-bindgen-test of
  `Viewer::receive` on encoded frames; `web.rs` serves `index.html`, `application/wasm`, 304 on ETag; vitest:
  credentials round trip, `syncUrl`, `fitView`, `choosePage`, `parsePairingInfo`, `isLoopbackHost`;
  Gate C: replay a fixture through a test client and see it live, then committed, in the browser.

## Contract-implemented items (usable now)

Constants and data types only: every struct/enum definition above, all `pub const`s (ids, `MAX_SKEW_MS`,
`MAX_WALL_MS`, `HLC_BYTES`, `POSITION_STEP`, `MAX_TITLE_BYTES`, `MAX_PAGE_SIDE_PX`, `BACKGROUND_SPACING_PX`,
`MAX_DELTA_SAMPLES`, `MAX_LIVE_STROKES_PER_DEVICE`, timeouts, `FRAME_VERSION`, `PROTOCOL_VERSION`,
`MIN_PROTOCOL_VERSION`, `MAX_FRAME_BYTES`, ports, `MIGRATION_SQL`, `SCHEMA_VERSION`, `STAMPS_VERSION`,
render/pairing/limiter/mDNS/QR constants, `CONTENT_SECURITY_POLICY`, draw-list strides), the v1 SQL, the
redacting `Debug` of `AuthToken`/`PairingCode`, `MdnsAdvertiser`'s `Debug`, `Replica`/`SyncHub`/`Viewer`
`Default` (delegating to `new`), the web tooling configs, `index.html` and `style.css`. Every function is a
`todo!()` / `unimplemented` stub.
