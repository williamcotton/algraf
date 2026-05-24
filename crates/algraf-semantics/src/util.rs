//! Small helpers shared by the analyzer.

/// Case-insensitive Levenshtein distance.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// The closest candidate to `name` within a small edit distance, if any.
pub fn closest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    // Allow a couple of edits, scaling up for longer names.
    let max = (name.chars().count() / 3).max(2);
    candidates
        .map(|c| (distance(name, c), c))
        .filter(|(d, _)| *d <= max)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}
