# Full gate: a change is done only when this passes.
default: check

check: fmt-check clippy test wasm-build wasm-test sizes deny

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

android:
    cd android && ./gradlew :app:assembleDebug

android-check:
    cd android && ./gradlew :app:lintDebug ktlintCheck detekt :app:testDebugUnitTest

install:
    cd android && ./gradlew :app:installDebug

# libpenwise_ffi.so for Android (Gradle runs the same build automatically) → target/jniLibs/arm64-v8a/.
ffi:
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.16248370" cargo ndk -t arm64-v8a -P 34 -o target/jniLibs build --release -p penwise-ffi

# Browser viewer bundle (wasm) — rebuild after changing core or penwise-wasm.
web:
    wasm-pack build crates/penwise-wasm --target web --out-dir ../../web/pkg --out-name penwise_wasm

# PC mirror server on :7420 (PENWISE_PORT, PENWISE_WEB, PENWISE_DATA).
serve:
    cargo run -p penwise-server
