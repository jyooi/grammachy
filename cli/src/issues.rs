//! The guarantees every engine adapter owes the shell, in one place.
//!
//! Spec section 5.1 asks that Issues come out sorted by `start`, never overlap,
//! and never carry a `fix` equal to the `original`. Each adapter maps its own
//! engine answer to [`Issue`]s and then hands the list to [`normalise`], so the
//! guarantees are written once and every engine keeps them.

use crate::envelope::Issue;

/// Sort, drop the no-op Issues, and keep the earlier of two overlapping ones.
pub fn normalise(issues: Vec<Issue>) -> Vec<Issue> {
    let mut issues: Vec<Issue> = issues
        .into_iter()
        .filter(|issue| issue.fix != issue.original)
        .collect();

    // A shorter span first keeps the tighter of two Issues that start together.
    issues.sort_by_key(|issue| (issue.start, issue.end));

    let mut kept: Vec<Issue> = Vec::with_capacity(issues.len());
    for issue in issues {
        match kept.last() {
            Some(previous) if issue.start < previous.end => continue,
            _ => kept.push(issue),
        }
    }
    kept
}
