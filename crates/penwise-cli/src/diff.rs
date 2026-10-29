//! Line diff for the adapter confirmation (`penwise link` shows the exact change before applying it).
//!
//! Config files are small, so a plain longest-common-subsequence walk is plenty; the output is the unified
//! format people already read, with three lines of context.

use std::fmt::Write as _;
use std::path::Path;

/// Context lines around each change.
const CONTEXT: usize = 3;

/// One line of the diff: `' '` unchanged, `'-'` removed, `'+'` added.
struct Op<'a> {
    tag: char,
    text: &'a str,
    old: usize,
    new: usize,
}

/// A unified diff (headers included) for `path`, or headers only when the texts are equal.
#[must_use]
pub fn unified(path: &Path, before: &str, after: &str) -> String {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let ops = walk(&old, &new);
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.tag != ' ')
        .map(|(index, _)| index)
        .collect();
    let mut out = format!(
        "--- {}\n+++ {} (penwise adapter)\n",
        path.display(),
        path.display()
    );
    for hunk in hunks(&changed, ops.len()) {
        let slice = &ops[hunk.clone()];
        let old_start = slice.iter().find(|op| op.tag != '+').map_or(0, |op| op.old);
        let new_start = slice.iter().find(|op| op.tag != '-').map_or(0, |op| op.new);
        let old_count = slice.iter().filter(|op| op.tag != '+').count();
        let new_count = slice.iter().filter(|op| op.tag != '-').count();
        writeln!(
            out,
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@"
        )
        .ok();
        for op in slice {
            out.push(op.tag);
            out.push_str(op.text);
            out.push('\n');
        }
    }
    out
}

/// The changed-op indices grouped into ranges padded with [`CONTEXT`] lines; close hunks merge.
fn hunks(changed: &[usize], total: usize) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for &index in changed {
        let start = index.saturating_sub(CONTEXT);
        let end = (index + CONTEXT + 1).min(total);
        if let Some(last) = ranges.last_mut()
            && start <= last.end
        {
            last.end = last.end.max(end);
            continue;
        }
        ranges.push(start..end);
    }
    ranges
}

/// The edit script for two line lists: unchanged lines first where equal.
fn walk<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
    let table = table(old, new);
    let mut ops = Vec::with_capacity(old.len() + new.len());
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            ops.push(Op {
                tag: ' ',
                text: old[i],
                old: i + 1,
                new: j + 1,
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(Op {
                tag: '-',
                text: old[i],
                old: i + 1,
                new: 0,
            });
            i += 1;
        } else {
            ops.push(Op {
                tag: '+',
                text: new[j],
                old: 0,
                new: j + 1,
            });
            j += 1;
        }
    }
    for (index, line) in old.iter().enumerate().skip(i) {
        ops.push(Op {
            tag: '-',
            text: line,
            old: index + 1,
            new: 0,
        });
    }
    for (index, line) in new.iter().enumerate().skip(j) {
        ops.push(Op {
            tag: '+',
            text: line,
            old: 0,
            new: index + 1,
        });
    }
    ops
}

/// `table[i][j]` is the LCS length of `old[i..]` and `new[j..]`.
fn table(old: &[&str], new: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}

#[cfg(test)]
mod tests;
