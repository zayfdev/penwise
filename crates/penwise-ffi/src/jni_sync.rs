//! JNI exports of [`SyncCore`] (session half): lifecycle, `Hello`, `receive`, `pollPush`, persistence.
//! Names are `Java_dev_penwise_core_NativeSync_sync*`; see `sync_core` for the semantics and the blob layouts.
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JObject, JString};
use jni::sys::{jboolean, jint, jlong};
use penwise_proto::{ClientKind, DeviceId};

use crate::error::SyncError;
use crate::jni_util::{
    bytes_in, bytes_out, free_handle, guarded, handle_mut, handle_ref, into_handle, string_in,
};
use crate::stroke::FfiError;
use crate::sync_core::SyncCore;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncNew<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    device: JByteArray<'local>,
    entropy: jlong,
) -> jlong {
    guarded(&mut env, |env| {
        let device = DeviceId::from_slice(&bytes_in(env, &device)?)
            .ok_or(FfiError::BadArgument("device id must be 16 bytes"))?;
        Ok(into_handle(SyncCore::new(device, entropy.cast_unsigned())))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncRestore<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    state: JByteArray<'local>,
    entropy: jlong,
) -> jlong {
    guarded(&mut env, |env| {
        let core = SyncCore::restore(&bytes_in(env, &state)?, entropy.cast_unsigned())?;
        Ok(into_handle(core))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncFree<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    guarded(&mut env, |_env| {
        free_handle::<SyncCore>(handle);
        Ok(())
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncExportState<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let bytes = handle_ref::<SyncCore>(handle)?.export_state()?;
        bytes_out(env, &bytes)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncDevice<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = handle_ref::<SyncCore>(handle)?.device().to_bytes();
        bytes_out(env, &id)
    })
}

/// `kind`: `ClientKind::code` (0 tablet, 1 browser, 2 agent).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncHello<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    token: JString<'local>,
    kind: jint,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let kind = u8::try_from(kind)
            .ok()
            .and_then(ClientKind::from_code)
            .ok_or(SyncError::BadArgument("client kind"))?;
        let token = string_in(env, &token)?;
        let frame = handle_mut::<SyncCore>(handle)?.hello(&token, kind, now_ms.cast_unsigned())?;
        bytes_out(env, &frame)
    })
}

/// One server frame in, the `Outcome` blob out (layout in `sync_core::outcome`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncReceive<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    frame: JByteArray<'local>,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let frame = bytes_in(env, &frame)?;
        let outcome = handle_mut::<SyncCore>(handle)?.receive(&frame, now_ms.cast_unsigned())?;
        bytes_out(env, &outcome.encode())
    })
}

/// The next `PushOps` frame, or an empty array when nothing may be sent now.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncPollPush<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let frame = handle_mut::<SyncCore>(handle)?.poll_push()?;
        bytes_out(env, frame.as_deref().unwrap_or(&[]))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncOffline<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    guarded(&mut env, |_env| {
        handle_mut::<SyncCore>(handle)?.go_offline();
        Ok(())
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncHeadSeq<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jlong {
    guarded(&mut env, |_env| {
        Ok(i64::try_from(handle_ref::<SyncCore>(handle)?.head_seq()).unwrap_or(i64::MAX))
    })
}

/// `[pending ops, dead letters]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncOutboxCounts<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jni::objects::JIntArray<'local> {
    guarded(&mut env, |env| {
        let core = handle_ref::<SyncCore>(handle)?;
        let counts = [
            i32::try_from(core.outbox().len()).unwrap_or(i32::MAX),
            i32::try_from(core.outbox().dead().len()).unwrap_or(i32::MAX),
        ];
        crate::jni_util::ints_out(env, &counts)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncCatalog<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let blob = handle_ref::<SyncCore>(handle)?.catalog_blob();
        bytes_out(env, &blob)
    })
}

/// Stamps and queues the edit list (`encode_edits` layout) for `page`; returns the op count.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncCommitEdits<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
    edits: JByteArray<'local>,
    now_ms: jlong,
) -> jint {
    guarded(&mut env, |env| {
        let page = crate::jni_util::page_in(env, &page)?;
        let edits = crate::scene_handle::decode_edits(&bytes_in(env, &edits)?)?;
        let ids =
            handle_mut::<SyncCore>(handle)?.commit_edits(page, &edits, now_ms.cast_unsigned())?;
        Ok(i32::try_from(ids.len()).unwrap_or(i32::MAX))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncSwitchPage<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let frame =
            handle_ref::<SyncCore>(handle)?.switch_page(crate::jni_util::page_in(env, &page)?)?;
        bytes_out(env, &frame)
    })
}

/// `Focus` frame (ask the other devices to show `page` and fit frame `item`; an empty `item` fits the page's
/// content). Empty when the server's `Welcome` did not report protocol 2: an older server would drop it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncFocus<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
    item: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let page = crate::jni_util::page_in(env, &page)?;
        let item = crate::jni_util::opt_id_in(env, &item)?.map(penwise_core::item::ItemId);
        let frame = handle_ref::<SyncCore>(handle)?.focus(page, item)?;
        bytes_out(env, frame.as_deref().unwrap_or_default())
    })
}

/// True when this connection's `Welcome` reported protocol 2 or newer (`Focus` may be sent).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncServerSupportsFocus<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jboolean {
    guarded(&mut env, |_env| {
        Ok(jboolean::from(
            handle_ref::<SyncCore>(handle)?.server_supports_focus(),
        ))
    })
}

/// `Watch` frame; an empty `page` stops watching.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncWatch<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let page = crate::jni_util::opt_page_in(env, &page)?;
        let frame = handle_ref::<SyncCore>(handle)?.watch(page)?;
        bytes_out(env, &frame)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_NativeSync_syncPing<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    nonce: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let frame = handle_ref::<SyncCore>(handle)?.ping(nonce.cast_unsigned())?;
        bytes_out(env, &frame)
    })
}
