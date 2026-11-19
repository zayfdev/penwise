# Penwise

Penwise takes pen notes on an Android tablet and syncs them live to your PC. It also includes a design harness so you can draw on your running website and hand the marks to a coding agent.

## What it does

- Write and sketch on the tablet. One Rust ink engine computes stroke geometry on the tablet, in the browser, and on the server so every screen shows the same ink.
- See your pages live in a PC window or any browser on your network. Drawing works offline on the tablet and syncs when it reconnects.
- Link a web project with `penwise link` to draw on your live site in a frame on an infinite board.
- Tie each mark to the element under it, including its source file and line. A circle means look here, a cross means remove, and a wavy line means it feels off.
- Hand your marks to a coding agent over MCP using four tools (`look`, `show`, `resolve`, `notes`). The agent shows a proposal on the board and edits the code after you pick.

## Status

This is a personal project built in the open. It has only been built and tested on a Galaxy Tab S9 with a Staedtler Noris digital pen and an Arch Linux PC. Nothing else has been tried.

Today, `penwise link` writes agent setups for Claude Code and omp. Adapters that map elements to source files exist for Vite, SvelteKit, and Next.

See [docs/next-iteration.md](docs/next-iteration.md) for what doesn't work yet.

## Layout

| Path | What |
|---|---|
| `crates/penwise-core` | The ink engine. Pure Rust, no I/O |
| `crates/penwise-proto` | Sync and wire formats |
| `crates/penwise-server` | PC server for sync, storage, rendering, and the agent API |
| `crates/penwise-ffi`, `crates/penwise-wasm` | Thin bindings for Android and the browser |
| `crates/penwise-mcp`, `crates/penwise-cli` | The agent server and the `penwise` command |
| `android/` | The tablet app (Kotlin) |
| `web/` | The PC and browser viewer (TypeScript) |
| `adapters/` | Vite and Next plugins that add source locations |

## Run it

You need Rust (pinned in `rust-toolchain.toml`), Node, and [just](https://github.com/casey/just).

```bash
just check            # format, lint, tests, size limits
just web              # build the browser viewer
just serve            # PC server on port 7420
just install-desktop  # install the server as a login service, plus the CLI and the agent server
just android          # build the tablet app
```

Then link a project.

```bash
cd ~/src/my-site
penwise link
```

Read the owner guide in [docs/design-harness.md](docs/design-harness.md) and the agent tools guide in [docs/mcp.md](docs/mcp.md). Rules for working in this codebase are in [AGENTS.md](AGENTS.md).

## Security

The server uses one shared access token and plain HTTP. Run it on your own LAN or over Tailscale. Don't expose its ports to the internet.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option. Ported third-party code is listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).