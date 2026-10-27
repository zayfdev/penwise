//! Shared JNI plumbing for the `SyncCore` / `SceneHandle` exports: handle boxing, array marshalling and the
//! panic/error guard.
//!
//! A handle is a `Box<T>` leaked into a `jlong`; Kotlin owns it and frees it once (a second free and any use
//! after the free are refused: live handles are registered). Handles are not thread-safe: Kotlin must not use
//! one handle from two threads at once, and must never pass a `SyncCore` handle where a `SceneHandle` is
//! expected (the wrappers in Kotlin keep the types apart).
#![allow(unsafe_code)] // Boxed-handle round trip.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JString};
use jni::sys::jlong;

use crate::stroke::FfiError;

/// Runs `f` with panics caught (`with_env` wraps it in `catch_unwind`) and every error or panic resolved to a
/// thrown `RuntimeException`, returning `T::default()` (null / 0) in that case. Nothing unwinds into the JVM.
pub fn guarded<'local, T: Default>(
    env: &mut EnvUnowned<'local>,
    f: impl FnOnce(&mut jni::Env<'local>) -> Result<T, FfiError>,
) -> T {
    env.with_env(f).resolve::<ThrowRuntimeExAndDefault>()
}

/// Addresses of every handle made by [`into_handle`] and not yet freed. A zeroed, stale or foreign handle is
/// refused instead of dereferenced, and a double free is a no-op, so a late call from another thread
/// (a render thread racing the close of the core) fails with an exception instead of corrupting memory.
static LIVE: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());

fn live() -> std::sync::MutexGuard<'static, BTreeSet<usize>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Leaks `value` into a handle.
pub fn into_handle<T>(value: T) -> jlong {
    let ptr = Box::into_raw(Box::new(value));
    live().insert(ptr as usize);
    ptr as jlong
}

/// Borrows the value behind a handle made by [`into_handle`].
///
/// # Errors
/// `BadArgument` for 0 and for a handle that was freed or never made here.
pub fn handle_mut<'a, T>(handle: jlong) -> Result<&'a mut T, FfiError> {
    let ptr = handle as *mut T;
    if ptr.is_null() {
        return Err(FfiError::BadArgument("null handle"));
    }
    if !live().contains(&(ptr as usize)) {
        return Err(FfiError::BadArgument("stale handle"));
    }
    // SAFETY: the address is in `LIVE`, so it came from `into_handle` and was not freed; the Kotlin wrapper
    // guarantees no concurrent use and the right handle type.
    Ok(unsafe { &mut *ptr })
}

/// Shared borrow of a handle (read-only operations on a second handle).
pub fn handle_ref<'a, T>(handle: jlong) -> Result<&'a T, FfiError> {
    handle_mut::<T>(handle).map(|v| &*v)
}

/// Takes the value out of a handle and frees it (the handle is dead afterwards).
///
/// # Errors
/// `BadArgument` for 0 and for a handle that was freed or never made here.
pub fn take_handle<T>(handle: jlong) -> Result<T, FfiError> {
    let ptr = handle as *mut T;
    if ptr.is_null() || !live().remove(&(ptr as usize)) {
        return Err(FfiError::BadArgument("stale handle"));
    }
    // SAFETY: it was in `LIVE`, so `into_handle` made it and this is the first (and now only) free.
    Ok(*unsafe { Box::from_raw(ptr) })
}

/// Drops a handle (0, an already freed or an unknown handle is a no-op).
pub fn free_handle<T>(handle: jlong) {
    let ptr = handle as *mut T;
    if !ptr.is_null() && live().remove(&(ptr as usize)) {
        // SAFETY: it was in `LIVE`, so `into_handle` made it and this is the first free.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

pub fn bytes_in(env: &jni::Env<'_>, array: &JByteArray<'_>) -> Result<Vec<u8>, FfiError> {
    Ok(env.convert_byte_array(array)?)
}

pub fn bytes_out<'local>(
    env: &mut jni::Env<'local>,
    bytes: &[u8],
) -> Result<JByteArray<'local>, FfiError> {
    Ok(env.byte_array_from_slice(bytes)?)
}

pub fn floats_in(env: &mut jni::Env<'_>, array: &JFloatArray<'_>) -> Result<Vec<f32>, FfiError> {
    let mut out = vec![0.0; array.len(env)?];
    if !out.is_empty() {
        array.get_region(env, 0, &mut out)?;
    }
    Ok(out)
}

pub fn ints_in(env: &mut jni::Env<'_>, array: &JIntArray<'_>) -> Result<Vec<i32>, FfiError> {
    let mut out = vec![0; array.len(env)?];
    if !out.is_empty() {
        array.get_region(env, 0, &mut out)?;
    }
    Ok(out)
}

pub fn floats_out<'local>(
    env: &mut jni::Env<'local>,
    data: &[f32],
) -> Result<JFloatArray<'local>, FfiError> {
    let array = env.new_float_array(data.len())?;
    array.set_region(env, 0, data)?;
    Ok(array)
}

pub fn ints_out<'local>(
    env: &mut jni::Env<'local>,
    data: &[i32],
) -> Result<JIntArray<'local>, FfiError> {
    let array = env.new_int_array(data.len())?;
    array.set_region(env, 0, data)?;
    Ok(array)
}

pub fn string_in(env: &jni::Env<'_>, s: &JString<'_>) -> Result<String, FfiError> {
    Ok(s.try_to_string(env)?)
}

/// A 16-byte id from a `byte[]`.
pub fn id_in(env: &jni::Env<'_>, array: &JByteArray<'_>) -> Result<[u8; 16], FfiError> {
    bytes_in(env, array)?
        .try_into()
        .map_err(|_| FfiError::BadArgument("id must be 16 bytes"))
}

/// An optional id: an empty array is `None`.
pub fn opt_id_in(env: &jni::Env<'_>, array: &JByteArray<'_>) -> Result<Option<[u8; 16]>, FfiError> {
    let bytes = bytes_in(env, array)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    bytes
        .try_into()
        .map(Some)
        .map_err(|_| FfiError::BadArgument("id must be 16 or 0 bytes"))
}

/// Concatenated 16-byte ids.
pub fn ids_in(bytes: &[u8]) -> Result<Vec<[u8; 16]>, FfiError> {
    if !bytes.len().is_multiple_of(16) {
        return Err(FfiError::BadArgument("id list length"));
    }
    Ok(bytes
        .chunks_exact(16)
        .filter_map(|c| <[u8; 16]>::try_from(c).ok())
        .collect())
}

pub fn page_in(
    env: &jni::Env<'_>,
    array: &JByteArray<'_>,
) -> Result<penwise_proto::PageId, FfiError> {
    penwise_proto::PageId::from_slice(&bytes_in(env, array)?)
        .ok_or(FfiError::BadArgument("page id must be 16 bytes"))
}

pub fn item_in(
    env: &jni::Env<'_>,
    array: &JByteArray<'_>,
) -> Result<penwise_core::item::ItemId, FfiError> {
    id_in(env, array).map(penwise_core::item::ItemId)
}

/// An optional page id: an empty array is `None`.
pub fn opt_page_in(
    env: &jni::Env<'_>,
    array: &JByteArray<'_>,
) -> Result<Option<penwise_proto::PageId>, FfiError> {
    opt_id_in(env, array)?
        .map(|b| penwise_proto::PageId::from_slice(&b).ok_or(FfiError::BadArgument("page id")))
        .transpose()
}
