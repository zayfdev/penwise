//! Which marks belong to which round (spec v2 §4): the project's marks with the op-log seq that added each,
//! read against the `rounds` rows. Round *n* holds the marks added after round *n−1*'s `sent_seq`, up to its
//! own; the open round holds everything after the last `sent_seq`.
//!
//! The scan reads only the project's own pages (its notebook, else its board) and only their frame and mark
//! rows, so the inbox stays quick on a library with thousands of strokes.

use std::collections::BTreeMap;

use penwise_core::item::{FramePreset, ItemId, ItemKind};
use penwise_core::marks::{MarkEntry, MarkStatus};
use penwise_proto::{NotebookId, PageId};

use crate::api::{ApiError, AppState};
use crate::frames::registry::Project;
use crate::storage::{LedgerRow, Round, RoundStatus};

/// One mark of the project, placed in time.
#[derive(Debug, Clone)]
pub(crate) struct RoundMark {
    pub(crate) page: PageId,
    /// The mark without a placement (`rect` is `None`): the ledger never opens a page.
    pub(crate) entry: MarkEntry,
    /// The op-log seq of the op that added the mark item.
    pub(crate) added_seq: u64,
    /// The op-log seq of the op that last changed it (a status edit moves it).
    pub(crate) updated_seq: u64,
    pub(crate) preset: FramePreset,
    pub(crate) route: String,
}

/// A project's rounds and marks, as of one read.
#[derive(Debug)]
pub(crate) struct Ledger {
    rounds: Vec<Round>,
    marks: Vec<RoundMark>,
}

impl Ledger {
    /// Reads the rows and the frame and mark rows of the project's pages.
    ///
    /// # Errors
    /// 500 when storage fails.
    pub(crate) async fn load(app: &AppState, project: &str) -> Result<Self, ApiError> {
        let rounds = app.storage.rounds(project.to_owned()).await?;
        let mut marks = Vec::new();
        if let Some(found) = app.projects.get(project).await? {
            for page in project_pages(app, &found).await? {
                let rows = app.storage.frames_and_marks(page).await?;
                marks.extend(marks_of_rows(page, project, rows));
            }
        }
        Ok(Self { rounds, marks })
    }

    pub(crate) fn rounds(&self) -> &[Round] {
        &self.rounds
    }

    pub(crate) fn round(&self, n: u32) -> Option<&Round> {
        self.rounds.iter().find(|round| round.n == n)
    }

    /// The open round's number: its row, else the number after the last round (1 for a new project).
    pub(crate) fn open_number(&self) -> u32 {
        self.rounds
            .iter()
            .find(|round| round.status == RoundStatus::Open)
            .map_or_else(
                || self.rounds.iter().map(|round| round.n).max().unwrap_or(0) + 1,
                |round| round.n,
            )
    }

    /// Round `n`'s status; a round without a row is the open one.
    pub(crate) fn status_of(&self, n: u32) -> RoundStatus {
        self.round(n)
            .map_or(RoundStatus::Open, |round| round.status)
    }

    /// The latest sent round that is not resolved yet.
    pub(crate) fn latest_sent(&self) -> Option<&Round> {
        self.rounds
            .iter()
            .rev()
            .find(|round| round.status == RoundStatus::Sent)
    }

    /// The round `look` shows: the latest sent-but-not-resolved one, else the open one.
    pub(crate) fn current(&self) -> u32 {
        self.latest_sent()
            .map_or_else(|| self.open_number(), |round| round.n)
    }

    /// The mark `id`, whichever round it is in.
    pub(crate) fn find(&self, id: ItemId) -> Option<&RoundMark> {
        self.marks.iter().find(|mark| mark.entry.id == id)
    }

    /// The seq window `(after, up_to]` of round `n`'s marks.
    fn window(&self, n: u32) -> (u64, u64) {
        let after = n
            .checked_sub(1)
            .and_then(|previous| self.round(previous))
            .and_then(|round| round.sent_seq)
            .unwrap_or(0);
        let upto = self
            .round(n)
            .filter(|round| round.status != RoundStatus::Open)
            .and_then(|round| round.sent_seq)
            .unwrap_or(u64::MAX);
        (after, upto)
    }

    /// The marks of round `n`, oldest first.
    pub(crate) fn marks_of(&self, n: u32) -> Vec<&RoundMark> {
        let (after, upto) = self.window(n);
        let mut found: Vec<&RoundMark> = self
            .marks
            .iter()
            .filter(|mark| mark.added_seq > after && mark.added_seq <= upto)
            .collect();
        found.sort_by_key(|mark| (mark.added_seq, mark.entry.id));
        found
    }

    /// The marks drawn after round `n` was sent (its `sent_seq`), oldest first. Empty while `n` is open.
    pub(crate) fn marks_after(&self, n: u32) -> Vec<&RoundMark> {
        let (_, upto) = self.window(n);
        let mut found: Vec<&RoundMark> = self
            .marks
            .iter()
            .filter(|mark| mark.added_seq > upto)
            .collect();
        found.sort_by_key(|mark| (mark.added_seq, mark.entry.id));
        found
    }

    /// Every reopened mark, whichever round it was drawn in: the owner is asking again.
    pub(crate) fn reopened(&self) -> Vec<&RoundMark> {
        let mut found: Vec<&RoundMark> = self
            .marks
            .iter()
            .filter(|mark| mark.entry.status == MarkStatus::Reopened)
            .collect();
        found.sort_by_key(|mark| (mark.added_seq, mark.entry.id));
        found
    }
}

/// The pages a project's marks live on: its notebook's pages, else its board page.
async fn project_pages(app: &AppState, project: &Project) -> Result<Vec<PageId>, ApiError> {
    let catalog = app.storage.catalog().await?;
    let notebook = project
        .notebook
        .as_deref()
        .and_then(|text| text.parse::<NotebookId>().ok())
        .filter(|id| catalog.notebook(*id).is_some_and(|nb| !nb.deleted));
    if let Some(notebook) = notebook {
        return Ok(catalog
            .pages(notebook)
            .into_iter()
            .filter(|page| !page.deleted)
            .map(|page| page.id)
            .collect());
    }
    Ok(crate::agent::board_page(app, project)
        .await?
        .into_iter()
        .collect())
}

/// The marks among `rows` that sit on a frame of `project`.
fn marks_of_rows(page: PageId, project: &str, rows: Vec<LedgerRow>) -> Vec<RoundMark> {
    let mut frames: BTreeMap<ItemId, (FramePreset, String, bool)> = BTreeMap::new();
    for row in &rows {
        if let ItemKind::Frame(frame) = &row.item.kind {
            frames.insert(
                row.item.id,
                (frame.preset, frame.route.clone(), frame.project == project),
            );
        }
    }
    rows.into_iter()
        .filter_map(|row| {
            let ItemKind::Mark(mark) = &row.item.kind else {
                return None;
            };
            let (preset, route, ours) = frames.get(&mark.frame)?;
            ours.then(|| RoundMark {
                page,
                entry: MarkEntry {
                    id: row.item.id,
                    frame: mark.frame,
                    preset: mark.preset,
                    status: mark.status,
                    state: mark.state.clone(),
                    strokes: mark.strokes.clone(),
                    rect: None,
                },
                added_seq: row.added_seq,
                updated_seq: row.updated_seq,
                preset: *preset,
                route: route.clone(),
            })
        })
        .collect()
}
