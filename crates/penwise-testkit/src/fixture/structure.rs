//! Notebook/page skeleton and the side tables of the fixture.

use rusqlite::params;

use super::PageExpect;
use super::seed::{Rows, err};

impl Rows<'_> {
    pub(super) fn structure(&mut self) -> Result<(), String> {
        let nbs = [
            ("nb_main", "Main", 0, 0),
            ("nb_second", "Second", 1, 0),
            ("nb_trash", "Trash", 2, 1),
        ];
        for (id, title, pos, deleted) in nbs {
            self.conn
                .execute(
                    "INSERT INTO notebooks (id,title,position,deleted) VALUES (?1,?2,?3,?4)",
                    params![id, title, pos, deleted],
                )
                .map_err(|e| err(&e))?;
            self.expect.notebooks.push((id.to_owned(), deleted != 0));
        }
        let pages = [
            (
                "pg_a",
                "nb_main",
                "Page A",
                "fixed:1404:1872",
                "ruled",
                0,
                0,
            ),
            ("pg_b", "nb_main", "Page B", "infinite", "grid", 1, 0),
            (
                "pg_gone",
                "nb_main",
                "Deleted page",
                "fixed:800:600",
                "blank",
                2,
                1,
            ),
            (
                "pg_odd",
                "nb_second",
                "Odd kind",
                "weird:kind",
                "dots",
                0,
                0,
            ),
            (
                "pg_in_trash",
                "nb_trash",
                "In trash",
                "infinite",
                "blank",
                0,
                0,
            ),
        ];
        for (id, nb, title, kind, bg, pos, deleted) in pages {
            self.conn
                .execute(
                    "INSERT INTO pages (id,notebook,title,kind,background,position,deleted) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![id, nb, title, kind, bg, pos, deleted],
                )
                .map_err(|e| err(&e))?;
            self.expect.pages.push(PageExpect {
                id: id.to_owned(),
                notebook: nb.to_owned(),
                deleted: deleted != 0,
                ..PageExpect::default()
            });
        }
        Ok(())
    }

    /// History, anchors, ink anchors and the active page.
    pub(super) fn extras(&mut self) -> Result<(), String> {
        let c = self.conn;
        for stamp in 1..=3_i64 {
            c.execute(
                "INSERT INTO object_history (page, stamp, id, kind, layer, z, codec, payload) VALUES ('pg_a', ?1, ?2, 'stroke', 'ink', 65536, 'stroke_v1', x'00')",
                params![stamp, format!("h{stamp}")],
            )
            .map_err(|e| err(&e))?;
            self.expect.history_rows += 1;
        }
        c.execute(
            "INSERT INTO anchors (page, selector, rect, role, text, url, stale) VALUES ('pg_a', '#hero', '[0,0,10,10]', 'banner', 'Hi', NULL, 0)",
            [],
        )
        .map_err(|e| err(&e))?;
        self.expect.anchors_rows = 1;
        c.execute(
            "INSERT INTO ink_anchors (page, object, selector) VALUES ('pg_a', '123', '#hero')",
            [],
        )
        .map_err(|e| err(&e))?;
        self.expect.ink_anchor_rows = 1;
        c.execute(
            "INSERT INTO state (key, value) VALUES ('active', 'pg_a')",
            [],
        )
        .map_err(|e| err(&e))?;
        "pg_a".clone_into(&mut self.expect.active_page);
        Ok(())
    }
}
