//! Cross-crate utility helpers with no syntax, data, render, or analyzer
//! dependencies.

/// Case-insensitive Unicode-aware Levenshtein distance.
pub fn edit_distance(a: &str, b: &str) -> usize {
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
        .map(|c| (edit_distance(name, c), c))
        .filter(|(d, _)| *d <= max)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// Return true for URI-scheme-like values while preserving single-letter
/// Windows drive prefixes such as `C:\...` as local paths.
pub fn is_url_like(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    if colon == 1 && value.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let scheme = &value[..colon];
    !scheme.is_empty()
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::{closest, edit_distance, is_url_like};

    #[test]
    fn closest_is_unicode_case_aware() {
        let candidates = ["S\u{00e3}oPaulo", "Zurich", "Bogota"];
        assert_eq!(
            closest("s\u{00e3}o-paulo", candidates.iter().copied()),
            Some("S\u{00e3}oPaulo")
        );
        assert_eq!(edit_distance("\u{00c4}xis", "\u{00e4}xes"), 1);
    }

    #[test]
    fn url_like_preserves_local_paths() {
        assert!(is_url_like("https://example.com/logo.png"));
        assert!(is_url_like("data:image/png;base64,abc"));
        assert!(is_url_like("foo+bar.v1-baz:asset"));
        assert!(!is_url_like("logos/team.png"));
        assert!(!is_url_like("/tmp/logo.png"));
        assert!(!is_url_like(r"C:\logos\team.png"));
    }
}
