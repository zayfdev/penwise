//! The element map: which selectors a captured page had, and where they sat.
//!
//! A picture taken from a browser carries the page it came from. Storing the element rectangles alongside lets an
//! annotation be resolved to what it marks ("inside `.hero__cta`") instead of only to pixels, and lets a later
//! capture re-resolve selectors after the layout moved.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Library;
use super::store::Store;

/// One element of a captured page, in CSS pixels of the captured viewport.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// CSS selector unique enough to find the element again.
    pub selector: String,
    /// `[x, y, width, height]` in CSS px, relative to the page's top-left.
    pub rect: [f32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Trimmed text content, for the agent's benefit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where the capture came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Anchor {
    /// Rejects anchors that cannot be resolved later: no selector, or a rect that is not four finite numbers.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        !self.selector.is_empty()
            && self.selector.len() <= MAX_SELECTOR
            && self.rect.iter().all(|value| value.is_finite())
    }
}

/// Longest accepted selector, in bytes.
const MAX_SELECTOR: usize = 1024;
/// Most anchors accepted in one request.
pub const MAX_ANCHORS: usize = 5_000;

impl Store {
    /// Replaces the element map of one page.
    pub(crate) fn write_anchors(&self, page: &str, anchors: &[Anchor]) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        tx.execute("DELETE FROM anchors WHERE page = ?1", params![page])
            .map_err(|error| error.to_string())?;
        for anchor in anchors {
            tx.execute(
                "INSERT OR REPLACE INTO anchors (page, selector, rect, role, text, url, stale)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
                params![
                    page,
                    anchor.selector,
                    serde_json::to_string(&anchor.rect).map_err(|error| error.to_string())?,
                    anchor.role,
                    anchor.text,
                    anchor.url
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    }

    /// One page's element map.
    pub(crate) fn read_anchors(&self, page: &str) -> Result<Vec<Anchor>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT selector, rect, role, text, url FROM anchors WHERE page = ?1
                 ORDER BY selector",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![page], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut anchors = Vec::new();
        for row in rows {
            let (selector, rect, role, text, url) = row.map_err(|error| error.to_string())?;
            let rect: [f32; 4] = serde_json::from_str(&rect).unwrap_or([0.0; 4]);
            anchors.push(Anchor {
                selector,
                rect,
                role,
                text,
                url,
            });
        }
        Ok(anchors)
    }
}

impl Store {
    /// Replaces which anchor each object on a page belongs to.
    pub(crate) fn write_ink_anchors(
        &self,
        page: &str,
        rows: &[(String, String)],
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        tx.execute("DELETE FROM ink_anchors WHERE page = ?1", params![page])
            .map_err(|error| error.to_string())?;
        for (object, selector) in rows {
            tx.execute(
                "INSERT OR REPLACE INTO ink_anchors (page, object, selector) VALUES (?1, ?2, ?3)",
                params![page, object, selector],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    }

    /// Which anchor each object on a page belongs to.
    pub(crate) fn read_ink_anchors(&self, page: &str) -> Result<Vec<(String, String)>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut stmt = conn
            .prepare("SELECT object, selector FROM ink_anchors WHERE page = ?1")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![page], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|error| error.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|error| error.to_string())?);
        }
        Ok(out)
    }
}

impl Library {
    /// Attributes every stroke on `page` to the smallest anchor containing it, so an agent learns *what* a mark
    /// is about rather than only where it is. Strokes outside every anchor stay unattributed.
    pub async fn resolve_anchors(&self, page: &str) {
        let anchors = match self.anchors(page) {
            Ok(anchors) if !anchors.is_empty() => anchors,
            Ok(_) => {
                let _ = self.store.write_ink_anchors(page, &[]);
                return;
            }
            Err(error) => {
                tracing::warn!(page, %error, "anchor read failed");
                return;
            }
        };
        let strokes = self.strokes(page).await;
        let mut rows = Vec::new();
        for stroke in &strokes {
            let Some((x, y)) = centre(&stroke.samples) else {
                continue;
            };
            if let Some(anchor) = containing(&anchors, x, y) {
                rows.push((stroke.seed.clone(), anchor.selector.clone()));
            }
        }
        if let Err(error) = self.store.write_ink_anchors(page, &rows) {
            tracing::warn!(page, %error, "anchor attribution failed");
        }
    }

