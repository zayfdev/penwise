//! JNI exports of [`SyncCore`] (authoring half): notebooks, pages, legacy import, live frames.
//! `*Update*` flag bits say which optional values are present (see each function).
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]` entry points.

use jni::EnvUnowned;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JObject, JString};
use jni::sys::{jfloat, jint, jlong};
use penwise_core::Seed;
use penwise_core::brush::BrushParams;
use penwise_core::sample::Sample;
use penwise_proto::{Background, NotebookId, NotebookPatch, PageKind, PagePatch};

use crate::error::SyncError;
use crate::jni_util::{
    bytes_out, floats_in, guarded, handle_mut, handle_ref, ints_in, ints_out, item_in, opt_id_in,
    page_in, string_in,
};
use crate::scene_handle::SAMPLE_FLOATS;
use crate::sync_core::{LiveChunk, NewPage, SyncCore};

const HAS_TITLE: jint = 1;
const HAS_POSITION: jint = 2;
const HAS_DELETED: jint = 4;
const DELETED_VALUE: jint = 8;
const HAS_BACKGROUND: jint = 16;

fn flag(flags: jint, bit: jint) -> bool {
    flags & bit != 0
}

/// New notebook; an empty `id` mints one. Returns the id.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncCreateNotebook<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    title: JString<'local>,
    position: jlong,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = opt_id_in(env, &id)?.and_then(|b| NotebookId::from_slice(&b));
        let title = string_in(env, &title)?;
        let made = handle_mut::<SyncCore>(handle)?.create_notebook(
            id,
            &title,
            position,
            now_ms.cast_unsigned(),
        )?;
        bytes_out(env, &made.to_bytes())
    })
}

/// Notebook update; flags: 1 title, 2 position, 4 deleted (value in 8).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncUpdateNotebook<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    flags: jint,
    title: JString<'local>,
    position: jlong,
    now_ms: jlong,
) {
    guarded(&mut env, |env| {
        let id = NotebookId::from_slice(&crate::jni_util::bytes_in(env, &id)?)
            .ok_or(SyncError::BadArgument("notebook id"))?;
        let patch = NotebookPatch {
            title: flag(flags, HAS_TITLE)
                .then(|| string_in(env, &title))
                .transpose()?,
            position: flag(flags, HAS_POSITION).then_some(position),
            deleted: flag(flags, HAS_DELETED).then_some(flag(flags, DELETED_VALUE)),
        };
        handle_mut::<SyncCore>(handle)?.update_notebook(id, patch, now_ms.cast_unsigned())?;
        Ok(())
    });
}

/// New page; an empty `id` mints one; empty `size` = infinite, else `[width, height]`. `background` is
/// `Background::code`. Returns the id.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncCreatePage<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    notebook: JByteArray<'local>,
    title: JString<'local>,
    position: jlong,
    size: JFloatArray<'local>,
    background: jint,
    now_ms: jlong,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let id = opt_id_in(env, &id)?.and_then(|b| penwise_proto::PageId::from_slice(&b));
        let notebook = NotebookId::from_slice(&crate::jni_util::bytes_in(env, &notebook)?)
            .ok_or(SyncError::BadArgument("notebook id"))?;
        let size = floats_in(env, &size)?;
        let kind = match size.as_slice() {
            [] => PageKind::Infinite,
            [width, height] => PageKind::Fixed {
                width: *width,
                height: *height,
            },
            _ => return Err(SyncError::BadArgument("size must be empty or [w, h]").into()),
        };
        let background = u8::try_from(background)
            .ok()
            .and_then(Background::from_code)
            .ok_or(SyncError::BadArgument("background"))?;
        let page = NewPage {
            notebook,
            title: string_in(env, &title)?,
            position,
            kind,
            background,
        };
        let made = handle_mut::<SyncCore>(handle)?.create_page(id, page, now_ms.cast_unsigned())?;
        bytes_out(env, &made.to_bytes())
    })
}

/// Page update; flags: 1 title, 2 position, 4 deleted (value in 8), 16 background.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncUpdatePage<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: JByteArray<'local>,
    flags: jint,
    title: JString<'local>,
    position: jlong,
    background: jint,
    now_ms: jlong,
) {
    guarded(&mut env, |env| {
        let page = page_in(env, &id)?;
        let background = if flag(flags, HAS_BACKGROUND) {
            Some(
                u8::try_from(background)
                    .ok()
                    .and_then(Background::from_code)
                    .ok_or(SyncError::BadArgument("background"))?,
            )
        } else {
            None
        };
        let patch = PagePatch {
            title: flag(flags, HAS_TITLE)
                .then(|| string_in(env, &title))
                .transpose()?,
            position: flag(flags, HAS_POSITION).then_some(position),
            background,
            deleted: flag(flags, HAS_DELETED).then_some(flag(flags, DELETED_VALUE)),
        };
        handle_mut::<SyncCore>(handle)?.update_page(page, patch, now_ms.cast_unsigned())?;
        Ok(())
    });
}

/// Imports an old `PWST` page file; returns `[added, alreadyPresent, skipped]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncImportLegacyPage<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page_text: JString<'local>,
    file: JByteArray<'local>,
    now_ms: jlong,
) -> JIntArray<'local> {
    guarded(&mut env, |env| {
        let page_text = string_in(env, &page_text)?;
        let file = crate::jni_util::bytes_in(env, &file)?;
        let report = handle_mut::<SyncCore>(handle)?.import_legacy_page(
            &page_text,
            &file,
            now_ms.cast_unsigned(),
        )?;
        let to_int = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        ints_out(
            env,
            &[
                to_int(report.added),
                to_int(report.already_present),
                to_int(report.skipped),
            ],
        )
    })
}

