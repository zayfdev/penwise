//! JNI exports of [`SyncCore`]: sync status (caught up, revision), off-lock state persistence, dead letters and
//! the server-change decision. Names are `Java_dev_penwise_core_Native_sync*`.
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JObject};
use jni::sys::{jboolean, jlong};
use penwise_proto::OpId;

use crate::jni_util::{
    bytes_out, free_handle, guarded, handle_mut, handle_ref, id_in, into_handle, take_handle,
};
use crate::sync_core::{StateSnapshot, SyncCore, encode_dead_letters};

/// True once the replica holds what the server had when the connection began ([`SyncCore::is_caught_up`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncCaughtUp<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jboolean {
    guarded(&mut env, |_env| {
        Ok(handle_ref::<SyncCore>(handle)?.is_caught_up())
    })
}

/// Changes whenever the persistable state changed ([`SyncCore::revision`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncRevision<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jlong {
    guarded(&mut env, |_env| {
        Ok(i64::try_from(handle_ref::<SyncCore>(handle)?.revision()).unwrap_or(i64::MAX))
    })
}

/// Copies the persistable state (under the caller's lock); returns a snapshot handle for `syncStateEncode` or
/// `syncStateDiscard`, which free it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncStateBegin<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jlong {
    guarded(&mut env, |_env| {
        Ok(into_handle(
            handle_ref::<SyncCore>(handle)?.snapshot_state()?,
        ))
    })
}

/// Encodes and frees a snapshot; touches no `SyncCore`, so no lock is needed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncStateEncode<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    snapshot: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let copy = take_handle::<StateSnapshot>(snapshot)?;
        bytes_out(env, &copy.encode()?)
    })
}

/// Frees a snapshot without encoding it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncStateDiscard<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    snapshot: jlong,
) {
    guarded(&mut env, |_env| {
        free_handle::<StateSnapshot>(snapshot);
        Ok(())
    });
}

/// The dead letters blob ([`encode_dead_letters`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncDeadLetters<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let letters = handle_ref::<SyncCore>(handle)?.dead_letters();
        bytes_out(env, &encode_dead_letters(&letters))
    })
}

/// Stored envelope bytes of one dead letter (empty when unknown), for export.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncDeadLetterBytes<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    op_id: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let op_id = OpId(id_in(env, &op_id)?);
        let bytes = handle_ref::<SyncCore>(handle)?.dead_letter_bytes(op_id);
        bytes_out(env, bytes.as_deref().unwrap_or(&[]))
    })
}

/// Dismisses one dead letter.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncDeadLetterRemove<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    op_id: JByteArray<'local>,
) {
    guarded(&mut env, |env| {
        let op_id = OpId(id_in(env, &op_id)?);
        handle_mut::<SyncCore>(handle)?.remove_dead_letter(op_id);
        Ok(())
    });
}

/// True while a different server waits for the user's decision.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncServerChangePending<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> jboolean {
    guarded(&mut env, |_env| {
        Ok(handle_ref::<SyncCore>(handle)?.server_change_pending())
    })
}

/// Settles a held server change: `keep` re-queues every live notebook, page and item for the new server,
/// otherwise they are dropped. Returns the `Outcome` blob (reconnect + every page changed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncResolveServerChange<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    keep: jboolean,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let outcome =
            handle_mut::<SyncCore>(handle)?.resolve_server_change(keep, now_ms.cast_unsigned())?;
        bytes_out(env, &outcome.encode())
    })
}
