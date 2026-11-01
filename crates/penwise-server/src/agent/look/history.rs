//! The `history:` line of `look`: what the last closed round of a lane kept.

use std::collections::BTreeMap;

use crate::api::{ApiError, AppState};
use crate::storage::{Generation, GenerationRole};

/// A lane: `(preset, route)`.
pub(super) type Lane = (String, String);

/// `history: ...` text per lane that has a past generation, from the project's stored generations.
pub(super) async fn lines(
    app: &AppState,
    project: &str,
) -> Result<BTreeMap<Lane, String>, ApiError> {
    let rows = app.storage.generations(project.to_owned(), None).await?;
    Ok(summarize(&rows))
}

/// One line per lane for its newest round: `history: round 3 picked "A" (2 variants)`.
pub(super) fn summarize(rows: &[Generation]) -> BTreeMap<Lane, String> {
    let mut newest: BTreeMap<Lane, u32> = BTreeMap::new();
    for row in rows {
        let round = newest
            .entry((row.preset.clone(), row.route.clone()))
            .or_insert(row.round);
        *round = (*round).max(row.round);
    }
    newest
        .into_iter()
        .map(|(lane, round)| {
            let variants: Vec<&Generation> = rows
                .iter()
                .filter(|row| row.round == round && row.role == GenerationRole::Variant)
                .filter(|row| row.preset == lane.0 && row.route == lane.1)
                .collect();
            let line = line_of(round, &variants);
            (lane, line)
        })
        .collect()
}

fn line_of(round: u32, variants: &[&Generation]) -> String {
    let count = match variants.len() {
        0 => "no variants".to_owned(),
        1 => "1 variant".to_owned(),
        many => format!("{many} variants"),
    };
    match variants.iter().find(|row| row.picked) {
        Some(pick) => format!("history: round {round} picked \"{}\" ({count})", pick.label),
        None if variants.is_empty() => format!("history: round {round} ({count})"),
        None => format!("history: round {round} no pick ({count})"),
    }
}
