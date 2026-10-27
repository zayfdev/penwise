//! What a migration did (printed by `penwise-server migrate`).

use std::fmt;
use std::path::PathBuf;

/// Per legacy page counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageCount {
    pub legacy_id: String,
    pub migrated: u64,
    pub unmigrated: u64,
}

/// Counts of one v3 → v4 run (also produced by `--dry-run`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// The database was already v4: nothing was done.
    pub already_current: bool,
    pub dry_run: bool,
    /// `VACUUM INTO` copy of the v3 database (not made by a dry run).
    pub backup: Option<PathBuf>,
    pub notebooks: u64,
    pub pages: u64,
    /// Rows of the v3 `objects` table.
    pub objects_rows: u64,
    pub strokes: u64,
    pub images: u64,
    /// Rows quarantined into `legacy_unmigrated`.
    pub unmigrated: u64,
    pub history_rows: u64,
    pub anchors_rows: u64,
    pub ink_anchor_rows: u64,
    /// Ops written to the log.
    pub ops: u64,
    /// Titles or page kinds that had to be repaired to be valid (blank notebook title, invalid page size).
    pub adjusted: u64,
    pub per_page: Vec<PageCount>,
}

impl fmt::Display for MigrationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.already_current {
            return writeln!(f, "already schema v4: nothing to do");
        }
        let mode = if self.dry_run {
            "dry run (nothing written)"
        } else {
            "migrated"
        };
        writeln!(f, "v3 -> v4: {mode}")?;
        if let Some(backup) = &self.backup {
            writeln!(f, "backup:        {}", backup.display())?;
        }
        writeln!(f, "notebooks:     {}", self.notebooks)?;
        writeln!(f, "pages:         {}", self.pages)?;
        writeln!(f, "objects rows:  {}", self.objects_rows)?;
        writeln!(f, "  strokes:     {}", self.strokes)?;
        writeln!(f, "  images:      {}", self.images)?;
        writeln!(
            f,
            "  unmigrated:  {} (kept in legacy_unmigrated)",
            self.unmigrated
        )?;
        writeln!(
            f,
            "history rows: {}  (kept as legacy_object_history)",
            self.history_rows
        )?;
        writeln!(
            f,
            "anchors:       {}  ink_anchors: {}",
            self.anchors_rows, self.ink_anchor_rows
        )?;
        writeln!(f, "ops written:   {}", self.ops)?;
        writeln!(f, "repaired:      {}", self.adjusted)?;
        for page in &self.per_page {
            writeln!(
                f,
                "  page {}: migrated {} unmigrated {}",
                page.legacy_id, page.migrated, page.unmigrated
            )?;
        }
        Ok(())
    }
}
