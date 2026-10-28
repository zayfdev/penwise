# Full gate: a change is done only when this passes.
default: check

check: fmt-check clippy test wasm-build wasm-test jni-symbols web-check sizes deny

fmt-check:
    cargo fmt --all --check

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace --all-features

wasm-build:
    cargo build -p penwise-core --target wasm32-unknown-unknown

# Core geometry goldens on wasm32 under Node (core-determinism.md §4).
wasm-test:
    wasm-pack test --node --release crates/penwise-core
    wasm-pack test --node --release crates/penwise-wasm

# Every JNI export in penwise-ffi matches an `external fun` in Native.kt (new exports are reported).
jni-symbols:
    scripts/check-jni-symbols.sh

sizes:
    scripts/check-sizes.sh

deny:
    cargo deny check licenses bans sources

record:
    scripts/record-pen.sh

# Raw pen captures → fixtures/pwrec/*.pwrec (golden test inputs).
fixtures:
    cargo run -p penwise-fixtures -- convert fixtures/raw/20261024-094722 fixtures/pwrec

# Rewrite geometry (and raster) goldens natively; then `just wasm-test` proves them cross-target.
bless:
    PENWISE_BLESS=1 cargo test -p penwise-core --features raster

# Android (android/): the SDK comes from ANDROID_HOME, defaulting to ~/Android/Sdk.
export ANDROID_HOME := env_var_or_default("ANDROID_HOME", home_directory() / "Android/Sdk")

# ANDROID_HOME defaults to the usual SDK location so a bare `just android` works.
android:
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} ./gradlew :app:assembleDebug

android-check:
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} ./gradlew :app:lintDebug ktlintCheck detekt :app:testDebugUnitTest

install:
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} ./gradlew :app:installDebug

# Build, install and smoke-check the tablet: crash buffer, page load, local files, sync, screenshot.
device-check:
    scripts/device-check.sh

# libpenwise_ffi.so for Android (Gradle runs the same build automatically) → target/jniLibs/arm64-v8a/.
ffi:
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.16248370" cargo ndk -t arm64-v8a -P 34 -o target/jniLibs build --release -p penwise-ffi

# Chrome extension flow against the shipped server client (Bun, fake Penwise server).
extension-check:
    bun test extension

# Components from a running page onto one Penwise page (needs Chrome/Chromium; CHROME= overrides).
grab *ARGS:
    bun scripts/grab.mjs {{ARGS}}

# Browser viewer bundle (wasm) — rebuild after changing core or penwise-wasm.
web:
    cd web && npm run wasm && npm run build

# Browser viewer: types, lint, unit tests (vitest against the real wasm).
web-check:
    cd web && npm install --package-lock=false --no-audit --no-fund && npm run wasm && npm run typecheck && npm run lint && npm test

# Browser end to end: real server binary + headless Chromium.
web-e2e:
    cargo build -p penwise-server && cd web && npm run wasm && npm run e2e

# PC mirror server on :7420 (PENWISE_PORT, PENWISE_WEB, PENWISE_DATA).
serve: web
    cargo run -p penwise-server

# This PC: release server as a login service + "Penwise" app launcher (scripts/install-desktop.sh).
install-desktop:
    scripts/install-desktop.sh

# MCP (stdio) server for agents: reads the page from the running server.
mcp:
    cargo run -q -p penwise-mcp
