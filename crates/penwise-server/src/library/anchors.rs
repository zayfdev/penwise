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

impl Library {
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
        Ok(anchors.len())
    }

    /// A page's element map.
    pub fn anchors(&self, page: &str) -> Result<Vec<Anchor>, String> {
        self.store.read_anchors(page)
    }
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
