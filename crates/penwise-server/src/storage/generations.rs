//! Kept generations (locked-frames contract §2.2): the pictures a resolved round left on the board, per lane.
//!
//! One row per picture. A freeze records its rows just before it commits its board edits, inside the project's
//! resolve lock; if that commit fails the resolve is retried, and recording replaces every row of the lane and
//! round it names, so a retry over a changed board leaves no stale rows. Rows are server-local and never synced.
//! Only rounds that are `resolved` are listed.

use rusqlite::{Connection, params};

use super::error::StorageError;

/// Whether a picture is the lane's live frame before the round or one of its variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationRole {
    Og,
    Variant,
}

impl GenerationRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Og => "og",
            Self::Variant => "variant",
        }
    }

    fn parse(text: &str) -> Result<Self, StorageError> {
        match text {
            "og" => Ok(Self::Og),
            "variant" => Ok(Self::Variant),
            other => Err(StorageError::Corrupt(format!("generation role {other}"))),
        }
    }
}

/// One `generations` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub project: String,
    pub round: u32,
    pub route: String,
    pub preset: String,
    pub role: GenerationRole,
    /// The variant's label; empty for the OG picture.
    pub label: String,
    /// The variant the owner had chosen when the round resolved.
    pub picked: bool,
    /// The Image item on the board, 32 hex characters.
    pub item: String,
    /// The asset id of the picture's bytes.
    pub asset: String,
    pub w: u32,
    pub h: u32,
    /// When the round resolved (read side only; ignored when recording).
    pub resolved_ms: u64,
}

/// Writes `rows` in one transaction. First every existing row of each `(project, round, route, preset)` the rows
/// name is deleted, so recording a lane again replaces it as a whole.
pub(crate) fn record(conn: &mut Connection, rows: &[Generation]) -> Result<(), StorageError> {
    let tx = conn.transaction()?;
    let lanes: std::collections::BTreeSet<(&str, u32, &str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row.project.as_str(),
                row.round,
                row.route.as_str(),
                row.preset.as_str(),
            )
        })
        .collect();
    for (project, round, route, preset) in lanes {
        tx.execute(
            "DELETE FROM generations WHERE project = ?1 AND round = ?2 AND route = ?3 AND preset = ?4",
            params![project, i64::from(round), route, preset],
        )?;
    }
    for row in rows {
        tx.execute(
            "INSERT OR REPLACE INTO generations
               (project, round, route, preset, role, label, picked, item, asset, w, h)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                row.project,
                i64::from(row.round),
                row.route,
                row.preset,
                row.role.as_str(),
                row.label,
                i64::from(row.picked),
                row.item,
                row.asset,
                i64::from(row.w),
                i64::from(row.h),
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// The rows of resolved rounds of `project` (of one `(route, preset)` lane when given), newest round first;
/// within a round the OG row first, then variants by label.
pub(crate) fn list(
    conn: &Connection,
    project: &str,
    lane: Option<&(String, String)>,
) -> Result<Vec<Generation>, StorageError> {
    let (route, preset) = lane.map_or((None, None), |(r, p)| (Some(r.as_str()), Some(p.as_str())));
    let mut stmt = conn.prepare(
        "SELECT g.project, g.round, g.route, g.preset, g.role, g.label, g.picked, g.item, g.asset,
                g.w, g.h, r.resolved_ms
         FROM generations g
         JOIN rounds r ON r.project = g.project AND r.n = g.round AND r.status = 2
         WHERE g.project = ?1 AND (?2 IS NULL OR g.route = ?2) AND (?3 IS NULL OR g.preset = ?3)
         ORDER BY g.round DESC, g.route, g.preset, g.role = 'variant', g.label, g.item",
    )?;
    let mut rows = stmt.query(params![project, route, preset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Generation {
            project: row.get(0)?,
            round: u32::try_from(row.get::<_, i64>(1)?).unwrap_or(u32::MAX),
            route: row.get(2)?,
            preset: row.get(3)?,
            role: GenerationRole::parse(&row.get::<_, String>(4)?)?,
            label: row.get(5)?,
            picked: row.get::<_, i64>(6)? != 0,
            item: row.get(7)?,
            asset: row.get(8)?,
            w: u32::try_from(row.get::<_, i64>(9)?).unwrap_or(0),
            h: u32::try_from(row.get::<_, i64>(10)?).unwrap_or(0),
            resolved_ms: super::rows::to_u64(row.get::<_, Option<i64>>(11)?.unwrap_or(0)),
        });
    }
    Ok(out)
}
