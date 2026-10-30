//! JNI exports of the design harness' frames and marks: the frame list, presets, routes, layouts, bound strokes,
//! mark status, and frame space (live window, board ↔ document points). Names are
//! `Java_dev_penwise_core_NativeFrames_*`; the scene lifecycle and the pen tools live in `jni_scene` and
//! `jni_scene_tools` (`NativeScene`).
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong};
use penwise_core::Seed;

use crate::error::SyncError;
use crate::jni_util::{
    bytes_out, floats_in, floats_out, guarded, handle_mut, handle_ref, ints_in, item_in, string_in,
};
use crate::scene_handle::{SceneHandle, StrokeSpec, encode_edits};
use crate::sync_core::SyncCore;

/// Every live frame as the C1 JSON array (`scene_handle/frames.rs`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneFrames<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JString<'local> {
    guarded(&mut env, |env| {
        let json = handle_ref::<SceneHandle>(handle)?.frames_json();
        Ok(env.new_string(json)?)
    })
}

/// Merges one `pw:layout` message into frame `frame_id`'s layout; true when the visible output changed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetFrameLayout<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
    json: JString<'local>,
) -> jboolean {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let json = string_in(env, &json)?;
        let changed = handle_mut::<SceneHandle>(handle)?.set_frame_layout(frame, &json)?;
        Ok(jboolean::from(changed))
    })
}

/// Drops frame `frame_id`'s layout entirely (demote to the still: its layout replaces the live one); true when a
/// layout existed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneClearFrameLayout<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
) -> jboolean {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let cleared = handle_mut::<SceneHandle>(handle)?.clear_frame_layout(frame);
        Ok(jboolean::from(cleared))
    })
}

/// Replaces frame `frame_id`'s layout with the one in `json` in one native call (a live frame demoted to its still).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneReplaceFrameLayout<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
    json: JString<'local>,
) -> jboolean {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let json = string_in(env, &json)?;
        let changed = handle_mut::<SceneHandle>(handle)?.replace_frame_layout(frame, &json)?;
        Ok(jboolean::from(changed))
    })
}

/// JSON array of the element keys frame `frame_id`'s bound strokes need the bridge to watch.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneFrameKeys<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
) -> JString<'local> {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let keys = handle_ref::<SceneHandle>(handle)?.frame_keys(frame);
        Ok(env.new_string(keys)?)
    })
}

/// Every live mark as JSON (`scene_handle/marks.rs`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneMarks<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JString<'local> {
    guarded(&mut env, |env| {
        let json = handle_ref::<SceneHandle>(handle)?.marks_json();
        Ok(env.new_string(json)?)
    })
}

/// Sets the show-done view flag; true when the visible output changed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetShowDone<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    on: jboolean,
) -> jboolean {
    guarded(&mut env, |_env| {
        let changed = handle_mut::<SceneHandle>(handle)?.set_show_done(on);
        Ok(jboolean::from(changed))
    })
}

/// Places a frame from `{project,route,preset,cssW?,cssH?,x,y}` (C1); returns the edit blob.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneAddFrame<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    json: JString<'local>,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let json = string_in(env, &json)?;
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.add_frame(&json, now_ms.cast_unsigned())?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Switches a frame's preset and CSS size; `cssW`/`cssH` only matter for `custom`.
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is fixed by `NativeFrames.sceneSetFramePreset`"
)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetFramePreset<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    preset: JString<'local>,
    css_w: jint,
    css_h: jint,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let preset = string_in(env, &preset)?;
        let id = item_in(env, &id)?;
        let (css_w, css_h) = (css_w.max(0).cast_unsigned(), css_h.max(0).cast_unsigned());
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.set_frame_preset(id, &preset, css_w, css_h)?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Points a frame at another route; returns the edit blob.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetFrameRoute<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    route: JString<'local>,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let route = string_in(env, &route)?;
        let id = item_in(env, &id)?;
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.set_frame_route(id, &route)?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Marks variant frame `id` as the owner's pick (or un-marks it); choosing one un-chooses its group's others.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetFrameChosen<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    chosen: jboolean,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = item_in(env, &id)?;
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.set_frame_chosen(id, chosen)?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Commits a stroke drawn on a frame (`bindingJson` = `{"frame":"<32hex>","mode":"bound"|"area","key":"…"?}`);
/// samples are page coords. Returns the edit blob (the stroke plus its mark add/update).
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is fixed by `NativeFrames.sceneAddBoundStroke`"
)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneAddBoundStroke<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    brush: JIntArray<'local>,
    scale: jfloat,
    seed: jlong,
    samples: JFloatArray<'local>,
    times: JIntArray<'local>,
    binding_json: JString<'local>,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let (preset, argb) = match ints_in(env, &brush)?.as_slice() {
            [preset, argb] => (*preset, *argb),
            _ => return Err(SyncError::BadArgument("brush must be [preset, argb]").into()),
        };
        let (samples, times) = (floats_in(env, &samples)?, ints_in(env, &times)?);
        let binding_json = string_in(env, &binding_json)?;
        let spec = StrokeSpec {
            id: item_in(env, &id)?,
            preset: usize::try_from(preset).map_err(|_| SyncError::BadArgument("preset"))?,
            argb: argb.cast_unsigned(),
            scale,
            seed: Seed::from_i64_wire(seed).get(),
            samples: &samples,
            times: &times,
        };
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.add_bound_stroke(&spec, &binding_json, now_ms.cast_unsigned())?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Sets a mark's status (`new`/`seen`/`done`/`reopened`); returns the edit blob.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneSetMarkStatus<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    mark_id: JByteArray<'local>,
    status: JString<'local>,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = item_in(env, &mark_id)?;
        let status = string_in(env, &status)?;
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.set_mark_status(id, &status)?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Frame `frame_id`'s live view on the board, `[x, y, w, h]`; empty for an unknown frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_frameLiveWindow<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
) -> JFloatArray<'local> {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let window = handle_ref::<SceneHandle>(handle)?.frame_live_window(frame);
        floats_out(env, window.as_ref().map_or(&[][..], |w| &w[..]))
    })
}

/// Bounds of everything on `layers` (bit set as for `sceneRenderOrder`) as `[min_x, min_y, max_x, max_y]`; empty
/// for an empty page.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_sceneContentBounds<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    layers: jint,
) -> JFloatArray<'local> {
    guarded(&mut env, |env| {
        let bounds =
            handle_ref::<SceneHandle>(handle)?.content_bounds(crate::jni_scene::layers_of(layers));
        floats_out(env, bounds.as_ref().map_or(&[][..], |b| &b[..]))
    })
}

/// The document point of frame `frame_id` under board point `(x, y)`, `[x, y]`; empty for an unknown frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_frameDocPoint<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
    x: jfloat,
    y: jfloat,
) -> JFloatArray<'local> {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let point = handle_ref::<SceneHandle>(handle)?.frame_doc_point(frame, [x, y]);
        floats_out(env, point.as_ref().map_or(&[][..], |p| &p[..]))
    })
}

/// The board point of document point `(x, y)` of frame `frame_id`, `[x, y]`; empty for an unknown frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeFrames_frameBoardPoint<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame_id: JByteArray<'local>,
    x: jfloat,
    y: jfloat,
) -> JFloatArray<'local> {
    guarded(&mut env, |env| {
        let frame = item_in(env, &frame_id)?;
        let point = handle_ref::<SceneHandle>(handle)?.frame_board_point(frame, [x, y]);
        floats_out(env, point.as_ref().map_or(&[][..], |p| &p[..]))
    })
}
