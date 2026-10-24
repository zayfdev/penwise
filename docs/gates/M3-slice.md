# Gate record: M3 server + browser view (thin slice)

Branch `m3-server`. Verdict: **PASS for the slice**, full M3 design still open.

## What shipped

A working vertical slice of the M3 contract, not the whole contract:

- `crates/penwise-wasm`: builds a stroke's geometry in the browser through the same Rust engine (JSON out:
  outline points + contour ends, or 9-float dabs, plus colour/opacity/multiply).
- `crates/penwise-server`: axum server on port 7420. Serves the viewer from `web/`, accepts WebSocket sessions,
  holds the page in memory, persists it to `page.json`, and fans every change out to all connected viewers.
- `web/`: read-only live mirror (Canvas2D). Outlines fill with the nonzero rule; stamp dabs draw as radial-gradient
  ellipses; multiply strokes render in the under-ink band. No bundler: plain ES modules plus the wasm-pack output.
- Tablet side (`m4-android`): `SyncClient` pushes each finished stroke and rewrites the page on undo/clear/erase,
  queueing while offline and reconnecting every 3 s across two addresses (LAN, then Tailscale).

## Deviations from `docs/contracts/m3-server.md` (deliberate, for the slice)

| Contract | Slice | Why |
|---|---|---|
| postcard + HLC op log, LWW merge | JSON frames: `hello` / `stroke` / `page` | Fastest path to a visible mirror; the op log adds nothing until editing is shared |
| SQLite + migrations | one `page.json` | Same durability for a single device, no schema churn while the item model settles |
| pairing, tokens, mDNS, Tailscale discovery | bind `0.0.0.0:7420`, two fixed URLs in the app | Pairing matters once more than one device writes; recorded as an open security item |
| embedded web assets | served from `web/` on disk | Avoids build-order coupling; trivial to embed later |

Security state (updated): the LAN socket now requires a token — anyone on the network can read or write the page. Acceptable on a
trusted home LAN, **not** acceptable over Tailscale or on untrusted Wi-Fi; the token-in-QR pairing from the contract
must land before daily out-of-home use.

## Verification (evidence)

- `cargo fmt --check`, `cargo clippy -p penwise-server -p penwise-wasm -D warnings`: clean.
- Static routes: `/` 200 html, `/app.js` 200 js, `/pkg/*.js` 200 js, `*.wasm` 200 application/wasm, unknown 404,
  `../` traversal 404.
- Real Chromium against the running server: connected (`mirroring · 0 strokes`), a ballpoint stroke pushed over a
  second WebSocket appeared with pressure taper and a wavy centreline; a pencil stroke (stamp dabs) appeared with
  grain and taper. Screenshots in `/tmp/penwise-viewer*.png`.
- Persistence: `page.json` round-tripped (seed kept as a decimal string, 600 floats for 120 samples).
- Tablet: app connects from `192.168.1.3` to the server (`connected`), sending its page on launch.

Host firewall: `ufw` rules added for port 7420 from `192.168.1.0/24` and on `tailscale0`.

## Open

- Auth/pairing; Tailscale join on the tablet (it is not on the tailnet yet, so only the LAN URL works today).
- Live in-progress ink (viewer currently draws completed strokes only).
- PNG render endpoint and MCP tools (M5) are not built.
