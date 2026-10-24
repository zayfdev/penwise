# Penwise — Codebase Rules

Rules for humans and agents. Tooling enforces the mechanical ones (`just check`); a change that
fails `just check` is not done.

## Structure

- Crate boundaries are fixed: `penwise-core` ← `penwise-proto` ← (`penwise-server`, `penwise-ffi`, `penwise-wasm`).
  Arrows point at dependencies; nothing depends upward.
- `penwise-core` is pure: no I/O, no async, no networking, no clocks, no logging. Input samples in, geometry/pixels out.
- `penwise-ffi` and `penwise-wasm` are thin binding layers: type conversion only, zero logic.
- Modules are named for a domain concept (`smoothing`, `outline`, `codec`, `pairing`), never `utils`, `helpers`,
  `common`, `misc`, `manager`.
- One concept per file. When a file needs a section-divider comment, split it.

## Size limits

| Limit | Soft (review) | Hard (check fails) | Enforced by |
|---|---|---|---|
| Lines per source file, tests excluded | 300 | 500 | `scripts/check-sizes.sh` |
| Lines per function | 50 | 80 | clippy `too_many_lines`, detekt `LongMethod` |
| Function parameters | 5 | 7 (use a struct) | clippy `too_many_arguments`, detekt `LongParameterList` |

Rust unit tests live in a sibling `tests.rs` (`#[cfg(test)] mod tests;`) or in `tests/`, so they never push a
source file over the limit. Generated code (`uniffi`, `wasm-bindgen` output) is excluded and never edited by hand.

## Rust

- Edition 2024, toolchain pinned in `rust-toolchain.toml`; dependency versions declared once in the workspace `Cargo.toml`.
- Unsafe code is forbidden workspace-wide (`[workspace.lints]`); only `penwise-ffi` opts out with its own `[lints]`.
- `cargo clippy --all-targets -- -D warnings` with `clippy::pedantic` on; workspace denies `unwrap_used`, `expect_used`,
  `panic`, `todo`, `dbg_macro`, `print_stdout`/`print_stderr` (allowed in tests via `clippy.toml`).
- Errors: `thiserror` enums in libraries; `anyhow` only in `penwise-server`'s `main`. No stringly-typed errors.
- Logging: `tracing` only, in server code.
- Hot paths (per pen sample): no allocation; reuse buffers owned by the caller or the stroke builder.
- **Determinism:** `penwise-core` output must be bit-identical on native and WebAssembly. Use `libm` for
  transcendental math, never `f32::sin`/`powf` from std; no `HashMap` iteration order in output; golden tests run on both targets.

## Kotlin (android/)

- Packages by feature (`ink`, `sync`, `pairing`, `notebooks`), not by layer type.
- No logic in Activities/Composables beyond wiring UI to state; state lives in ViewModels / plain classes.
- No `!!`; coroutines with structured concurrency, no `GlobalScope`.
- ktlint + detekt clean; Android Lint warnings treated as errors.

## TypeScript (web/)

- `strict` on; no `any`, no non-null `!`; Biome for lint + format.
- Rendering goes through `penwise-wasm`; the viewer never re-implements stroke geometry.

## Wire format & storage

- Every wire message and stored blob carries a version. Changing `penwise-proto` requires a compat test that decodes the
  previous version's fixtures.
- SQLite schema changes go through numbered migrations; never edit an applied migration.

## Testing

- Golden fixtures in `fixtures/` are real pen recordings from the Tab S9 (raw samples incl. pressure/tilt/timing).
  Core changes that move pixels must update goldens deliberately, with before/after images in the PR.
- Every bug fix lands with a test that failed before the fix.
- Test behavior (outputs, round-trips, sync convergence, MCP tool results), not implementation details or mocks echoing input.
- Sync logic gets convergence tests: shuffled/duplicated/delayed op delivery must produce identical state.

## Verification gates

- `just check`: fmt, clippy, cargo tests (native + wasm), size check, `cargo deny` (licenses), web typecheck/lint/test,
  Android lint + unit tests.
- `just device-check`: build + install debug APK, replay fixtures through the debug input harness, screencap, compare
  against goldens, verify the stroke arrived at the server and in the browser view.
- Run heavy builds through `batch` (see host guidance).

## Dependencies

- Licenses: MIT, Apache-2.0, BSD, ISC, Zlib only (enforced by `cargo deny` / license checks). No tldraw.
- Add a dependency only when it replaces meaningful code; note the reason in the PR.

## Commits

- Small, single-purpose, conventional prefixes (`core:`, `server:`, `android:`, `web:`, `proto:`).
