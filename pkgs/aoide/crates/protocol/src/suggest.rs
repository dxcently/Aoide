//! The one edit-distance matcher behind every "did you mean": command paths,
//! flags, enum values. Hand-rolled (no `strsim`) so the lockfile stays tiny.

/// Optimal-string-alignment distance over chars: Levenshtein plus an adjacent
/// transposition as one edit, so `wiat` is one step from `wait`.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        rows[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (rows[i - 1][j - 1] + cost).min(rows[i - 1][j] + 1).min(rows[i][j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = d;
        }
    }
    rows[a.len()][b.len()]
}

/// The closest candidates to a mistyped `target`, nearest first, at most
/// `limit`. Close means a typo, not a different word: the cutoff grows with
/// the target's length (one edit up to 7 chars, two up to 11, then a quarter
/// of it, capped at three). Ties keep candidate order.
pub fn closest<'a>(target: &str, candidates: impl IntoIterator<Item = &'a str>, limit: usize) -> Vec<&'a str> {
    let target = target.to_lowercase();
    let cutoff = (target.chars().count() / 4).clamp(1, 3);
    let mut scored: Vec<(usize, &str)> = candidates
        .into_iter()
        .map(|c| (distance(&target, &c.to_lowercase()), c))
        .filter(|(d, _)| *d <= cutoff)
        .collect();
    scored.sort_by_key(|(d, _)| *d);
    scored.dedup_by_key(|(_, c)| *c);
    scored.into_iter().take(limit).map(|(_, c)| c).collect()
}

/// The lead phrase of a prose description: up to the first sentence end,
/// semicolon, colon or parenthesis, first letter lowered so it reads inside a
/// sentence of ours.
pub fn lead(desc: &str) -> String {
    let end = [". ", "; ", ": ", " (", " — ", " -- ", "\n"]
        .iter()
        .filter_map(|sep| desc.find(sep))
        .min()
        .unwrap_or(desc.len());
    let s = desc[..end].trim_end_matches('.').trim();
    let word = s.split_whitespace().next().unwrap_or_default();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if !word.chars().skip(1).any(char::is_uppercase) => c.to_lowercase().chain(chars).collect(),
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transposition_is_one_edit() {
        assert_eq!(distance("wiat", "wait"), 1);
        assert_eq!(distance("nodee", "node"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
    }

    #[test]
    fn only_typos_are_close() {
        assert_eq!(closest("wiat", ["wait", "yes", "code"], 2), vec!["wait"]);
        assert!(closest("id", ["to", "yes"], 2).is_empty(), "two edits on two letters is a different word");
        assert!(closest("zzzzzz", ["wait", "code"], 2).is_empty());
    }

    #[test]
    fn lead_stops_at_the_first_clause_and_lowers_the_initial() {
        assert_eq!(lead("Target session id (required unless --to is given); its socket"), "target session id");
        assert_eq!(lead("Seconds to block. Then stop."), "seconds to block");
        assert_eq!(lead("WxH, or one of: 16:9"), "WxH, or one of");
    }
}
