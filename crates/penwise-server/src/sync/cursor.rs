//! Cursor arithmetic of one session: which part of a broadcast batch is new, and what a browser sees.

use penwise_proto::{AnyOp, ClientKind, SeqOp};

use super::store::OpFilter;

/// What to do with a broadcast batch given the last delivered seq.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Nothing new (empty, or entirely at or below the cursor).
    Skip,
    /// Deliver `batch[start..]`; it starts exactly at `cursor + 1`.
    Deliver { start: usize },
    /// The batch starts beyond `cursor + 1`: read the gap from storage.
    Gap,
}

/// Classifies a contiguous, ascending batch against `cursor`.
#[must_use]
pub fn classify(cursor: u64, batch: &[SeqOp]) -> Delivery {
    let (Some(first), Some(last)) = (batch.first(), batch.last()) else {
        return Delivery::Skip;
    };
    if last.seq <= cursor {
        return Delivery::Skip;
    }
    if first.seq > cursor.saturating_add(1) {
        return Delivery::Gap;
    }
    let start = batch.partition_point(|op| op.seq <= cursor);
    Delivery::Deliver { start }
}

/// True when `op` belongs in a session with `filter`.
#[must_use]
pub fn visible(filter: OpFilter, op: &SeqOp) -> bool {
    match filter {
        OpFilter::All => true,
        OpFilter::Catalog {
            page,
            page_from_seq,
        } => match &op.op {
            AnyOp::Notebook(_) | AnyOp::Page(_) => true,
            AnyOp::Item(item) => op.seq > page_from_seq && page == Some(item.page),
        },
    }
}

/// Filter of a session of `kind` watching `page` (snapshot taken at `page_from_seq`).
#[must_use]
pub fn filter_for(
    kind: ClientKind,
    page: Option<penwise_proto::PageId>,
    page_from_seq: u64,
) -> OpFilter {
    match kind {
        ClientKind::Tablet | ClientKind::Agent => OpFilter::All,
        ClientKind::Browser => OpFilter::Catalog {
            page,
            page_from_seq,
        },
    }
}
