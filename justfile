# Full gate: a change is done only when this passes.
default: check

check: fmt-check clippy test wasm-build sizes deny

fmt-check:
    cargo fmt --all --check

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

wasm-build:
    cargo build -p penwise-core --target wasm32-unknown-unknown

sizes:
    scripts/check-sizes.sh

deny:
    cargo deny check licenses bans sources
