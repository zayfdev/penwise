//! JNI exports of [`SceneHandle`]: lifecycle, mirroring, undo/redo and render data.
//! Names are `Java_dev_penwise_core_Native_scene*`. Tools live in `jni_scene_tools`.
//!
//! Tools (`jni_scene_tools`), undo and redo commit to the `SyncCore` in the same call and return the edits as
//! an `encode_edits` blob (informational: they are already committed).
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JFloatArray, JObject, JString};
use jni::sys::{jboolean, jint, jlong};
use penwise_core::item::LayerSet;

use crate::jni_util::{
    bytes_in, bytes_out, floats_out, free_handle, guarded, handle_mut, handle_ref, into_handle,
    item_in, page_in, string_in,
};
use crate::render_floats::item_text;
use crate::scene_handle::{SceneHandle, encode_edits};
use crate::sync_core::{SyncCore, decode_changes};

pub(crate) fn layers_of(bits: jint) -> LayerSet {
    LayerSet {
        bits: u8::try_from(bits & 0b111).unwrap_or(0),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneOpen<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    sync: jlong,
    page: JByteArray<'local>,
    entropy: jlong,
) -> jlong {
    guarded(&mut env, |env| {
        let page = page_in(env, &page)?;
        let core = handle_ref::<SyncCore>(sync)?;
        Ok(into_handle(SceneHandle::open(
            core,
            page,
            entropy.cast_unsigned(),
        )?))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneFree<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    guarded(&mut env, |_env| {
        free_handle::<SceneHandle>(handle);
        Ok(())
    });
}

/// Mirrors the changes listed in a `syncReceive` outcome blob.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneRefresh<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    sync: jlong,
    outcome: JByteArray<'local>,
) {
    guarded(&mut env, |env| {
        let changes = decode_changes(&bytes_in(env, &outcome)?)?;
        handle_mut::<SceneHandle>(handle)?.refresh(handle_ref::<SyncCore>(sync)?, &changes)?;
        Ok(())
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneReload<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    sync: jlong,
) {
    guarded(&mut env, |_env| {
        handle_mut::<SceneHandle>(handle)?.reload(handle_ref::<SyncCore>(sync)?)?;
        Ok(())
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneNewItemId<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = handle_mut::<SceneHandle>(handle)?.new_item_id(now_ms.cast_unsigned());
        bytes_out(env, &id.0)
    })
}

/// `open != 0` begins an undo gesture, `0` ends it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneGesture<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    open: jint,
) {
    guarded(&mut env, |_env| {
        let scene = handle_mut::<SceneHandle>(handle)?;
        if open == 0 {
            scene.end_gesture();
        } else {
            scene.begin_gesture();
        }
        Ok(())
    });
}

/// `bit0` can undo, `bit1` can redo.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneHistory<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jint {
    guarded(&mut env, |_env| {
        let scene = handle_ref::<SceneHandle>(handle)?;
        Ok(jint::from(scene.can_undo()) | jint::from(scene.can_redo()) << 1)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneUndo<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.undo()?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneRedo<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    core: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let scene = handle_mut::<SceneHandle>(handle)?;
        let edits = scene.redo()?;
        scene.commit_into(
            handle_mut::<SyncCore>(core)?,
            &edits,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &encode_edits(&edits))
    })
}

/// Live item ids on `layers` (bit set: 1 ink, 2 agent, 4 text) in paint order, 16 bytes each.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneRenderOrder<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    layers: jint,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let ids = handle_ref::<SceneHandle>(handle)?.render_order(layers_of(layers));
        let flat: Vec<u8> = ids.iter().flat_map(|id| id.0).collect();
        bytes_out(env, &flat)
    })
}

/// Render data of one item (layout in `render_floats`); empty when the item is not drawable.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneItemFloats<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
) -> JFloatArray<'local> {
    guarded(&mut env, |env| {
        let id = item_in(env, &id)?;
        let floats = handle_ref::<SceneHandle>(handle)?
            .item_floats(id)
            .unwrap_or_default();
        floats_out(env, &floats)
    })
}

/// Ids whose render data changed since the last call: byte 0 is `1` when everything changed, then 16-byte ids.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneTakeDirty<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let dirty = handle_mut::<SceneHandle>(handle)?.take_dirty();
        bytes_out(env, &dirty)
    })
}

/// Content of text item `id`; empty for any other item.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneItemText<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
) -> JString<'local> {
    guarded(&mut env, |env| {
        let id = item_in(env, &id)?;
        let text = item_text(handle_ref::<SceneHandle>(handle)?.scene(), id).unwrap_or_default();
        Ok(env.new_string(text)?)
    })
}

/// Every live frame as the C1 JSON array (`scene_handle/frames.rs`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneFrames<'local>(
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
pub extern "system" fn Java_dev_penwise_core_Native_sceneSetFrameLayout<'local>(
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

/// JSON array of the element keys frame `frame_id`'s bound strokes need the bridge to watch.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_sceneFrameKeys<'local>(
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
pub extern "system" fn Java_dev_penwise_core_Native_sceneMarks<'local>(
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
pub extern "system" fn Java_dev_penwise_core_Native_sceneSetShowDone<'local>(
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