    /// Annotations grouped by the element they belong to, plus the count that belongs to none.
    pub async fn annotations(&self, page: &str) -> Result<serde_json::Value, String> {
        let anchors = self.anchors(page)?;
        let attributed = self.store.read_ink_anchors(page)?;
        let mut elements: Vec<serde_json::Value> = Vec::new();
        for anchor in &anchors {
            let strokes = attributed
                .iter()
                .filter(|(_, selector)| selector == &anchor.selector)
                .count();
            elements.push(serde_json::json!({
                "selector": anchor.selector,
                "rect": anchor.rect,
                "role": anchor.role,
                "text": anchor.text,
                "strokes": strokes,
            }));
        }
        let unanchored = self
            .strokes(page)
            .await
            .len()
            .saturating_sub(attributed.len());
        Ok(serde_json::json!({
            "page": page,
            "elements": elements,
            "unanchored": unanchored,
        }))
    }

    /// Replaces a page's element map; returns how many anchors were stored, or an error for a bad page.
    pub async fn put_anchors(&self, page: &str, anchors: Vec<Anchor>) -> Result<usize, String> {
        if self.page(page).await.is_none() {
            return Err(format!("unknown page {page}"));
        }
        if anchors.len() > MAX_ANCHORS {
            return Err(format!("more than {MAX_ANCHORS} anchors"));
        }
        let anchors: Vec<Anchor> = anchors.into_iter().filter(Anchor::is_well_formed).collect();
        self.store.write_anchors(page, &anchors)?;
        self.resolve_anchors(page).await;
        Ok(anchors.len())
    }

    /// A page's element map.
    pub fn anchors(&self, page: &str) -> Result<Vec<Anchor>, String> {
        self.store.read_anchors(page)
    }
}

