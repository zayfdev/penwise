# Android stack research (2026-10-24)

Scope: the Penwise Kotlin app on a Galaxy Tab S9 (Android 16 / API 36), built from the CLI with the Gradle wrapper
on JDK 21. The SDK has `platforms;android-37.0`, build-tools 37.0.0 and NDK r30 (30.0.16248370). Nothing below
has been built yet. Versions were checked against Maven Central, Google Maven, the Gradle plugin portal and crates.io
on 2026-10-24. Anything unverified is marked [UNVERIFIED].

> **License blocker for Main (§3):** `uniffi`, `uniffi_core` and `uniffi_bindgen` 0.32.2 are **MPL-2.0**
> ([crates.io](https://crates.io/crates/uniffi)). `deny.toml` / AGENTS.md allow only MIT/Apache/BSD/ISC/Zlib/Unicode-3.0,
> so `cargo deny` will reject PLAN.md's choice as written. There are two ways forward: add a scoped MPL-2.0 exception
> for the `uniffi*` crates, or hand-write JNI with the `jni` crate (MIT OR Apache-2.0). §3 recommends one.

---

## 1. Toolchain versions and wrapper bootstrap

| Component | Latest (2026-10-24) | Constraint that matters |
|---|---|---|
| AGP | 9.4.1 (also 9.3.3) | API 37 needs AGP ≥ 9.1.1 ([about-agp](https://developer.android.com/build/releases/about-agp)). AGP 9.3 needs Gradle ≥ 9.5.0 and AGP 9.4 needs ≥ 9.6.0. JDK 17 minimum ([9.3](https://developer.android.com/build/releases/agp-9-3-0-release-notes), [9.4](https://developer.android.com/build/releases/agp-9-4-0-release-notes)) |
| Gradle | 9.8.2 (9.6.1 = 2026-06-26) | KGP 2.4.20–2.4.21 fully supports Gradle ≤ 9.7.0 and **AGP ≤ 9.3.1** ([Kotlin docs](https://kotlinlang.org/docs/gradle-configure-project.html)) |
| Kotlin (KGP) | 2.4.21 (2.4.20 was released 2026-09-07; [releases](https://kotlinlang.org/docs/releases.html)) | AGP 9 has [built-in Kotlin](https://developer.android.com/build/migrate-to-built-in-kotlin). Do **not** apply `org.jetbrains.kotlin.android`: AGP 9 errors with "no longer required" |
| Compose compiler plugin | `org.jetbrains.kotlin.plugin.compose` 2.4.21 | Its version must equal the KGP version |
| Compose BOM | `androidx.compose:compose-bom:2026.09.00` (UI 1.12.1, material3 1.4.0) | Compose 1.12 needs compileSdk 37 and AGP ≥ 9.2 ([Aug '26 release](https://android-developers.googleblog.com/2026/08/jetpack-compose-august-2026-release.html)) |
| detekt (constraint) | 2.0.0-alpha.6 was tested with Gradle 9.6.1, Kotlin 2.4.10 and AGP 9.3.1 | See §6 |

The pick: **AGP 9.3.3 + Gradle 9.6.1 + Kotlin 2.4.21**.
- AGP 9.3.x is the newest line that both KGP 2.4.2x and detekt test against.
- 9.3.2 fixed a lint crash on JDK 17. We run JDK 21 anyway.
- 9.3.3 fixed an R8 non-determinism bug ([9.3 notes](https://developer.android.com/build/releases/agp-9-3-0-release-notes)).
- AGP 9.4.1 would work too, but it falls outside KGP's "fully supported" range, so you may get deprecation noise.

Pitfalls:
- **Build-tools:** AGP 9.3 defaults to build-tools **36.0.0**, but only 37.0.0 is installed. Set
  `buildToolsVersion = "37.0.0"`, or AGP will try to download 36.0.0.
- **NDK:** AGP's default NDK is 28.2. Set `ndkVersion = "30.0.16248370"` so AGP can strip the Rust `.so` with the
  installed NDK. Otherwise you get the warning "Unable to strip library".
- **compileSdk:** compile against the 37.0 platform with the AGP 9 DSL: `compileSdk { version = release(37) }`, or
  `release(37) { minorApiLevel = 0 }` to pin the minor ([build config](https://developer.android.com/build)).
  Keep `targetSdk = 36`.
- **minSdk:** use **34**. The front-buffer renderer needs 29 and `NsdManager.registerServiceInfoCallback` needs 34,
  and there is only one device.
- **Not on PATH:** neither `gradle` nor `sdkmanager` is on PATH. Put `~/Android/Sdk/platform-tools` on PATH for `adb`,
  and write `sdk.dir` into `local.properties`, or set `ANDROID_HOME`.

### Bootstrapping the wrapper without Android Studio

There is no system Gradle, so download a distribution once, use it to generate the wrapper, then delete it:

```sh
cd /tmp && curl -LO https://services.gradle.org/distributions/gradle-9.6.1-bin.zip
echo "9c0f7faeeb306cb14e4279a3e084ca6b596894089a0638e68a07c945a32c9e14  gradle-9.6.1-bin.zip" | sha256sum -c
unzip -q gradle-9.6.1-bin.zip
cd ~/workspace/penwise/android && /tmp/gradle-9.6.1/bin/gradle wrapper \
  --gradle-version 9.6.1 --distribution-type bin \
  --gradle-distribution-sha256-sum 9c0f7faeeb306cb14e4279a3e084ca6b596894089a0638e68a07c945a32c9e14
```

This writes `gradlew`, `gradlew.bat` and `gradle/wrapper/gradle-wrapper.{jar,properties}`; commit all four
([Gradle wrapper docs](https://docs.gradle.org/current/userguide/gradle_wrapper.html)). The SHA-256 was fetched from
`services.gradle.org/.../gradle-9.6.1-bin.zip.sha256`. Use a `settings.gradle.kts` with `pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }`
and a `gradle/libs.versions.toml` version catalog.

Root `build.gradle.kts`:

```kotlin
plugins {
    id("com.android.application") version "9.3.3" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false // also pins KGP 2.4.21 on the classpath
}
```

The KGP-pinning mechanism here is [UNVERIFIED]. If AGP still resolves its bundled KGP 2.2.10, add
`id("org.jetbrains.kotlin.android") version "2.4.21" apply false`, which is the pattern
[about-agp](https://developer.android.com/build/releases/about-agp) shows.

**Recommendation:** AGP **9.3.3**, Gradle **9.6.1** (wrapper, `bin`, sha256-pinned), Kotlin/Compose plugin
**2.4.21**, Compose BOM **2026.09.00**, `compileSdk = release(37)`, `targetSdk = 36`, `minSdk = 34`,
`buildToolsVersion = "37.0.0"`, `ndkVersion = "30.0.16248370"`, JDK 21 (`jvmToolchain(21)` is optional).

---

## 2. Low-latency inking

**Libraries:**
- `androidx.graphics:graphics-core`: **1.0.4** stable; 1.1.0-beta01 (2026-10-07) adds `CanvasFrontBufferedRenderer.execute` and fixes inverted buffer transforms ([release notes](https://developer.android.com/jetpack/androidx/releases/graphics)).
- `androidx.input:input-motionprediction`: **1.0.0** stable since 2025-11-19, minSdk 23. Since beta06 it delegates to the system `MotionPredictor` when one exists ([release notes](https://developer.android.com/jetpack/androidx/releases/input)).

**Choosing the renderer.** Use `CanvasFrontBufferedRenderer<T>(surfaceView, callback)` ([API](https://developer.android.com/reference/kotlin/androidx/graphics/lowlatency/CanvasFrontBufferedRenderer)).
- It has two layers: a **front-buffered layer** (single buffer scanned out while you draw, so low latency but it can
  tear) and a **multi-buffered layer** (normal vsync latency, no tearing).
- `penwise-core` returns outline polygons, which map directly to `Path` + `Canvas.drawPath`.
- `GLFrontBufferedRenderer` (callbacks `onDrawFrontBufferedLayer(eglManager, bufferInfo, transform, param)` /
  `onDrawMultiBufferedLayer`) is only worth it if you need GL shaders, for example for particle brushes.

How the API works:
- `renderFrontBufferedLayer(param: T)` calls `Callback.onDrawFrontBufferedLayer(canvas, w, h, param)` on the renderer's
  internal thread.
- On **pen-up**, call `commit()`. It calls `onDrawMultiBufferedLayer(canvas, w, h, params)` with **every** `param`
  since the last commit, then atomically hides the front layer and shows the multi-buffered one.
- On `ACTION_CANCEL` (palm or navigation gesture), call `cancel()`. It hides the front layer and does not commit.
- To redraw the whole scene (resume, undo, remote ops), use `renderMultiBufferedLayer(collection)`. `clear()` wipes
  both layers.
- Call `release(true)` in `onDestroy`/`surfaceDestroyed`.

Pitfalls:
- **Incremental front layer:** draw only the new segment each callback. Front-buffer content persists until the next
  commit/cancel. That persistence is inferred from the commit semantics and Google's samples ([stylus guide](https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features)), and is not stated verbatim [UNVERIFIED].
  - Core smoothing makes the stroke tail unstable, and predicted points must be erased on the next frame. So keep a
    "stable prefix / unstable tail" split: before redrawing the tail, clip to the tail's bounds and clear it with
    `drawColor(TRANSPARENT, PorterDuff.Mode.CLEAR)`.
- **Threading:** callbacks run on the renderer thread. Make `T` an immutable snapshot (for example a per-frame
  segment index range into a preallocated buffer). Don't make it a mutable list the UI thread is writing to.
- **Size:** the front buffer is for small areas. Never repaint the full page through it ([guide](https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features)).
- **Buffer transform:** 1.0.4 has the inverted-buffer-transform bug for some rotations. Lock the canvas activity's
  orientation or use 1.1.0-beta01.

**Input path:**
- **Unbuffered dispatch.** `View.requestUnbufferedDispatch(event)` (API 21) applies to the current gesture only, so call
  it on every `ACTION_DOWN`. Alternatively call
  `requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_POINTER)` (API 30) once, which also covers hover and
  `ACTION_POINTER_DOWN` (the pen lands while a palm is already down).
  - Without it, `ACTION_MOVE` is batched per vsync (120 Hz), so our 482 Hz digitizer delivers about 4 samples per event.
- **Historical samples.** Always read them, unbuffered or not. Loop `h in 0 until historySize` over
  `getHistoricalX/Y/Pressure/AxisValue(axis, pointerIndex, h)` and `getHistoricalEventTime(h)` (oldest first), then
  read the current sample. Use real timestamps; don't assume even spacing ([chromeos/low-latency-stylus](https://github.com/chromeos/low-latency-stylus)).
  Never allocate per sample (AGENTS.md): write into a reusable `FloatArray`/direct `ByteBuffer`.
- **Prediction.** `MotionEventPredictor.newInstance(view)`. Call `record(event)` on every event; `predict()` returns a
  `MotionEvent?` that **you must `recycle()`**. Draw predicted points only on the front layer as the erasable tail.
  Never send them to core as real samples or into the op log.
  Whether Samsung enables the system predictor on the Tab S9 is [UNVERIFIED]; the library falls back to its own
  Kalman predictor.
- **Tool type and palm rejection.**
  - Draw only when `getToolType(pointerIndex) == TOOL_TYPE_STYLUS`. Treat `TOOL_TYPE_ERASER` the same way (our pen
    has no eraser end).
  - Track the pen by **pointer id**, not by index 0. A palm may be pointer 0.
  - Use fingers for pan/zoom.
  - Handle `ACTION_CANCEL`, and `FLAG_CANCELED` on `ACTION_POINTER_UP` (API 33+), by dropping that pointer's stroke.
  - The recordings show palm contacts on `sec_touchscreen` while the pen is near. Whether Android 16's dispatcher
    cancels the touch stream when the stylus (a different device) goes down needs testing on the device
    [UNVERIFIED].
- **Gesture navigation.** Set `systemBarsBehavior = BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE` in the canvas immersive
  mode so edge swipes don't become strokes.

**Axis semantics** ([AOSP touch-devices](https://source.android.com/docs/core/interaction/input/touch-devices), [stylus guide](https://developer.android.com/develop/ui/views/touch-and-input/stylus-input/advanced-stylus-features)):

| Axis | Meaning on this device |
|---|---|
| `AXIS_PRESSURE` | Raw 0–4095 normalized to about 0..1. Keep it as float; the core pressure curve works on 0..1 |
| `AXIS_TILT` | Angle from perpendicular, 0..π/2 rad. InputReader computes `tilt = acos(cos tx · cos ty)` from raw `ABS_TILT_X/Y` |
| `AXIS_ORIENTATION` | Azimuth −π..π rad, `atan2(-sin tx, sin ty)`. 0 = pointing "up", positive = clockwise |
| `AXIS_DISTANCE` | Hover distance, from raw 0–255 (we observed 0–~110). Only relative values are meaningful |
| raw tilt x/y | **Not exposed** in `MotionEvent`. `tx = (raw − center)·π/180`, so ±63 becomes ±63°. The map (tx,ty)→(tilt,orientation) is invertible for \|tx\|,\|ty\| < 90°, so no information is lost |

Feed core `(tilt, orientation)`. The browser can derive the same pair from `PointerEvent.altitudeAngle` (= π/2 − tilt)
and `azimuthAngle`; azimuth uses a different zero direction, so convert in `penwise-wasm`.
Two caveats:
- Orientation follows display rotation, because InputReader rotates it with the surface [UNVERIFIED for Samsung].
- Fixtures recorded via `getevent` hold raw tilt x/y. The recorder/replay harness must convert with the formulas above
  so fixture input matches what the live app produces.

**Hover:**
- Hover events arrive as `ACTION_HOVER_ENTER/MOVE/EXIT` through `onHoverEvent` / `onGenericMotionEvent`, not
  `onTouchEvent`, with tool type `STYLUS` and pressure 0.
- Use them for the brush-size cursor. Draw the cursor in a normal overlay view, not the front buffer.

**androidx.ink (reference only):**
- Status: 1.0.0 stable (announced Jan 2026), 1.1.0-alpha10 (2026-10-21), Apache-2.0 ([release notes](https://developer.android.com/jetpack/androidx/releases/ink)).
- `InProgressStrokesView` wraps front-buffering, prediction and unbuffered dispatch. Its brush/geometry engine
  duplicates `penwise-core`, and PLAN requires geometry that is bit-identical across tablet, server and wasm.
- Use its source as a reference for input handling and the "wet → dry" handoff. Don't use it as a dependency.

**Recommendation:** `androidx.graphics:graphics-core:1.0.4` (move to 1.1.0 when it goes stable),
`CanvasFrontBufferedRenderer` on a `SurfaceView` hosted via `AndroidView`, `androidx.input:input-motionprediction:1.0.0`,
`requestUnbufferedDispatch(SOURCE_CLASS_POINTER)`, historical samples always, stylus-only drawing by pointer id,
`commit()` on pen-up and `cancel()` on cancel. Pass tilt and orientation to core as radians.

---

## 3. Rust integration (penwise-ffi)

| Item | Version | License |
|---|---|---|
| `uniffi` / `uniffi_bindgen` | **0.32.2** (2026-09-22) | **MPL-2.0** ⚠ |
| JNA | `net.java.dev.jna:jna:5.19.1@aar` (2026-06-12) | LGPL-2.1-or-later **OR** Apache-2.0 (take Apache) |
| cargo-ndk | 4.1.2 (installed; the latest) | Apache-2.0/MIT |
| `jni` crate (alternative) | 0.22.4 | MIT OR Apache-2.0 |

**uniffi 0.32 notes** ([CHANGELOG](https://github.com/mozilla/uniffi-rs/blob/main/CHANGELOG.md)):
- Kotlin uses JNA **direct mapping** (since 0.30).
- 0.32.0 makes `&[u8]` arguments zero-copy: Kotlin must pass a **direct** `java.nio.ByteBuffer`.
- 0.32.0 changes `--config` to expect a *global* config file.
- 0.32.1 fixes an aarch64 checksum failure, so use ≥ 0.32.1.
- Writable `&mut [u8]` buffers and the experimental JNI-based Kotlin bindgen exist only on unreleased `main`.

**Bindgen as a workspace binary** ([tutorial](https://mozilla.github.io/uniffi-rs/latest/tutorial/foreign_language_bindings.html)):
crate `crates/uniffi-bindgen` depends on `uniffi = { version = "=0.32.2", features = ["cli"] }`, and its `main.rs` is
`fn main() { uniffi::uniffi_bindgen_main() }`.
- Pin the **same exact version** in `penwise-ffi`. A mismatch fails at load with a checksum/contract error.
- Use library mode against the Android `.so`. The metadata is architecture-agnostic, so no host build is needed:

```sh
cargo ndk -t arm64-v8a -P 34 -o android/app/build/rustJniLibs build --release -p penwise-ffi
cargo run -p uniffi-bindgen -- generate --library target/aarch64-linux-android/release/libpenwise_ffi.so \
  --language kotlin --out-dir android/app/build/generated/uniffi/kotlin
```

- `crates/penwise-ffi/uniffi.toml`: `[bindings.kotlin] package_name = "dev.penwise.ffi"` and `cdylib_name = "penwise_ffi"`.
- The output is `dev/penwise/ffi/penwise_ffi.kt` under `build/generated/...` and is never committed (PLAN.md).

**Gradle wiring:** use AGP's Variant API, not `dependsOn` on `merge*JniLibFolders` (task names are internal):

```kotlin
val cargoNdk = tasks.register<Exec>("cargoNdkArm64") {               // inputs: crates/**, Cargo.lock
    workingDir = rootDir.parentFile
    commandLine("cargo", "ndk", "-t", "arm64-v8a", "-P", "34", "-o", outDir.get().asFile.path,
                "build", "--release", "-p", "penwise-ffi")
    outputs.dir(outDir)                                               // outDir: layout.buildDirectory.dir("rustJniLibs")
}
androidComponents.onVariants { v ->
    v.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdkTask, CargoNdkTask::outDir)   // [UNVERIFIED: jniLibs]
    v.sources.kotlin?.addGeneratedSourceDirectory(bindgenTask, UniffiBindgenTask::outDir)
}
```

- `addGeneratedSourceDirectory` needs a typed task with a `DirectoryProperty` output, so write small abstract
  `CargoNdkTask` / `UniffiBindgenTask` classes in `buildSrc` that use `ExecOperations`.
- AGP then orders them before merge/compile automatically ([gradle-recipes](https://github.com/android/gradle-recipes/blob/agp-9.0/addGeneratedSourceFolder/README.md)).
- Declare `inputs.dir("../crates")` plus `Cargo.lock` so Gradle's up-to-date checks work. Cargo stays incremental anyway.
- Set `ndk { abiFilters += "arm64-v8a" }`. This also drops JNA's other-ABI `libjnidispatch.so`.
- Point cargo-ndk at r30 with `ANDROID_NDK_HOME`. NDK ≥ r28 links 16 KB-page-aligned by default.
- R8 rules: `-keep class com.sun.jna.** { *; }` and `-keep class dev.penwise.ffi.** { *; }`. JNA uses reflection ([gobley notes](https://gobley.dev/docs/gradle-plugins/uniffi/)).

**Alternatives:**
- **Mozilla `rust-android-gradle`:** the plugin portal shows 0.9.6 (Jan 2025) for `org.mozilla.rust-android-gradle.rust-android`.
  A 0.10.0 (Aug 2026) listing is [UNVERIFIED]. It drives cargo through Python plus AGP's NDK and adds nothing over a
  20-line cargo-ndk task. Skip it.
- **Gobley** is a KMP-oriented uniffi Gradle plugin. Overkill here.

**Per-call FFI cost** ([uniffi#3011](https://github.com/mozilla/uniffi-rs/issues/3011), measured 2026):
- Each uniffi Kotlin call allocates about **1.5 KB** (a `RustCallStatus` with 4 JNA `Memory` blocks registered with
  JNA's global Cleaner) and costs about **3–4 µs**. Methods pay this twice (the clone call).
- Calls from non-JVM threads add about 17 µs for attach/detach.
- Hand-written JNI with a reused status costs about 80 ns and 0 B ([uniffi#2672](https://github.com/mozilla/uniffi-rs/issues/2672)).
- So: **one call per MotionEvent batch**, never per sample.
  - Pack the historical and current samples (`x,y,p,tilt,orient,t`, f32/u32 LE) into a reusable **direct
    ByteBuffer** and call `stroke_append(builder, buf)` (zero-copy `&[u8]`).
  - Fetch the new outline segment in the same or one more call. The returned `Vec` crosses as a copied RustBuffer,
    which is fine for a few hundred points.
  - That is about 120–480 calls/s × 1.5 KB ≈ 0.2–0.7 MB/s of garbage: acceptable, but visible in GC.

**Recommendation:**
- **uniffi 0.32.2** for the full API: types, edit ops, codec.
- Add a scoped `cargo deny` exception, `[[licenses.exceptions]] allow = ["MPL-2.0"]`, for crates
  `uniffi`, `uniffi_core`, `uniffi_macros`, `uniffi_meta`, `uniffi_bindgen`, `uniffi_checksum_derive`, `uniffi_pipeline`, `uniffi_internal_macros`.
  MPL-2.0 is file-level copyleft and unmodified use imposes no obligations on our code. This needs Main's / the
  user's sign-off because it changes AGENTS.md policy.
- If the exception is refused, use **`jni` 0.22.4** with hand-written `extern "system"` functions in `penwise-ffi`.
  The API is small (stroke builder + ops as postcard bytes), and that also gives the fastest hot path.
- In either case, JNA **5.19.1@aar** (Apache-2.0 option) is needed only with uniffi. Build with **cargo-ndk 4.1.2**
  through the Variant API task above.

---

## 4. Local storage (op-log outbox)

| Option | Version | Notes |
|---|---|---|
| Room 2 | `androidx.room:room-runtime:2.8.5` | Annotation DAOs plus KSP. Mature. Blobs and `@RawQuery` are fine |
| Room 3 | `androidx.room3:room3-*:3.0.3` (stable 2026-07-01) | KMP-first rewrite: coroutines-only, Kotlin codegen, **KSP required** ([notes](https://developer.android.com/jetpack/androidx/releases/room3)) |
| SQLDelight | `app.cash.sqldelight:android-driver:2.4.1` (2026-10-06) | SQL-first `.sq` files generate typesafe Kotlin. No KSP. `verifySqlDelightMigration` checks migrations. Apache-2.0 |
| Plain SQLite | framework `SQLiteDatabase` or `androidx.sqlite:sqlite-bundled:2.7.1` | No codegen. Hand-mapped cursors |

The outbox needs very little: an append-only table plus a page/item snapshot cache. Ops are already opaque postcard
blobs produced by Rust. Design:
- `ops(seq INTEGER PRIMARY KEY, op_id BLOB NOT NULL UNIQUE, hlc INTEGER NOT NULL, page_id BLOB NOT NULL, payload BLOB NOT NULL, acked INTEGER NOT NULL DEFAULT 0)`
- A `sync_state(last_seen_seq)` table.
- Insert the op and update local state in **one transaction**, with WAL on (the default for the framework DB with
  `enableWriteAheadLogging`).
- Drain `WHERE acked=0 ORDER BY seq`, mark acked by `op_id` on server ack, and prune.
- `op_id` makes server-side apply idempotent, so duplicate pushes after a reconnect are harmless.

**Recommendation:** **SQLDelight 2.4.1** (`app.cash.sqldelight` Gradle plugin + `android-driver`, dialect
`sqlite-3-38`).
- The schema is plain SQL that mirrors the server's SQLite tables.
- It avoids KSP, a moving part that must track Kotlin 2.4.
- Migrations are verified in `just check`.
- Room adds annotation machinery but no value for a blob log.

---

## 5. Networking, discovery, pairing

**WebSocket:** `com.squareup.okhttp3:okhttp:5.5.1` (2026-10-22, Apache-2.0; [changelog](https://github.com/square/okhttp/blob/master/CHANGELOG.md)).
- Connect with `client.newWebSocket(Request, WebSocketListener)` and send binary postcard frames with `ws.send(ByteString)`.
- Set `pingInterval(15, SECONDS)` to detect dead Wi-Fi/Tailscale paths.
- OkHttp does **not** reconnect. Write a backoff loop that re-runs the outbox handshake (`last_seen_seq`).
- `send()` returns false once its 16 MiB queue is full, so throttle live `StrokeDelta` at the app level (one message
  per frame).
- 5.2+ reimplemented the WebSocket client on `Response.socket`. That is a behavior change only; the API is the same.

**mDNS (`_penwise._tcp`) with `NsdManager`** ([API](https://developer.android.com/reference/android/net/nsd/NsdManager)):
- Discover with `discoverServices("_penwise._tcp", PROTOCOL_DNS_SD, listener)`.
- For each hit, call `registerServiceInfoCallback(info, executor, cb)` (API 34). `resolveService` is deprecated since 34.
- Read `si.hostAddresses` (`getHost()` is deprecated) and the TXT `attributes`.
- Unregister every callback in `onStop`. Don't persist IPs.
- **Local Network Protection** ([docs](https://developer.android.com/privacy-and-security/local-network-permission)):
  - Apps with **targetSdk < 37** keep implicit LAN access.
  - At targetSdk 37 LAN is blocked unless the app holds `ACCESS_LOCAL_NETWORK` or uses the `DiscoveryRequest` +
    `FLAG_SHOW_PICKER` flow (API 37 / T-ext 22).
  - Staying on targetSdk 36 avoids this for now. Before bumping, switch to the picker or request the permission.

**Tailscale coexistence:**
- The Tailscale Android app is a `VpnService` and Android allows **one VPN at a time** ([FAQ](https://tailscale.com/docs/reference/faq/other-vpns)).
- Without an exit node it only routes 100.64.0.0/10 and MagicDNS, so LAN traffic and mDNS still use Wi-Fi.
- With an exit node, the user must enable **Allow LAN access**, which has had regressions
  ([#17720](https://github.com/tailscale/tailscale/issues/17720)).
- mDNS never crosses the tailnet (L3 tun, no multicast).
- Endpoint selection:
  1. Read the endpoint list from the QR code.
  2. Race the LAN address (from NSD or the QR) against the Tailscale MagicDNS name (`deskpc.<tailnet>.ts.net`).
  3. Prefer the LAN address if it answers within about 300 ms.
- The app needs no Tailscale SDK. Resolution goes through the system DNS that Tailscale sets.
- Whether LNP classifies 100.x CGNAT addresses as "local" at targetSdk 37 is [UNVERIFIED].

**QR scanning without ML Kit:**
- Avoid ML Kit and Google code scanner: proprietary Play-services.
- **zxing-cpp** `io.github.zxing-cpp:android:3.1.1` (2026-07-29, Apache-2.0; [README](https://github.com/zxing-cpp/zxing-cpp/tree/master/wrappers/android)).
  It is native and fast, and works directly on CameraX `ImageProxy`: `BarcodeReader().read(imageProxy)`.
- **CameraX** `androidx.camera:camera-{core,camera2,lifecycle,view}:1.6.2` (1.7.0-rc01 is pending). Use `ImageAnalysis`
  with `STRATEGY_KEEP_ONLY_LATEST` on a single-thread executor.
- Pure-Java `com.google.zxing:core:3.5.4` (Apache-2.0, maintenance mode) is the no-JNI fallback, but it is slower and
  needs manual luminance conversion.
- zxing-cpp ships its own `.so`. Filter ABIs to arm64-v8a (see §3).

**Recommendation:**
- OkHttp **5.5.1** WebSocket with an app-level reconnect/backoff and outbox resync.
- `NsdManager.discoverServices` + `registerServiceInfoCallback` (minSdk 34), with targetSdk kept at 36.
- LAN-first race with Tailscale MagicDNS fallback.
- QR: CameraX **1.6.2** + zxing-cpp **3.1.1**.

---

## 6. Lint and format

| Tool | Version | Notes |
|---|---|---|
| ktlint Gradle plugin | `org.jlleitschuh.gradle.ktlint` **14.2.0** (2026-03-12) | Pin the engine: `ktlint { version.set("1.8.0") }`. ktlint **1.8.0** (2025-12-15) is the latest CLI |
| detekt | `dev.detekt` **2.0.0-alpha.6** (2026-08-04) | The only line that tests Kotlin 2.4 / AGP 9.3 / Gradle 9.6.1 ([compat table](https://detekt.dev/docs/introduction/compatibility/)). 1.23.8 (the last stable) targets Kotlin 2.0.21 / AGP 8.8 and is not usable with KGP 2.4 / AGP 9 |
| Android Lint | bundled with AGP 9.3.3 | See below |

Pitfalls:
- **ktlint vs Kotlin 2.4:** ktlint 1.8.0's embedded parser predates Kotlin 2.4. New syntax, such as name-based
  destructuring and explicit backing fields, may fail to parse [UNVERIFIED]. Avoid those features until ktlint catches up.
- Configure ktlint through `.editorconfig` with `ktlint_code_style = ktlint_official`. Add
  `ktlint_function_naming_ignore_when_annotated_with = Composable` for Compose.
- **detekt 2.0:** the coordinates moved from `io.gitlab.arturbosch.detekt` to `dev.detekt`, and the ktlint wrapper rules
  are now separate. Use the ktlint plugin for formatting and detekt for analysis only (`buildUponDefaultConfig = true`,
  config in `android/config/detekt.yml`). Expect breaking changes before 2.0.0 final.

Android Lint (`android { lint { … } }`, [Lint DSL](https://developer.android.com/reference/tools/gradle-api/9.3/com/android/build/api/dsl/Lint)):

```kotlin
lint {
    warningsAsErrors = true
    abortOnError = true
    checkDependencies = true
    checkReleaseBuilds = true
    // These depend on time or the network, not on our code; they would break builds when a new version ships:
    disable += setOf("OldTargetApi", "GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion")
    baseline = file("lint-baseline.xml") // only if needed; keep it empty
}
```

`OldTargetApi` would otherwise fail every build, because targetSdk 36 < compileSdk 37 is deliberate (see §5 LNP).

**Recommendation:** ktlint-gradle **14.2.0** pinned to ktlint **1.8.0**; detekt **2.0.0-alpha.6** (`dev.detekt`);
Android Lint with `warningsAsErrors = true` and the four time-dependent checks disabled. Wire
`./gradlew ktlintCheck detekt lintDebug testDebugUnitTest` into `just check`.
