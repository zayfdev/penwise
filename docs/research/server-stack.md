> **Historical research (2026-10), written before the op-log rewrite; current server: docs/rewrite-design.md, docs/pages.md.**

# penwise-server stack research (2026-10)

Scope: crates and external services for `crates/penwise-server` (and the server-side half of `penwise-proto`).
Versions and licenses were pulled from the crates.io API on 2026-10-24. Licenses are SPDX strings from crates.io;
every crate recommended here falls within the AGENTS.md allow-list (MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode-3.0).

| Crate | Version | License | Use |
|---|---|---|---|
| `rmcp` | 3.7.0 (2026-10-22) | Apache-2.0 (MIT for legacy contributions) | MCP server |
| `axum` | 0.8.9 | MIT | HTTP + WebSocket (`ws` feature) |
| `tokio` | 1.54.1 | MIT | runtime |
| `tower` / `tower-http` | 0.5.3 / 0.7.1 | MIT | middleware (trace, limit, cors) |
| `subtle` | 2.6.1 | BSD-3-Clause | constant-time token compare |
| `sha2` / `getrandom` / `base64` | 0.11.1 / 0.4.3 / 0.23.1 | MIT OR Apache-2.0 | token hashing, generation, encoding |
| `rusqlite` | **0.40.2** (0.41.0 shipped 2026-10-24, see pitfalls) | MIT | SQLite (`bundled` = SQLite 3.54.0) |
| `rusqlite_migration` | 2.6.0 | Apache-2.0 | migrations via `user_version` |
| `postcard` | 1.1.3 | MIT OR Apache-2.0 | wire + storage blobs |
| `ulid` | 3.0.0 | MIT | ids |
| `mdns-sd` | 0.21.6 | Apache-2.0 OR MIT | mDNS responder |
| `qrcode` | 0.14.1 | MIT OR Apache-2.0 | pairing QR (SVG) |
| `rust-embed` | 8.13.0 | MIT | embed web viewer |
| `reqwest` | 0.13.5 | MIT OR Apache-2.0 | Ollama client (optional; rmcp already pulls hyper) |

---

## 1. MCP: `rmcp` (official Rust SDK)

