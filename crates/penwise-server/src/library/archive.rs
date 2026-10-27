//! Server-side archives of a page's objects: the undo a client cannot reach.

use rusqlite::{OptionalExtension, Transaction, params};

use super::model::PageContent;
use super::store::{HISTORY_KEEP, Store, read_objects};

impl Store {
    /// Copies a page's objects into `object_history`, so a destructive edit can be undone on the server.
    ///
    /// Clears, replaces and erases used to be irreversible once the client had written them: the tablet has its
    /// own undo stack, the server had nothing. Only the newest [`HISTORY_KEEP`] archives per page are kept.
    pub(crate) fn archive(&self, page: &str) -> Result<usize, String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let archived = archive_in(&tx, page)?;
        if archived > 0 {
            prune_in(&tx, page)?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(archived)
    }

    /// One page's objects, read fresh from the database.
    pub(crate) fn content(&self, page: &str) -> Result<PageContent, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        read_objects(&conn, page)
    }

    /// The archive stamps for a page, newest first, with how many objects each holds.
    pub(crate) fn history(&self, page: &str) -> Result<Vec<(i64, usize)>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT stamp, COUNT(*) FROM object_history WHERE page = ?1
                 GROUP BY stamp ORDER BY stamp DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![page], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    usize::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|error| error.to_string())?);
        }
        Ok(out)
    }

    /// Puts the newest archive (or the one at `stamp`) back, replacing the page's objects.
    ///
    /// `None` (and nothing changed) when there is no such archive. The current objects are archived first, in
    /// the same transaction, so a restore is itself undoable.
    pub(crate) fn restore(&self, page: &str, stamp: Option<i64>) -> Result<Option<usize>, String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let chosen: Option<i64> = match stamp {
            Some(stamp) => tx
                .query_row(
                    "SELECT stamp FROM object_history WHERE page = ?1 AND stamp = ?2 LIMIT 1",
                    params![page, stamp],
                    |row| row.get(0),
                )
                .optional(),
            None => tx.query_row(
                "SELECT MAX(stamp) FROM object_history WHERE page = ?1",
                params![page],
                |row| row.get::<_, Option<i64>>(0),
            ),
        }
        .map_err(|error| error.to_string())?;
        let Some(stamp) = chosen else {
            return Ok(None);
        };
        archive_in(&tx, page)?;
        tx.execute("DELETE FROM objects WHERE page = ?1", params![page])
            .map_err(|error| error.to_string())?;
        let restored = tx
            .execute(
                "INSERT INTO objects (page, id, kind, layer, z, codec, payload)
                 SELECT page, id, kind, layer, z, codec, payload
                 FROM object_history WHERE page = ?1 AND stamp = ?2",
                params![page, stamp],
            )
            .map_err(|error| error.to_string())?;
        prune_in(&tx, page)?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(Some(restored))
    }
}

/// Copies the page's objects into `object_history` under a stamp newer than every existing one (two edits in
/// one millisecond must not collide). Returns how many objects were copied.
fn archive_in(tx: &Transaction<'_>, page: &str) -> Result<usize, String> {
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(0);
    let newest: Option<i64> = tx
        .query_row(
            "SELECT MAX(stamp) FROM object_history WHERE page = ?1",
            params![page],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let stamp = newest.map_or(now, |newest| now.max(newest.saturating_add(1)));
    tx.execute(
        "INSERT INTO object_history (page, stamp, id, kind, layer, z, codec, payload)
         SELECT page, ?2, id, kind, layer, z, codec, payload FROM objects WHERE page = ?1",
        params![page, stamp],
    )
    .map_err(|error| error.to_string())
}

/// Keeps only the newest [`HISTORY_KEEP`] archives of a page.
fn prune_in(tx: &Transaction<'_>, page: &str) -> Result<(), String> {
    tx.execute(
        "DELETE FROM object_history WHERE page = ?1 AND stamp NOT IN (
             SELECT DISTINCT stamp FROM object_history WHERE page = ?1
             ORDER BY stamp DESC LIMIT ?2
         )",
        params![page, HISTORY_KEEP],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}
