//! JNI bindings for `dev.penwise.core.Native` (Android). Conversion only: all ink logic lives in `penwise-core`
//! and all sync logic in `penwise-proto`.
//!
//! Two layers:
//! - the live-stroke exports in this file (`stroke*`, presets, paper tile): the pen's in-progress preview;
//! - the tablet surfaces, bytes in / bytes out, JNI-free at the core: [`sync_core::SyncCore`] (clock, outbox,
//!   replica, `/sync` frame codec, legacy import) and [`scene_handle::SceneHandle`] (scene, undo, pen tools,
//!   render floats), exported by the `jni_*` modules as `sync*` / `scene*`.
//!
//! Every entry point runs inside `EnvUnowned::with_env` (panics are caught) and resolves errors and panics
//! to a thrown `java.lang.RuntimeException` via [`ThrowRuntimeExAndDefault`], so nothing unwinds into the JVM.
//!
//! A stroke handle is a `Box<Stroke>` leaked into a `jlong`; Kotlin owns it and frees it exactly once via
//! `strokeFree`. Handles are not thread-safe: Kotlin must not use one handle from two threads at once.
#![allow(unsafe_code)] // JNI entry points need `#[unsafe(no_mangle)]` and the boxed-handle round trip.

mod bytes;
pub mod error;
pub mod ids;
mod jni_scene;
mod jni_scene_tools;
mod jni_sync;
mod jni_sync_authoring;
mod jni_util;
pub mod render_floats;
pub mod scene_handle;
mod stroke;
pub mod sync_core;

use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JObject};
use jni::sys::{jfloat, jint, jlong};
use penwise_core::stamp::texture;
use penwise_core::{Preset, Seed};

use crate::stroke::{FfiError, SAMPLE_FLOATS, Stroke};

/// Borrows the stroke behind a handle produced by `strokeNew`.
fn stroke_mut<'a>(handle: jlong) -> Result<&'a mut Stroke, FfiError> {
    let ptr = handle as *mut Stroke;
    if ptr.is_null() {
        return Err(FfiError::BadArgument("null stroke handle"));
    }
    // SAFETY: non-null handles come from `Box::into_raw` in `strokeNew` and stay valid until `strokeFree`;
    // the Kotlin wrapper guarantees no use after free and no concurrent use.
    Ok(unsafe { &mut *ptr })
}

fn new_float_array<'local>(
    env: &mut jni::Env<'local>,
    data: &[f32],
) -> Result<JFloatArray<'local>, FfiError> {
    let array = env.new_float_array(data.len())?;
    array.set_region(env, 0, data)?;
    Ok(array)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeNew<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    preset: jint,
    seed: jlong,
    argb: jint,
    size_scale: jfloat,
) -> jlong {
    env.with_env(|_env| -> Result<jlong, FfiError> {
        let params = stroke::brush_params(preset, argb, size_scale)?;
        let boxed = Box::new(Stroke::new(&params, Seed::from_i64_wire(seed).get()));
        Ok(Box::into_raw(boxed) as jlong)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeFree<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    env.with_env(|_env| -> Result<(), FfiError> {
        let ptr = handle as *mut Stroke;
        if !ptr.is_null() {
            // SAFETY: see `stroke_mut`; Kotlin calls this exactly once per handle and zeroes it afterwards.
            drop(unsafe { Box::from_raw(ptr) });
        }
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeStyle<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JFloatArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let style = stroke_mut(handle)?.style();
        new_float_array(env, &style)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokePush<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    samples: JFloatArray<'local>,
    t_us: JIntArray<'local>,
    count: jint,
) {
    env.with_env(|env| -> Result<(), FfiError> {
        let stroke = stroke_mut(handle)?;
        let count = usize::try_from(count).map_err(|_| FfiError::BadArgument("count < 0"))?;
        if samples.len(env)? < count * SAMPLE_FLOATS || t_us.len(env)? < count {
            return Err(FfiError::BadArgument("arrays shorter than count"));
        }
        let (xs, ts) = stroke.input_buffers(count);
        samples.get_region(env, 0, xs)?;
        t_us.get_region(env, 0, ts)?;
        stroke.push_buffered()
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeSnapshot<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JFloatArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let stroke = stroke_mut(handle)?;
        stroke.snapshot()?;
        new_float_array(env, stroke.output())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeFinish<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JFloatArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let stroke = stroke_mut(handle)?;
        stroke.finish()?;
        new_float_array(env, stroke.output())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_strokeContourEnds<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JIntArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let ends = stroke_mut(handle)?.contour_ends();
        let array = env.new_int_array(ends.len())?;
        array.set_region(env, 0, ends)?;
        Ok(array)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_presetName<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    index: jint,
) -> JObject<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let name = stroke::preset(index)?.name();
        Ok(JObject::from(env.new_string(name)?))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_presetDefaultArgb<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
) -> JIntArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let colors = Preset::ALL.map(|p| p.params().color.to_argb().cast_signed());
        let array = env.new_int_array(colors.len())?;
        array.set_region(env, 0, &colors)?;
        Ok(array)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_bakePaperTile<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    scale: jfloat,
    size: jint,
) -> JByteArray<'local> {
    env.with_env(|env| -> Result<_, FfiError> {
        let size = u32::try_from(size)
            .ok()
            .filter(|s| (1..=texture::MAX_TILE_SIZE).contains(s))
            .ok_or(FfiError::BadArgument("tile size out of range"))?;
        let mut tile = Vec::new();
        texture::bake_tile(scale, size, &mut tile);
        if tile.is_empty() {
            return Err(FfiError::BadArgument("tile allocation failed"));
        }
        Ok(env.byte_array_from_slice(&tile)?)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