/// The middle of a stroke's bounding box, in page px; `None` for a stroke without samples.
fn centre(samples: &[f32]) -> Option<(f32, f32)> {
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    let mut seen = false;
    for pair in samples.chunks_exact(5) {
        let (x, y) = (pair[0], pair[1]);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        seen = true;
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    seen.then(|| (min.0.midpoint(max.0), min.1.midpoint(max.1)))
}

/// The smallest anchor whose rectangle contains `(x, y)`; the innermost elements win, which is what a mark on
/// a button inside a card means.
fn containing(anchors: &[Anchor], x: f32, y: f32) -> Option<&Anchor> {
    anchors
        .iter()
        .filter(|anchor| {
            let [left, top, width, height] = anchor.rect;
            x >= left && x <= left + width && y >= top && y <= top + height
        })
        .min_by(|a, b| {
            let area = |anchor: &Anchor| anchor.rect[2] * anchor.rect[3];
            area(a).total_cmp(&area(b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(selector: &str) -> Anchor {
        Anchor {
            selector: selector.to_owned(),
            rect: [10.0, 20.0, 300.0, 120.0],
            role: Some("button".to_owned()),
            text: Some("Get started".to_owned()),
            url: Some("https://example.com/".to_owned()),
        }
    }

    #[tokio::test]
    async fn anchors_round_trip_per_page() {
        let dir = std::env::temp_dir().join(format!("penwise-anchors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Library::load(dir.clone());
        let page = library.active().await;
        assert_eq!(
            2,
            library
                .put_anchors(&page, vec![anchor(".nav"), anchor("#cta")])
                .await
                .expect("stores")
        );
        let read = library.anchors(&page).expect("reads");
        assert_eq!(2, read.len());
        assert_eq!(
            "#cta", read[0].selector,
            "ordered by selector ('#' before '.')"
        );
        for (got, want) in read[0].rect.iter().zip([10.0, 20.0, 300.0, 120.0]) {
            assert!((got - want).abs() < 0.001, "rect {got} != {want}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn writing_anchors_replaces_the_previous_map() {
        let dir =
            std::env::temp_dir().join(format!("penwise-anchors-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Library::load(dir.clone());
        let page = library.active().await;
        library
            .put_anchors(&page, vec![anchor(".old"), anchor(".gone")])
            .await
            .expect("stores");
        library
            .put_anchors(&page, vec![anchor(".new")])
            .await
            .expect("stores");
        let read = library.anchors(&page).expect("reads");
        assert_eq!(1, read.len(), "the new capture replaces the old map");
        assert_eq!(".new", read[0].selector);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unusable_anchors_are_dropped_and_unknown_pages_refused() {
        let dir = std::env::temp_dir().join(format!("penwise-anchors-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Library::load(dir.clone());
        let page = library.active().await;
        let mut broken = anchor("");
        broken.rect = [f32::NAN, 0.0, 0.0, 0.0];
        let kept = library
            .put_anchors(&page, vec![broken, anchor(".ok")])
            .await
            .expect("stores");
        assert_eq!(1, kept);
        assert!(library.put_anchors("pg_missing", Vec::new()).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod attribution_tests {
    use super::*;
    use crate::library::StrokeMsg;

    fn stroke_at(seed: &str, x: f32, y: f32) -> StrokeMsg {
        StrokeMsg {
            layer: "ink".to_owned(),
            preset: 0,
            argb: -16_777_216,
            scale: 1.0,
            seed: seed.to_owned(),
            samples: vec![x, y, 0.5, 0.0, 0.0, x + 20.0, y + 10.0, 0.5, 0.0, 0.0],
            times: vec![0, 8_000],
        }
    }

    fn boxed(selector: &str, rect: [f32; 4]) -> Anchor {
        Anchor {
            selector: selector.to_owned(),
            rect,
            role: None,
            text: None,
            url: None,
        }
    }

    #[tokio::test]
    async fn a_stroke_belongs_to_the_smallest_element_that_contains_it() {
        let dir = std::env::temp_dir().join(format!("penwise-ink-anchors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Library::load(dir.clone());
        let page = library.active().await;
        // A card with a button inside it: a mark near the button is about the button.
        library
            .put_anchors(
                &page,
                vec![
                    boxed(".card", [0.0, 0.0, 800.0, 600.0]),
                    boxed("#buy", [100.0, 100.0, 200.0, 80.0]),
                ],
            )
            .await
            .expect("anchors");
        library.append(&page, stroke_at("1", 150.0, 130.0)).await;
        library.append(&page, stroke_at("2", 700.0, 550.0)).await;
        library.append(&page, stroke_at("3", 2000.0, 2000.0)).await;

        let body = library.annotations(&page).await.expect("annotations");
        let elements = body["elements"].as_array().expect("elements");
        let strokes_for = |selector: &str| {
            elements
                .iter()
                .find(|element| element["selector"] == selector)
                .and_then(|element| element["strokes"].as_u64())
                .unwrap_or(0)
        };
        assert_eq!(1, strokes_for("#buy"), "inside the button");
        assert_eq!(1, strokes_for(".card"), "the other card mark");
        assert_eq!(
            1,
            body["unanchored"].as_u64().unwrap_or(0),
            "one mark outside everything"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rewriting_the_element_map_re_attributes_existing_ink() {
        let dir =
            std::env::temp_dir().join(format!("penwise-ink-anchors-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Library::load(dir.clone());
        let page = library.active().await;
        library.append(&page, stroke_at("1", 150.0, 130.0)).await;
        let before = library.annotations(&page).await.expect("annotations");
        assert_eq!(1, before["unanchored"].as_u64().unwrap_or(0));

        library
            .put_anchors(&page, vec![boxed(".hero", [100.0, 100.0, 400.0, 300.0])])
            .await
            .expect("anchors");
        let after = library.annotations(&page).await.expect("annotations");
        assert_eq!(
            0,
            after["unanchored"].as_u64().unwrap_or(0),
            "ink is now attributed"
        );
        assert_eq!(1, after["elements"][0]["strokes"].as_u64().unwrap_or(0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
