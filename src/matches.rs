use regex::bytes::Regex;
use regex_syntax::hir::Hir;
use regex_syntax::ParserBuilder;
use std::collections::HashSet;
use std::ops::Range;

/// Find byte ranges of `text` matched by every regex.
///
/// Each regex searches the whole of `text`, so the order of `regexes` does not matter.
///
/// # Arguments
/// * `text` - Text to search
/// * `regexes` - Regexes that must all match
///
/// # Returns
/// Ascending, non-overlapping byte ranges into `text`, without empty matches.
/// The whole of `text` if `regexes` is empty.
pub fn intersect_matches(text: &[u8], regexes: &[Regex]) -> Vec<Range<usize>> {
    let mut ranges = vec![Range { start: 0, end: text.len() }];
    for re in regexes {
        let matches: Vec<Range<usize>> = re.find_iter(text).map(|m| m.range()).collect();
        ranges = intersect_ranges(&ranges, &matches);
    }
    ranges
}

/// Intersect two sets of ranges.
///
/// # Arguments
/// * `a`, `b` - Ascending, non-overlapping ranges
///
/// # Returns
/// Ascending, non-overlapping, non-empty ranges covered by both `a` and `b`.
fn intersect_ranges(a: &[Range<usize>], b: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut overlaps = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let start = a[i].start.max(b[j].start);
        let end = a[i].end.min(b[j].end);
        if start < end {
            overlaps.push(start..end);
        }
        if a[i].end < b[j].end {
            i += 1;
        } else {
            j += 1;
        }
    }
    overlaps
}

/// Alternation of regex patterns. The flags of a pattern, e.g. `(?-u)`, apply only to that pattern.
///
/// # Arguments
/// * `patterns` - Regex patterns
///
/// # Returns
/// Regex that matches where one of `patterns` matches.
/// Where more than one pattern matches at a position, the match of the earliest pattern.
///
/// # Errors
/// The parse error of the first pattern that is invalid alone, e.g. `A)|(C`.
pub fn any_regex(patterns: &[String]) -> Result<Regex, regex::Error> {
    let mut hirs = Vec::new();
    for pattern in patterns {
        Regex::new(pattern)?;
        // Parse like `regex::bytes`, so a valid pattern parses.
        hirs.push(ParserBuilder::new().utf8(false).build().parse(pattern).expect("regex parsed the pattern"));
    }
    Regex::new(&Hir::alternation(hirs).to_string())
}

/// Regex for runs of at least `min_length` characters from `alphabet`.
///
/// # Arguments
/// * `alphabet` - Characters a run may contain
/// * `min_length` - Minimum run length in characters
///
/// # Returns
/// Regex that matches each run.
pub fn min_length_regex(alphabet: &HashSet<char>, min_length: usize) -> Regex {
    let re_alphabet: String = alphabet.iter().map(|c| regex::escape(&c.to_string())).collect();
    Regex::new(&format!("[{re_alphabet}]{{{min_length},}}")).unwrap()
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)] // Range sets with one range are intended.
mod tests {
    use super::*;

    fn regexes(patterns: &[&str]) -> Vec<Regex> {
        patterns.iter().map(|p| Regex::new(p).unwrap()).collect()
    }

    #[test]
    fn intersect_matches_without_regex_is_whole_text() {
        assert_eq!(intersect_matches(b"abc", &[]), vec![0..3]);
        assert_eq!(intersect_matches(b"", &[]), vec![0..0]);
    }

    #[test]
    fn intersect_matches_keeps_motif_inside_long_run() {
        let res = regexes(&["GAATTC", "[ACGT]{10,}"]);
        assert_eq!(intersect_matches(b"xxACGTGAATTCACGT", &res), vec![6..12]);
    }

    #[test]
    fn intersect_matches_anchors_to_text() {
        let res = regexes(&["ACGT", "^A"]);
        assert!(intersect_matches(b"AACGT", &res).is_empty());
    }

    #[test]
    fn intersect_matches_drops_empty_matches() {
        assert_eq!(intersect_matches(b"xAx", &regexes(&["A*"])), vec![1..2]);
    }

    #[test]
    fn intersect_ranges_edge_cases() {
        assert_eq!(intersect_ranges(&[0..4], &[2..4, 4..6]), vec![2..4]);
        assert_eq!(intersect_ranges(&[0..2, 2..4], &[1..3]), vec![1..2, 2..3]);
        assert!(intersect_ranges(&[0..4], &[2..2]).is_empty());
        assert!(intersect_ranges(&[], &[0..1]).is_empty());
    }

    #[test]
    fn intersect_matches_returns_byte_offsets() {
        let res = regexes(&["A+", "A"]);
        assert_eq!(intersect_matches("éAA".as_bytes(), &res), vec![2..3, 3..4]);
    }

    #[test]
    fn any_regex_matches_each_pattern() {
        let re = any_regex(&["A".into(), "(?-u)^\\xff".into()]).unwrap();
        assert_eq!(re.find_iter(b"\xffxA").map(|m| m.range()).collect::<Vec<_>>(), vec![0..1, 2..3]);
    }

    #[test]
    fn any_regex_keeps_comment_in_its_pattern() {
        let re = any_regex(&["(?x)A # comment".into(), "T".into()]).unwrap();
        assert_eq!(re.find_iter(b"AT").map(|m| m.range()).collect::<Vec<_>>(), vec![0..1, 1..2]);
    }

    #[test]
    fn any_regex_rejects_pattern_invalid_alone() {
        assert!(any_regex(&["A)|(C".into()]).is_err());
    }

    #[test]
    fn min_length_regex_escapes_alphabet() {
        assert!(min_length_regex(&HashSet::from(['\\', ']']), 2).is_match(b"\\]"));
        let re = min_length_regex(&HashSet::from(['A', '-']), 3);
        assert!(re.is_match(b"A-A"));
        assert!(!re.is_match(b"AB"));
        assert!(min_length_regex(&HashSet::from(['é']), 2).is_match("éé".as_bytes()));
    }
}
