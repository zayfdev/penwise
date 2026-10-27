//! JSON shapes of the catalog (unchanged from the pre-rewrite routes, ids are now ULID text) and request bodies.

use penwise_proto::{Background, Catalog, Notebook, Page, PageKind};
use serde::{Deserialize, Serialize};

use super::AppState;
use super::error::ApiError;
use super::resolve;

/// Protocol label of the index document.
const INDEX_VERSION: u32 = 4;

/// `{"t":"fixed","width":W,"height":H}` or `{"t":"infinite"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum KindJson {
    Fixed { width: f32, height: f32 },
    Infinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundJson {
    Blank,
    Ruled,
    Grid,
    Dots,
}

impl From<KindJson> for PageKind {
    fn from(kind: KindJson) -> Self {
        match kind {
            KindJson::Fixed { width, height } => Self::Fixed { width, height },
            KindJson::Infinite => Self::Infinite,
        }
    }
}

impl From<PageKind> for KindJson {
    fn from(kind: PageKind) -> Self {
        match kind {
            PageKind::Fixed { width, height } => Self::Fixed { width, height },
            PageKind::Infinite => Self::Infinite,
        }
    }
}

impl From<BackgroundJson> for Background {
    fn from(background: BackgroundJson) -> Self {
        match background {
            BackgroundJson::Blank => Self::Blank,
            BackgroundJson::Ruled => Self::Lined,
            BackgroundJson::Grid => Self::Grid,
            BackgroundJson::Dots => Self::Dot,
        }
    }
}

impl From<Background> for BackgroundJson {
    fn from(background: Background) -> Self {
        match background {
            Background::Blank => Self::Blank,
            Background::Lined => Self::Ruled,
            Background::Grid => Self::Grid,
            Background::Dot => Self::Dots,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct PageJson {
    pub id: String,
    pub title: String,
    pub notebook: String,
    pub kind: KindJson,
    pub background: BackgroundJson,
}

#[derive(Debug, Serialize)]
pub(crate) struct NotebookJson {
    pub id: String,
    pub title: String,
    pub pages: Vec<PageJson>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MetaJson {
    pub version: u32,
    pub active: String,
    pub notebooks: Vec<NotebookJson>,
}

pub(crate) fn page_json(page: &Page) -> PageJson {
    PageJson {
        id: page.id.to_string(),
        title: page.title.clone(),
        notebook: page.notebook.to_string(),
        kind: page.kind.into(),
        background: page.background.into(),
    }
}

pub(crate) fn notebook_json(catalog: &Catalog, notebook: &Notebook) -> NotebookJson {
    NotebookJson {
        id: notebook.id.to_string(),
        title: notebook.title.clone(),
        pages: catalog.pages(notebook.id).iter().map(page_json).collect(),
    }
}

/// The whole index with the current page (empty string while there are no pages).
pub(crate) async fn meta(app: &AppState) -> Result<MetaJson, ApiError> {
    let catalog = app.storage.catalog().await?;
    let active = resolve::current(app, &catalog)
        .await?
        .map(|id| id.to_string())
        .unwrap_or_default();
    Ok(MetaJson {
        version: INDEX_VERSION,
        active,
        notebooks: catalog
            .notebooks()
            .iter()
            .map(|nb| notebook_json(&catalog, nb))
            .collect(),
    })
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct NewPage {
    pub title: Option<String>,
    pub kind: Option<KindJson>,
    pub background: Option<BackgroundJson>,
    pub notebook: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PagePatchBody {
    pub title: Option<String>,
    pub background: Option<BackgroundJson>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct TitleBody {
    #[serde(default)]
    pub title: String,
}
