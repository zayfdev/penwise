//! Server-side archives of a page's objects: the undo a client cannot reach.

use rusqlite::{OptionalExtension, params};

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
        let stamp = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_millis())
                .unwrap_or(0),
        )
        .unwrap_or(0);
        let archived = tx
            .execute(
                "INSERT INTO object_history (page, stamp, id, kind, layer, z, codec, payload)
                 SELECT page, ?2, id, kind, layer, z, codec, payload FROM objects WHERE page = ?1",
                params![page, stamp],
            )
            .map_err(|error| error.to_string())?;
        if archived > 0 {
            tx.execute(
                "DELETE FROM object_history WHERE page = ?1 AND stamp NOT IN (
                     SELECT DISTINCT stamp FROM object_history WHERE page = ?1
                     ORDER BY stamp DESC LIMIT ?2
                 )",
                params![page, HISTORY_KEEP],
            )
            .map_err(|error| error.to_string())?;
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
    pub(crate) fn restore(&self, page: &str, stamp: Option<i64>) -> Result<Option<usize>, String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let chosen: Option<i64> = match stamp {
            Some(stamp) => Some(stamp),
            None => conn
                .query_row(
                    "SELECT MAX(stamp) FROM object_history WHERE page = ?1",
                    params![page],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?
                .flatten(),
        };
        let Some(stamp) = chosen else {
            return Ok(None);
        };
        let tx = conn.transaction().map_err(|error| error.to_string())?;
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
        tx.commit().map_err(|error| error.to_string())?;
        Ok(Some(restored))
    }
}