/// `StrokeDelta` frame for `count` samples (interleaved like `strokePush`); `start` is the index of the first.
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is fixed by `Native.syncLiveDelta`"
)]
pub extern "system" fn Java_dev_penwise_core_Native_syncLiveDelta<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
    stroke: JByteArray<'local>,
    brush: JIntArray<'local>,
    scale: jfloat,
    seed: jlong,
    start: jint,
    samples: JFloatArray<'local>,
    times: JIntArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let page = page_in(env, &page)?;
        let stroke = item_in(env, &stroke)?;
        let (preset, argb) = match ints_in(env, &brush)?.as_slice() {
            [preset, argb] => (*preset, *argb),
            _ => return Err(SyncError::BadArgument("brush must be [preset, argb]").into()),
        };
        let preset = usize::try_from(preset).map_err(|_| SyncError::BadArgument("preset"))?;
        let params = BrushParams::for_preset(preset, argb.cast_unsigned(), scale);
        let (floats, times) = (floats_in(env, &samples)?, ints_in(env, &times)?);
        if floats.len() != times.len() * SAMPLE_FLOATS {
            return Err(SyncError::BadArgument("samples and times disagree").into());
        }
        let samples: Vec<Sample> = floats
            .chunks_exact(SAMPLE_FLOATS)
            .zip(&times)
            .map(|(s, &t)| Sample {
                x: s[0],
                y: s[1],
                pressure: s[2],
                tilt: s[3],
                azimuth: s[4],
                t_us: t.cast_unsigned(),
            })
            .collect();
        let chunk = LiveChunk {
            page,
            stroke,
            params: &params,
            seed: Seed::from_i64_wire(seed).get(),
            start: u32::try_from(start).map_err(|_| SyncError::BadArgument("start"))?,
            samples: &samples,
        };
        let frame = handle_ref::<SyncCore>(handle)?.live_delta(&chunk)?;
        bytes_out(env, &frame)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_penwise_core_Native_syncLiveCancel<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    handle: jlong,
    page: JByteArray<'local>,
    stroke: JByteArray<'local>,
) -> JByteArray<'local> {
    guarded(&mut env, |env| {
        let (page, stroke) = (page_in(env, &page)?, item_in(env, &stroke)?);
        let frame = handle_ref::<SyncCore>(handle)?.live_cancel(page, stroke)?;
        bytes_out(env, &frame)
    })
}
