//! Notebook/page lists for the UI as one flat blob (the replica's [`penwise_proto::Catalog`]).
//!
//! Layout (little-endian): `u32 n`, `n ×` notebook = `[16] id, u8 deleted, i64 position, u32 len + utf8 title`;
//! `u32 m`, `m ×` page = `[16] id, [16] notebook, u8 deleted, i64 position, u8 kind (0 fixed, 1 infinite),
//! f32 width, f32 height (0 for infinite), u8 background (Background::code), u32 len + utf8 title`.
//! Tombstoned rows are included (flag set) so the UI can offer "recently deleted". Notebooks and pages are in
//! ascending `(position, id)` order as returned by the catalog.

use penwise_proto::{Notebook, Page, PageKind};

use super::SyncCore;
use crate::bytes::Writer;

impl SyncCore {
    /// The catalog blob described in the module header.
    #[must_use]
    pub fn catalog_blob(&self) -> Vec<u8> {
        let catalog = self.replica.catalog();
        let mut notebooks = catalog.notebooks();
        notebooks.extend(catalog.deleted_notebooks());
        let mut w = Writer::new();
        w.count(notebooks.len());
        let mut pages = Vec::new();
        for notebook in &notebooks {
            write_notebook(&mut w, notebook);
            pages.extend(catalog.pages(notebook.id));
            pages.extend(catalog.deleted_pages(notebook.id));
        }
        w.count(pages.len());
        for page in &pages {
            write_page(&mut w, page);
        }
        w.finish()
    }
}

fn write_notebook(w: &mut Writer, notebook: &Notebook) {
    w.raw(&notebook.id.to_bytes());
    w.u8(u8::from(notebook.deleted));
    w.i64(notebook.position);
    w.blob(notebook.title.as_bytes());
}

fn write_page(w: &mut Writer, page: &Page) {
    w.raw(&page.id.to_bytes());
    w.raw(&page.notebook.to_bytes());
    w.u8(u8::from(page.deleted));
    w.i64(page.position);
    match page.kind {
        PageKind::Fixed { width, height } => {
            w.u8(0);
            w.f32(width);
            w.f32(height);
        }
        PageKind::Infinite => {
            w.u8(1);
            w.f32(0.0);
            w.f32(0.0);
        }
    }
    w.u8(page.background.code());
    w.blob(page.title.as_bytes());
}