**Status.** `rmcp` 3.7.0 is the official SDK ([crates.io](https://crates.io/crates/rmcp), [repo](https://github.com/modelcontextprotocol/rust-sdk)).
It implements MCP spec **2026-07-28** and stays compatible with 2025-11-25 and earlier ([docs.rs README](https://docs.rs/crate/rmcp/latest)).
It has moved fast: 1.0 shipped in 2026-03, 2.0 in 2026-06 and 3.0 on 2026-07-28. Pin it with `=3.7.0` and treat upgrades as deliberate work.
The repo is moving from MIT to Apache-2.0. New code is Apache-2.0, and older contributions may still be MIT ([LICENSE](https://github.com/modelcontextprotocol/rust-sdk/blob/main/LICENSE)). Both are on the allow-list.

**Features:** `server`, `macros` (both on by default), `schemars` (tool input schemas), and `transport-streamable-http-server`. The `auth` feature is the OAuth 2.0 client/server flow; we don't need it for a static bearer token.

**Defining tools.** Use `#[tool_router]` with `#[tool]`. Add `#[tool_handler]` when you need server metadata ([README](https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md)):

```rust
use rmcp::{handler::server::wrapper::Parameters, model::{CallToolResult, ContentBlock},
           schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct PageArg { /// Page ULID
    page_id: String, scale: Option<f32> }

#[derive(Clone)] struct PenwiseMcp { app: AppHandle }   // AppHandle: Arc<…> (db, render, bus)

#[tool_router]
impl PenwiseMcp {
    #[tool(description = "Render a page to PNG")]
    async fn get_page_png(&self, Parameters(a): Parameters<PageArg>) -> Result<CallToolResult, McpError> {
        let png: Vec<u8> = self.app.render_png(&a.page_id, a.scale).await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![
            ContentBlock::text(format!("page {} rendered", a.page_id)),
            ContentBlock::image(base64::engine::general_purpose::STANDARD.encode(png), "image/png"),
        ]))
    }
}
#[tool_handler(name = "penwise", version = "0.1.0", instructions = "Pen notes canvas")]
impl ServerHandler for PenwiseMcp {}
```

- Images and audio are passed as base64 strings plus a MIME type. `ResourceContents::blob(b64, uri)` embeds binary resources.
- Error split: return `Ok(CallToolResult::error(..))` when the tool ran but the input was bad (the agent sees the message). Return `Err(McpError)` for protocol or internal faults ([README "Error handling"](https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md)).
- The generated `inputSchema` comes from the fields of the `Parameters<T>` type and their `///` docs. The type's own doc comment is ignored.
- With 2026-07-28, `structuredContent` can be any JSON value (SEP-2106). Use it for `get_page_text` and similar tools as well as text.

**Streamable HTTP and mounting into axum.** `StreamableHttpService` is a Tower service:

```rust
use rmcp::transport::streamable_http_server::{StreamableHttpService, StreamableHttpServerConfig,
    session::local::LocalSessionManager};
let cfg = StreamableHttpServerConfig::default()
    .with_legacy_session_mode(false)     // stateless also for pre-2026 clients
    .with_json_response(true)            // single JSON body for simple tools
    .with_allowed_hosts(["localhost", "127.0.0.1", "deskpc.tail000000.ts.net", "100.64.10.100"]);
let app = app.clone();
let mcp = StreamableHttpService::new(move || Ok(PenwiseMcp { app: app.clone() }),
                                     LocalSessionManager::default().into(), cfg);
let router = axum::Router::new().nest_service("/mcp", mcp)
    .layer(axum::middleware::from_fn_with_state(tokens.clone(), require_bearer));
```

- In 3.x the default is **stateless**: the factory closure runs once per request. Keep all state in a cloned `Arc` handle ([README "Server-side"](https://github.com/modelcontextprotocol/rust-sdk/blob/main/README.md)).
- DNS-rebinding guard: `allowed_hosts` defaults to `localhost`, `127.0.0.1` and `::1`. Requests reaching the server by its Tailscale IP or MagicDNS name get rejected until those are added. `allowed_origins` is empty by default, which turns Origin checking off ([`tower.rs`](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/transport/streamable_http_server/tower.rs)). Agents aren't browsers, so `with_allowed_origins([])` plus enabling validation rejects any browser Origin.
- `max_request_body_bytes` is configurable. Keep it small: our tool inputs are tiny.

**Auth.** The SDK's own example `simple_auth_streamhttp.rs` wraps `nest_service("/mcp", …)` in `middleware::from_fn_with_state` and checks `Authorization: Bearer …` ([example](https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/simple_auth_streamhttp.rs)). Use the same pattern, but compare tokens in constant time (see §2). MCP clients such as Claude Code accept static headers, e.g. `"headers": {"Authorization": "Bearer …"}`. [UNVERIFIED for every client]

**Recommendation:** `rmcp = { version = "=3.7.0", features = ["server", "macros", "schemars", "transport-streamable-http-server"] }`. Run it stateless with JSON responses, nest it at `/mcp` on a **separate listener** (loopback + Tailscale IP), and put a bearer middleware in front with a per-agent token. Set `allowed_hosts` explicitly.

---

## 2. axum + tokio WebSockets, tower auth

**WebSocket.** In axum 0.8.9, enable `features = ["ws"]`. `Message::Binary` carries `bytes::Bytes` ([docs.rs axum::extract::ws](https://docs.rs/axum/0.8.9/axum/extract/ws/index.html)). axum ships its own tungstenite internally (0.29), so don't add `tokio-tungstenite` to the server. Only an integration-test client would need it (0.30.0).

```rust
async fn ws(ws: WebSocketUpgrade, ConnectInfo(peer): ConnectInfo<SocketAddr>, State(s): State<App>) -> Response {
    ws.max_message_size(4 << 20).on_upgrade(move |sock| session(sock, peer, s))
}
async fn session(sock: WebSocket, peer: SocketAddr, s: App) {
    let (mut tx, mut rx) = futures_util::StreamExt::split(sock);
    // 1) first frame must be Hello{proto_version, device_id, token} within 5 s, else close(4001)
    // 2) writer task: select over broadcast::Receiver<Arc<Bytes>> + direct replies (mpsc)
    while let Some(Ok(Message::Binary(b))) = rx.next().await {
        let env: Envelope = postcard::from_bytes(&b)?;   // versioned envelope
        /* … */
    }
}
// send: tx.send(Message::Binary(postcard::to_allocvec(&msg)?.into())).await
```

Serve the router with `axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())` so handlers can see the peer address.

**Fan-out.** Use `tokio::sync::broadcast` per page, or one global channel filtered by subscription. Encode once and share `Arc<Bytes>` so every socket gets the same bytes without re-encoding. If a receiver returns `RecvError::Lagged`, send that client a `Resync{from_seq}` and let it pull from the op log. `StrokeDelta`s are ephemeral and may be dropped.

**Auth for WebSockets.** Browsers can't set an `Authorization` header on `new WebSocket()`. Options: put the token in the first frame (recommended, same path for tablet and browser), use a query string (it leaks into logs), or use `Sec-WebSocket-Protocol` (a hack). For the browser, also check the `Origin` header on upgrade to block cross-site WebSocket hijacking.

**tower middleware.** `tower-http` 0.7.1 still has `ValidateRequestHeaderLayer::bearer`, but it has been **deprecated since 0.6.7** ("too basic to be useful in real applications") ([docs.rs](https://docs.rs/tower-http/0.7.1/tower_http/validate_request/struct.ValidateRequestHeaderLayer.html)). Write a `from_fn_with_state` middleware instead:

```rust
async fn require_bearer(State(t): State<Tokens>, req: Request, next: Next) -> Result<Response, StatusCode> {
    let tok = req.headers().get(AUTHORIZATION).and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?;
    let h: [u8; 32] = sha2::Sha256::digest(tok.as_bytes()).into();       // DB stores only hashes
    if t.any(|stored| stored.ct_eq(&h).into()) { Ok(next.run(req).await) } else { Err(StatusCode::UNAUTHORIZED) }
}
```

Generate tokens with `getrandom::fill(&mut [0u8; 32])` and encode them as base64url (`URL_SAFE_NO_PAD`). Keep `tower-http` for `trace`, `limit` (`RequestBodyLimitLayer`) and `cors` (web viewer only).

**Recommendation:** use axum 0.8 `ws` with binary postcard frames, auth in the first frame (`Hello`), and `broadcast` fan-out with lag → resync. Write a custom `from_fn_with_state` bearer middleware using a SHA-256 hash plus a `subtle` constant-time compare for HTTP/MCP. Don't use the deprecated tower-http bearer layer.

---

## 3. SQLite: rusqlite vs sqlx, migrations, WAL, op log

| | rusqlite 0.40/0.41 | sqlx 0.9.0 |
|---|---|---|
| Model | sync, thin over the C API | async facade; SQLite runs on a worker thread underneath |
| Bundled SQLite | `bundled` feature → 3.54.0 ([libsqlite3-sys](https://github.com/rusqlite/rusqlite/tree/master/libsqlite3-sys)) | also via libsqlite3-sys |
| Compile-time query checks | no | `query!` needs `DATABASE_URL` or offline `.sqlx` cache |
| Deps / build weight | small | large (macros, any-driver plumbing) |
| License | MIT | MIT OR Apache-2.0 |

For a single-process, single-writer append-only log, rusqlite is simpler and faster. Async adds nothing here because SQLite is blocking anyway. Run it on **one dedicated writer thread** (an mpsc of commands, or `tokio::task::spawn_blocking`) with a small pool of read-only connections. `tokio-rusqlite` 0.8.0 (MIT) wraps exactly that pattern if we want it ([crates.io](https://crates.io/crates/tokio-rusqlite)).

**Migrations.** `rusqlite_migration` 2.6.0 (Apache-2.0) stores the schema version in `PRAGMA user_version` with no extra table. Migrations are a `&[M]` const of SQL strings or `include_str!` files, and there's `MIGRATIONS.validate()` for tests ([docs.rs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/)). `refinery` 0.10 (MIT) is the heavier alternative and isn't needed.

**Pitfall: version skew.** rusqlite **0.41.0 was released today**, but `rusqlite_migration` 2.6.0, `tokio-rusqlite` 0.8.0 and `serde_rusqlite` 0.43 all require `rusqlite ^0.40`. Pin `rusqlite = "0.40"` until they catch up, or Cargo will link two `libsqlite3-sys` and fail on the `links` key.

**Pragmas** (set on every connection, outside migrations):
```sql
PRAGMA journal_mode = WAL;        -- persistent; readers don't block the writer
PRAGMA synchronous = NORMAL;      -- in WAL: durable across app crash; last txns may roll back on power loss
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```
([SQLite WAL](https://www.sqlite.org/wal.html), [pragma](https://www.sqlite.org/pragma.html)). For backups, use `VACUUM INTO` or rusqlite's `backup` feature rather than copying the file while WAL is active.

**Op-log schema sketch:**
```sql
CREATE TABLE op_log (
  seq        INTEGER PRIMARY KEY AUTOINCREMENT,  -- server-assigned, monotonic, never reused
  op_id      BLOB NOT NULL UNIQUE,               -- 16-byte ULID → idempotent outbox replay (INSERT … ON CONFLICT DO NOTHING)
  device_id  BLOB NOT NULL,
  hlc        INTEGER NOT NULL,                   -- packed u64 (see §4)
  page_id    BLOB,
  kind       INTEGER NOT NULL,
  payload    BLOB NOT NULL                       -- postcard-encoded op payload
) STRICT;
CREATE INDEX op_log_page_seq ON op_log(page_id, seq);
-- materialized state, updated in the same transaction as the append:
CREATE TABLE items (id BLOB PRIMARY KEY, page_id BLOB NOT NULL, layer INTEGER, z INTEGER,
                    deleted INTEGER NOT NULL, field_hlc BLOB NOT NULL, data BLOB NOT NULL) STRICT;
```
`seq` is what clients track as `last_seen_seq`. Keep ULIDs as `BLOB(16)` (`Ulid::to_bytes`), not text: half the size, and they still sort correctly.

**Recommendation:** use `rusqlite` 0.40 with `bundled` + `rusqlite_migration` 2.6. Run WAL with `synchronous=NORMAL`, one writer thread, and append to an `op_log` keyed by `seq AUTOINCREMENT` with a unique `op_id` for idempotency. Store payloads as postcard BLOBs and maintain an `items` projection in the same transaction.

---

## 4. Serialization, ULID, HLC

**postcard 1.1.3** (MIT OR Apache-2.0). Since 1.0 the wire format is **stable and specified**: changing it would require postcard 2.0 plus a new spec ([docs.rs](https://docs.rs/postcard), [spec](https://postcard.jamesmunns.com/wire-format.html)). No 2.x pre-release exists on crates.io. Encoding rules:
- integers wider than 8 bits are varints (zigzag for signed); `f32`/`f64` are fixed little-endian, so encoding is deterministic.
- enum discriminants and sequence lengths are varints, and the format is **not self-describing**.
- `no_std` + `alloc` friendly, so the same crate works in core, wasm and Android.

Versioning: a postcard format has no field tags. Adding a field to a struct breaks old decoders, and appending an enum variant only works forward (old readers fail on the new variant). Wrap everything in `Envelope { proto: u16, msg: Msg }`, append new `Msg`/`OpPayload` variants only at the end, and handle new structs by adding variants (`AddItemV2`), never by changing existing ones. Stored `payload` blobs need a `payload_version` (in `kind` or the first byte). For delta-encoded stroke samples, encode the deltas as `i32` and postcard's zigzag varints compress them automatically.

**bincode: do not use.** `bincode` 3.0.0 (2025-12-16) contains only a `compile_error!`. The project is **unmaintained** after a harassment incident ([readme](https://crates.io/crates/bincode/3.0.0)). The 2.x line is frozen. Its README lists `wincode` (Apache-2.0), postcard and rkyv as alternatives.
**rkyv** 0.8.18 (MIT) gives zero-copy reads, but its format is tied to layout and alignment, and the extra complexity isn't worth it for 4 KB frames. **Hand-rolled** encoding would only be worth it for the stroke sample blob. Postcard over `Vec<i32>` deltas is close to optimal already.

**ULID: `ulid` 3.0.0** (MIT). API: `Ulid::generate()` (3.x name), `from_parts(ms, rand)`, `to_bytes()/from_bytes()`, `timestamp_ms()`, and `Generator` for monotonic ids within the same millisecond ([docs.rs](https://docs.rs/ulid/3.0.0/ulid/)). Its `serde` feature serializes as a string, or as `u128` via `ulid::serde::ulid_as_u128`, and a postcard `u128` varint takes up to 19 bytes. Instead, wrap it in a newtype `ItemId([u8; 16])` with serde as bytes (`serde_bytes`, or a tuple of 16 bytes = 16 bytes fixed). For determinism, `penwise-core`/`proto` should **not** call `generate()`, since that's clock and RNG I/O. Have the caller inject `from_parts(now_ms, rand80)`. Turn off default features in core so `std`/`rand` aren't pulled in.

**HLC: hand-roll it (about 50 lines in `penwise-proto`).** Crate options: `uhlc` 0.9 (EPL-2.0 OR Apache-2.0, usable under Apache) is built around NTP64 timestamps and a 128-bit ID, which is overkill. `hybrid-logical-clock` 0.0.2 (MIT) has 10 recent downloads. A minimal pure HLC:
```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc { pub wall_ms: u64 /*48 bits used*/, pub counter: u16 }  // pack: wall_ms<<16 | counter → u64
impl Hlc {
    pub fn tick(self, now_ms: u64) -> Hlc { if now_ms > self.wall_ms { Hlc{wall_ms: now_ms, counter: 0} } else { Hlc{wall_ms: self.wall_ms, counter: self.counter + 1} } }
    pub fn recv(self, remote: Hlc, now_ms: u64) -> Hlc { let w = now_ms.max(self.wall_ms).max(remote.wall_ms);
        let c = if w == self.wall_ms && w == remote.wall_ms { self.counter.max(remote.counter) + 1 }
                else if w == self.wall_ms { self.counter + 1 } else if w == remote.wall_ms { remote.counter + 1 } else { 0 }; Hlc{wall_ms: w, counter: c} }
}
// LWW order: (hlc, device_id) — device_id breaks ties deterministically.
```
Add guards for counter overflow (saturate, or wait for the next ms) and for remote clocks far in the future (reject skew > N minutes). Algorithm: [Kulkarni et al., "Logical Physical Clocks"](https://cse.buffalo.edu/tech-reports/2014-04.pdf).

**Recommendation:** use postcard 1.x for both the wire and stored payloads, with a versioned envelope and append-only enum evolution. Use `ulid` 3 for generation at the edges, with ids stored as `[u8;16]`. Hand-roll the HLC packed into a `u64`, with LWW ties broken by `device_id`. Never use bincode.

---

## 5. mDNS, Android NSD, binding LAN + Tailscale

**Responder: `mdns-sd` 0.21.6** (Apache-2.0 OR MIT). It's pure Rust and runs one daemon thread with a flume channel, so it doesn't depend on any runtime ([docs.rs](https://docs.rs/mdns-sd/latest/mdns_sd/)).
```rust
let mdns = ServiceDaemon::new()?;
mdns.disable_interface(vec![IfKind::Name("tailscale0".into()), IfKind::Name("docker0".into()), IfKind::LoopbackV4, IfKind::IPv6])?; // [UNVERIFIED exact arg type: takes `impl IntoIfKindVec`]
let info = ServiceInfo::new("_penwise._tcp.local.", "Penwise on deskpc", "deskpc.local.", "", 7420,
                            &[("v", "1"), ("fp", fp_hex8)][..])?.enable_addr_auto();   // addrs follow interface changes
mdns.register(info)?;
let events = mdns.monitor()?;  // watch DaemonEvent::Error / NameChange
```
- The service-name label (`penwise`) must be ≤ 15 chars (`SERVICE_NAME_LEN_MAX_DEFAULT`).
- This PC has `docker0`, many `br-*` bridges and `tailscale0` (seen on this host). Disable everything except the physical LAN NIC, or the advertisement will include 172.x addresses the tablet can't reach. Tailscale doesn't carry multicast, so mDNS over `tailscale0` is useless.
- Never put the pairing token in TXT records: they're visible to the whole LAN. A short cert or server fingerprint and the protocol version are fine.
- The host firewall must allow UDP 5353 in and the service TCP port. mdns-sd can share 5353 with avahi-daemon. [UNVERIFIED on this host]

**Android NSD interop** (tablet on API 36):
- `nsd.discoverServices("_penwise._tcp", NsdManager.PROTOCOL_DNS_SD, listener)`. Note: no `.local.` suffix.
- `resolveService` is **deprecated since API 34**. Use `registerServiceInfoCallback(serviceInfo, executor, cb)`, available at API 34. `onServiceUpdated` gives `hostAddresses` (`getHost()` is also deprecated) plus `attributes: Map<String, ByteArray>` for TXT ([NsdManager](https://developer.android.com/reference/android/net/nsd/NsdManager), [NsdServiceInfo](https://developer.android.com/reference/kotlin/android/net/nsd/NsdServiceInfo)). The `DiscoveryRequest` overload needs API 37 or T-ext 22.
- Local network permission: with targetSdk ≤ 36, `INTERNET` grants local access implicitly. On Android 17 with targetSdk ≥ 37, `ACCESS_LOCAL_NETWORK` is required at runtime, or use `DiscoveryRequest.FLAG_SHOW_PICKER` ([local-network-permission](https://developer.android.com/privacy-and-security/local-network-permission)). Declare it now so the eventual targetSdk bump isn't a surprise.

**Binding.** Use two listeners, which also gives MCP the separate exposure PLAN.md calls for:
1. **Sync/web** (`:7420`). Bind `0.0.0.0` (plus `[::]` if wanted). It survives DHCP changes and Tailscale coming up late. Everything on it is token-gated, **except** the pairing/QR page, which must be served only when `ConnectInfo` peer `is_loopback()`. Binding specific addresses instead means a rescan/rebind loop whenever interfaces change; the alternative is filtering inbound connections by `peer.ip()` (allow RFC1918 + 100.64.0.0/10 + loopback).
2. **MCP** (`:7421`). Bind exactly `127.0.0.1` plus the Tailscale IPv4. `TcpListener::bind` on a 100.x address fails if tailscaled isn't up yet, so retry with backoff. Run one `axum::serve` per `TcpListener`, cloning the same `Router` for each.

Use `if-addrs` 0.15 (MIT OR BSD-3-Clause, already a dependency of mdns-sd) to enumerate LAN addresses for the QR payload. Pick the interface holding the default route, or a private IPv4 that's not on `docker*`, `br-*` or `veth*`.

**Discovering the Tailscale identity.** Both options return the same `ipnstate.Status` JSON. Verified on this host (tailscale 1.102.4):
- CLI: `tailscale status --json` → `.Self.DNSName` = `"deskpc.tail000000.ts.net."` (strip the trailing dot), `.Self.TailscaleIPs` = `["100.64.10.100","fd7a:…"]`, `.MagicDNSSuffix`, `.BackendState == "Running"`. The docs warn "format subject to change" ([CLI docs](https://tailscale.com/docs/reference/tailscale-cli)).
- LocalAPI: `GET http://local-tailscaled.sock/localapi/v0/status?peers=false` over the unix socket `/var/run/tailscale/tailscaled.sock` (mode 0666 here, no token). The `Host:` header **must** be `local-tailscaled.sock` or it returns 403. The API is not stable unless marked otherwise ([tailscale.com/client/local](https://pkg.go.dev/tailscale.com/client/local)). From Rust this needs a hyper unix-socket client.

Use `tailscale status --json` via `tokio::process::Command`, deserialize only `Self.{DNSName,TailscaleIPs}`, `BackendState` and `CurrentTailnet.MagicDNSEnabled` (with `#[serde(default)]` everywhere), and re-poll every 60 s. If the binary is missing or the backend isn't running, skip Tailscale. The tablet reaches the server over `deskpc.tail000000.ts.net`, and traffic is WireGuard-encrypted. The LAN path is **plaintext ws://**, so the `server_pubkey_fp` in the QR implies a TLS or Noise layer on LAN. That's a security-review decision and out of scope here.

**Recommendation:** run `mdns-sd` advertising `_penwise._tcp` only on the physical LAN interface with `enable_addr_auto`, and resolve on the tablet with `NsdManager` + `registerServiceInfoCallback`. Sync binds `0.0.0.0` with token auth and a loopback-only pairing page. MCP binds loopback + Tailscale IP. Get the Tailscale identity by polling `tailscale status --json`.

---

## 6. QR codes and embedded web assets

**`qrcode` 0.14.1** (MIT OR Apache-2.0). It was last released in 2024 but is stable and widely used (6.4 M recent downloads). Default features pull in `image`, so turn them off:
```toml
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
```
```rust
let svg: String = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M)?
    .render::<qrcode::render::svg::Color>().min_dimensions(320, 320).quiet_zone(true).build();
```
Payload: use a URI so a generic camera app can also deep-link, e.g. `penwise://pair?v=1&lan=192.168.1.19:7420&ts=deskpc.tail000000.ts.net&p=7420&t=<token>&fp=<hex>`. Keep it under ~150 bytes so the QR stays at a low version that's easy to scan from a tablet camera. Make the token single-use and short-lived (it's exchanged for a per-device token on first `Hello`), and regenerate it every time the page loads.

**`rust-embed` 8.13.0** (MIT):
```rust
#[derive(rust_embed::Embed)] #[folder = "../../web/dist/"] struct Assets;
async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/'); let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(f) => ([(CONTENT_TYPE, f.metadata.mimetype()), (ETAG, hex(f.metadata.sha256_hash()))], f.data).into_response(),
        None => Assets::get("index.html").map(|f| Html(f.data).into_response()).unwrap_or(StatusCode::NOT_FOUND.into_response()),
    }
}
```
- Enable features `mime-guess` (for `metadata.mimetype()`) and optionally `compression` (stores the files compressed via `include-flate`). The `axum-ex` feature only exists for an example. `axum-embed` 0.1.0 has been dormant since 2023, so skip it.
- In debug builds rust-embed **reads files from disk** at runtime unless the `debug-embed` feature is set. That's convenient for viewer development but a pitfall for "single binary" tests. The `folder` must exist at compile time, so the build needs `web/dist` (and the wasm) built first, or a `build.rs` that creates an empty placeholder.
- Serve `.wasm` as `application/wasm` (mime_guess does this) so `WebAssembly.instantiateStreaming` works.

**Recommendation:** generate the pairing QR as SVG with `qrcode` (svg-only, ECC M), embedding a short-lived token URI, on a loopback-only page. Serve the viewer with `rust-embed` (`mime-guess`, `compression`) through a single fallback handler with ETag.

---

## 7. Local vision models for handwriting OCR (Ollama)

Local hardware (seen on this host): **RTX 3060 12 GB + RTX 4060 8 GB**, Ollama client 0.34.3 (no daemon running, no models pulled).

| Model (Ollama tag) | Size (download) | License | Handwriting notes |
|---|---|---|---|
| `qwen3-vl:8b` (also 2b/4b/30b-a3b/32b) | 6.2 GB (8b) | Apache-2.0 | Top open-weights model on the OCR Arena leaderboard per [Autodidacts](https://www.autodidacts.io/usable-local-ai-handwriting-recognition/), with "acceptable WER even on cryptic handwriting". Supports grounding (bbox) output. |
| `qwen3.5:9b` (vision-capable; 4b/27b too) | 6.6 GB | Apache-2.0 [UNVERIFIED] | Newer general VLM, listed with the "vision" + "OCR" tags on [ollama.com/library/qwen3.5](https://ollama.com/library/qwen3.5). Grouped with Qwen3-VL as the doc-OCR leaders by [PromptQuorum](https://www.promptquorum.com/power-local-llm/local-vision-models-llava-ollama-2026). |
| `gemma4` (e2b/e4b/12b/26b/31b) | 4.6–20 GB | Gemma/Apache [UNVERIFIED] | Block-letter handwriting "moderate accuracy", cursive unreliable ([PromptQuorum](https://www.promptquorum.com/power-local-llm/local-vision-models-llava-ollama-2026)). |
| olmOCR-2-7B-1025 (community `richardyoung/olmocr2`) | ~5–8 GB quantized [UNVERIFIED] | Apache-2.0 | Qwen2.5-VL-7B fine-tuned with RLVR. olmOCR-Bench 82.4, the only open model in the top 3 on AIMultiple's cursive benchmark ([LlamaIndex](https://www.llamaindex.ai/insights/best-ai-for-handwriting-recognition)). It's prompt-specific (trained on a page-to-Markdown prompt), and the community Ollama packaging hasn't been verified. |
| `glm-ocr` (0.9B) | 1.8–2.7 GB | [UNVERIFIED] | Small dedicated OCR model. Fast, but tuned for print and documents. |
| `deepseek-ocr` (3B) | 6.7 GB | MIT [UNVERIFIED] | Document OCR. Handwriting isn't its strength. [UNVERIFIED] |
| `minicpm-v` (8B) / MiniCPM-V 4.6–4.7 | 5.5 GB | MiniCPM licence (non-OSI) [UNVERIFIED] | Mixed handwriting results: "garbled text across multiple lines" in [MindStudio tests](https://www.mindstudio.ai/blog/minicpm-v-4-7-1b-benchmark-test). |

Consensus across 2026 write-ups: print and block handwriting is usable locally, while cursive is still weak on every local model ([PromptQuorum](https://www.promptquorum.com/power-local-llm/local-vision-models-llava-ollama-2026)). Model weights are runtime data, not linked dependencies, so the AGENTS.md license rule doesn't strictly apply. Still prefer Apache-2.0 weights (Qwen, olmOCR).

**Ollama HTTP API** ([api.md](https://github.com/ollama/ollama/blob/main/docs/api.md)). Images are base64 strings with no `data:` prefix:
```json
POST http://127.0.0.1:11434/api/chat
{ "model": "qwen3-vl:8b", "stream": false, "keep_alive": "10m",
  "options": { "temperature": 0, "num_ctx": 8192 },
  "format": { "type": "object", "properties": { "lines": { "type": "array", "items": {
      "type": "object", "properties": { "text": {"type":"string"},
      "bbox": {"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4} },
      "required": ["text","bbox"] } } }, "required": ["lines"] },
  "messages": [{ "role": "user", "content": "Transcribe the handwriting line by line. bbox = [x0,y0,x1,y1] in 0-1000 image coords.",
                 "images": ["iVBORw0KGgo…"] }] }
```
- `/api/generate` takes the same top-level `images: [...]` field. `format` accepts `"json"` or a JSON Schema (structured outputs). `keep_alive: 0` unloads the model.
- Qwen-VL grounding uses coordinates relative to 0–1000. Map them back to page coordinates using the render transform, then link each text item to the strokes whose bboxes intersect it. Do this deterministically on the server, not with the model. [UNVERIFIED that Ollama's Qwen3-VL template keeps grounding accuracy under a JSON schema]
- Render input: tight crop of the inked region, dark ink on white, around 1024–1600 px on the long side. Thinking-capable models (qwen3-vl, qwen3.5, gemma4) add latency, so turn thinking off (`"think": false`) for scans.
- Two GPUs: Ollama spreads layers across both. An 8B model fits on the 3060 alone. Set `CUDA_VISIBLE_DEVICES` if the 4060 is busy with display.
- Client: `reqwest` 0.13 with `json`, or a ~40-line hyper client. Skip the `ollama-rs` crate, since one endpoint doesn't justify it.

**Recommendation:** default the `Recognizer` Ollama backend to **`qwen3-vl:8b`** (Apache-2.0, 6 GB, fits the 3060, grounding bboxes) with a JSON-schema `format`, `temperature 0` and thinking off. In M6, A/B it on the real pen fixtures against **olmOCR-2** (handwriting/cursive specialist), **`qwen3.5:9b`** and **`gemma4:12b`**. Pick by character error rate and latency, and keep the model name a config value.
