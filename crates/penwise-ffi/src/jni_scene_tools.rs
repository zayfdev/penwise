//! JNI exports of the [`SceneHandle`] tools. Each returns an `encode_edits` blob for `syncCommitEdits`
//! (the edits are already applied to the scene and recorded in the open undo gesture).
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JObject};
use jni::sys::{jboolean, jfloat, jint, jlong};
use penwise_core::Seed;
use penwise_core::brush::{BrushParams, Color};
use penwise_core::item::{ItemId, Layer};

use crate::error::SyncError;
use crate::jni_scene::layers_of;
use crate::jni_util::{
    bytes_in, bytes_out, floats_in, guarded, handle_mut, handle_ref, ids_in, ints_in, item_in,
};
use crate::scene_handle::{
    SceneHandle, ShapeSpec, StrokeSpec, encode_edits, handle_from_code, pairs, shape_kind,
};
use crate::stroke::FfiError;

fn item_list(bytes: &[u8]) -> Result<Vec<ItemId>, FfiError> {
    Ok(ids_in(bytes)?.into_iter().map(ItemId).collect())
}

fn id_blob(ids: &[ItemId]) -> Vec<u8> {
    ids.iter().flat_map(|id| id.0).collect()
}

/// Commits a finished stroke. `brush` = `[preset, argb]`; `samples`/`times` as for `strokePush`.
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is fixed by `Native.sceneAddStroke`"
)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneAddStroke<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    brush: JIntArray<'local>,
    scale: jfloat,
    seed: jlong,
    samples: JFloatArray<'local>,
    times: JIntArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let (preset, argb) = match ints_in(env, &brush)?.as_slice() {
            [preset, argb] => (*preset, *argb),
            _ => return Err(SyncError::BadArgument("brush must be [preset, argb]").into()),
        };
        let (samples, times) = (floats_in(env, &samples)?, ints_in(env, &times)?);
        let spec = StrokeSpec {
            id: item_in(env, &id)?,
            preset: usize::try_from(preset).map_err(|_| SyncError::BadArgument("preset"))?,
            argb: argb.cast_unsigned(),
            scale,
            seed: Seed::from_i64_wire(seed).get(),
            samples: &samples,
            times: &times,
        };
        let edits = handle_mut::<SceneHandle>(handle)?.add_stroke(&spec)?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Commits a shape. `geometry` = `[x0, y0, x1, y1, kindParam]`, `style` = `[kind, constrain, preset, argb,
/// fillArgb, hasFill]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneAddShape<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    geometry: JFloatArray<'local>,
    style: JIntArray<'local>,
    scale: jfloat,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let (geometry, style) = (floats_in(env, &geometry)?, ints_in(env, &style)?);
        let ([x0, y0, x1, y1, param], [kind, constrain, preset, argb, fill_argb, has_fill]) =
            (geometry.as_slice(), style.as_slice())
        else {
            return Err(SyncError::BadArgument("shape geometry/style arrays").into());
        };
        let spec = ShapeSpec {
            id: item_in(env, &id)?,
            kind: shape_kind(*kind, *param)?,
            start: [*x0, *y0],
            end: [*x1, *y1],
            constrain: *constrain != 0,
            brush: BrushParams::for_preset(
                usize::try_from(*preset).map_err(|_| SyncError::BadArgument("preset"))?,
                argb.cast_unsigned(),
                scale,
            ),
            fill: (*has_fill != 0).then(|| Color::from_argb(fill_argb.cast_unsigned())),
        };
        let edits = handle_mut::<SceneHandle>(handle)?.add_shape(&spec)?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Drags `handle_code` (`Handle` declaration order) of shape `id` to `(x, y)`.
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is fixed by `Native.sceneResizeShape`"
)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneResizeShape<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    handle_code: jint,
    x: jfloat,
    y: jfloat,
    constrain: jboolean,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let edits = handle_mut::<SceneHandle>(handle)?.resize_shape(
            item_in(env, &id)?,
            handle_from_code(handle_code)?,
            [x, y],
            constrain,
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// One eraser chunk; `path` is interleaved x, y (page px).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneErase<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    path: JFloatArray<'local>,
    radius: jfloat,
    precise: jboolean,
    layers: jint,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let path = pairs(&floats_in(env, &path)?);
        let edits =
            handle_mut::<SceneHandle>(handle)?.erase(&path, radius, precise, layers_of(layers))?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Ids selected by a lasso polygon (interleaved x, y), 16 bytes each, bottom → top.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneLasso<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    polygon: JFloatArray<'local>,
    layers: jint,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let polygon = pairs(&floats_in(env, &polygon)?);
        let ids = handle_ref::<SceneHandle>(handle)?.lasso(&polygon, layers_of(layers));
        bytes_out(env, &id_blob(&ids))
    })
}

/// Ids hit at a tap, 16 bytes each, bottom → top.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneHitTest<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    x: jfloat,
    y: jfloat,
    tolerance: jfloat,
    layers: jint,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let ids = handle_ref::<SceneHandle>(handle)?.hit_test([x, y], tolerance, layers_of(layers));
        bytes_out(env, &id_blob(&ids))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneDeleteItems<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    ids: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let ids = item_list(&bytes_in(env, &ids)?)?;
        let edits = handle_mut::<SceneHandle>(handle)?.delete_items(&ids)?;
        bytes_out(env, &encode_edits(&edits))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneMoveItems<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    ids: JByteArray<'local>,
    dx: jfloat,
    dy: jfloat,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let ids = item_list(&bytes_in(env, &ids)?)?;
        let edits = handle_mut::<SceneHandle>(handle)?.move_items(&ids, dx, dy)?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// `layer`: `Layer::code` (0 ink, 1 agent, 2 text).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneSetLayer<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    ids: JByteArray<'local>,
    layer: jint,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let layer = u8::try_from(layer)
            .ok()
            .and_then(Layer::from_code)
            .ok_or(SyncError::BadArgument("layer"))?;
        let ids = item_list(&bytes_in(env, &ids)?)?;
        let edits = handle_mut::<SceneHandle>(handle)?.set_layer(&ids, layer)?;
        bytes_out(env, &encode_edits(&edits))
    })
}
