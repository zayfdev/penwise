//! The pre-roll: scroll a freshly loaded page through its whole document before it is measured or captured.
//!
//! Why: a page that has only been loaded is not the page the owner sees after scrolling it. Images with
//! `loading="lazy"` have not been fetched, sections that fade in on scroll are still hidden, an infinite feed has
//! not grown, and every one of those changes the document height and the rects marks are bound to. So the page is
//! stepped through in viewport-sized steps (a short settle per step, capped at [`MAX_STILL_DOC`] px and a time
//! budget), returned to scroll 0, given a bounded wait for the network and for images to decode, and only then
//! measured (and animations are finished, [`freeze_animations`]). Anything that appears later is picked up by the next still, which a live reload or HMR triggers.

use std::time::Duration;

use serde_json::Value;

use crate::inspect::{Error, Inspect, Page};

/// The tallest document a still covers, in CSS px. A page past it is an infinite feed (contract §6).
pub(crate) const MAX_STILL_DOC: f32 = 30_000.0;
/// Time a step waits for observers, lazy loads and scroll handlers.
const SETTLE_MS: u32 = 120;
/// The whole scroll-through may take this long (about 34 steps of 900 px plus settles fit well inside it).
const BUDGET_MS: u32 = 8_000;
/// The network counts as idle after this long without a request in flight.
const QUIET: Duration = Duration::from_millis(300);
/// The wait for the network is abandoned after this long (a page that polls never goes quiet).
const NETWORK_LIMIT: Duration = Duration::from_secs(5);

const SCRIPT: &str = include_str!("preroll.js");

/// What the scroll-through found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Preroll {
    /// Scroll steps taken.
    pub(crate) steps: u32,
    /// The document height right after the scroll-through, in CSS px.
    pub(crate) doc_h: f32,
}

/// Rolls the page through its document and waits for it to settle; the page ends at scroll 0.
///
/// # Errors
/// The page could not be evaluated or the browser connection ended.
pub(crate) async fn preroll(
    inspect: &Inspect,
    page: &Page,
    viewport_h: u32,
) -> Result<Preroll, Error> {
    let call = format!("({SCRIPT})({viewport_h}, {MAX_STILL_DOC}, {SETTLE_MS}, {BUDGET_MS})");
    let rolled = tokio::time::timeout(
        Duration::from_millis(u64::from(BUDGET_MS) + 4_000),
        inspect.eval(page, &call),
    )
    .await
    .map_err(|_| Error::Timeout("pre-roll"))??;
    let network = page.wait_network_idle(QUIET, NETWORK_LIMIT).await?;
    if !network {
        tracing::debug!("pre-roll: network still busy after the wait; measuring anyway");
    }
    // Images that finished loading may not be decoded (or laid out) yet.
    inspect
        .eval(
            page,
            "Promise.race([
               Promise.all(Array.from(document.images).map(function (image) {
                 return image.decode ? image.decode().catch(function () {}) : Promise.resolve();
               })),
               new Promise(function (resolve) { setTimeout(resolve, 2000); })
             ]).then(function () { return true; })",
        )
        .await?;
    let measured = inspect
        .eval(
            page,
            "Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0)",
        )
        .await?;
    Ok(Preroll {
        steps: count(&rolled["steps"]),
        doc_h: number(&measured),
    })
}

fn count(value: &Value) -> u32 {
    u32::try_from(value.as_u64().unwrap_or(0)).unwrap_or(u32::MAX)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "document heights are far below 2²⁴"
)]
fn number(value: &Value) -> f32 {
    value.as_f64().unwrap_or(0.0) as f32
}

/// Finishes every running animation and transition (infinite ones are paused at their start), so that two
/// captures of an unchanged page are byte-identical and "unchanged" can be told by comparing them. Passes repeat
/// because finishing one can start another (a `transitionend` handler, a chained animation).
///
/// # Errors
/// The page could not be evaluated.
pub(crate) async fn freeze_animations(inspect: &Inspect, page: &Page) -> Result<(), Error> {
    inspect
        .eval(
            page,
            "(function () {
               for (var pass = 0; pass < 3; pass += 1) {
                 var running = document.getAnimations ? document.getAnimations() : [];
                 if (!running.length) break;
                 running.forEach(function (animation) {
                   try {
                     animation.finish();
                   } catch (_) {
                     try { animation.pause(); animation.currentTime = 0; } catch (__) { /* gone */ }
                   }
                 });
               }
               return true;
             })()",
        )
        .await?;
    Ok(())
}
