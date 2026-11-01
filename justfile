# Full gate: a change is done only when this passes.
default: check

# Heavy gates take one machine-wide lock, so parallel agents run them one at a time (CPU, RAM and the GPU
# driver: concurrent browser-test Chromiums froze the desktop on 2026-10-28).
gate_lock := "flock /tmp/penwise-gate.lock"

check:
    {{gate_lock}} just _check

_check: fmt-check clippy test wasm-build wasm-test jni-symbols web-check adapters-check sizes deny

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
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} {{gate_lock}} ./gradlew :app:assembleDebug

android-check:
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} {{gate_lock}} ./gradlew :app:lintDebug ktlintCheck detekt :app:testDebugUnitTest

install:
    cd android && ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} ./gradlew :app:installDebug

# Build, install and smoke-check the tablet: crash buffer, page load, local files, sync, screenshot.
device-check:
    scripts/device-check.sh

# On-device QA on the connected tablet: real numbers per behaviour, a pass/fail table, JSON report + screenshots in
# target/device-qa/<timestamp>/. `just device-qa` runs every group; `just device-qa scroll glue` runs those only.
# Groups: open scroll ink glue rotate lifecycle network reload fixtures round memory; --orient=both adds the other orientation. See docs/device-qa.md.
device-qa *GROUPS:
    flock /tmp/penwise-device.lock node scripts/device-qa/run.mjs {{GROUPS}}

# libpenwise_ffi.so for Android (Gradle runs the same build automatically) → target/jniLibs/arm64-v8a/.
ffi:
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.16248370" cargo ndk -t arm64-v8a -P 34 -o target/jniLibs build --release -p penwise-ffi

# Browser viewer bundle (wasm) — rebuild after changing core or penwise-wasm.
web:
    cd web && npm run wasm && npm run build

# Browser viewer: types, lint, unit tests (vitest against the real wasm).
web-check:
    cd web && npm install --package-lock=false --no-audit --no-fund && npm run wasm && npm run typecheck && npm run lint && npm test

# Browser end to end: real server binary + headless Chromium.
web-e2e:
    {{gate_lock}} sh -c 'cargo build -p penwise-server && cd web && npm run wasm && npm run e2e'

# Framework adapters (adapters/): transform fixtures per framework, plus a real Vite dev-server test.
adapters-check:
    cd adapters/vite && npm install --package-lock=false --no-audit --no-fund && npm test
    cd adapters/next && npm install --package-lock=false --no-audit --no-fund && npm test

# PC mirror server on :7420 (PENWISE_PORT, PENWISE_WEB, PENWISE_DATA).
serve: web
    cargo run -p penwise-server

# This PC: release server as a login service + "Penwise" app launcher (scripts/install-desktop.sh).
install-desktop:
    scripts/install-desktop.sh

# MCP (stdio) server for agents: reads the page from the running server.
mcp:
    cargo run -q -p penwise-mcp
