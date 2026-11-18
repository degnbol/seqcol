use std::collections::{HashMap, HashSet};

use yansi::{Painted, Style};

use crate::ansi_colors::{overlay, Char};

/// Count character occurrences at each position across all lines.
/// Only counts styled characters, optionally filtering by alphabet.
fn count_chars(
    lines_painted: &[Vec<Char>],
    max_line: usize,
    alphabet: &Option<HashSet<char>>,
) -> Vec<HashMap<char, i32>> {
    let mut letter_counts: Vec<HashMap<char, i32>> = Vec::with_capacity(max_line);
    for _ in 0..max_line {
        letter_counts.push(HashMap::new());
    }

    for painted_line in lines_painted {
        for (i, ch) in painted_line.iter().enumerate() {
            // Only include what is styled, which will effectively apply the regex filters.
            if let Char::Styled(painted) = ch {
                let counts = &mut letter_counts[i];
                let c = painted.value;
                match counts.get(&c) {
                    None => match alphabet {
                        None => {
                            counts.insert(c, 1);
                        }
                        Some(alpha) => {
                            if alpha.contains(&c) {
                                counts.insert(c, 1);
                            }
                        }
                    },
                    Some(n) => {
                        counts.insert(c, n + 1);
                    }
                };
            }
        }
    }

    letter_counts
}

/// Compute consensus character at each position.
/// Returns None for positions with ties or no counts.
fn compute_consensus(letter_counts: &[HashMap<char, i32>], max_line: usize) -> Vec<Option<char>> {
    let mut consensus: Vec<Option<char>> = Vec::with_capacity(max_line);

    for counts in letter_counts.iter().take(max_line) {
        let mut best: Option<char> = None;
        let mut max = 0;
        let mut tie = false;

        for (c, n) in counts.iter() {
            if *n > max {
                max = *n;
                best = Some(*c);
                tie = false;
            } else if *n == max && best.is_some() {
                tie = true;
            }
        }

        if tie {
            best = None;
        }
        consensus.push(best);
    }

    consensus
}

/// Collect references to painted chars that should be highlighted.
/// If `highlight_consensus` is true, collects consensus chars; otherwise collects mutations.
fn collect_chars_to_highlight<'a>(
    lines_painted: &'a mut [Vec<Char>],
    consensus: &[Option<char>],
    highlight_consensus: bool,
) -> Vec<&'a mut Painted<char>> {
    let mut to_highlight = vec![];

    for painted_line in lines_painted {
        for (i, ch) in painted_line.iter_mut().enumerate() {
            if let Some(cons_char) = consensus[i]
                && let Char::Styled(painted) = ch
            {
                let is_consensus = cons_char == painted.value;
                if is_consensus == highlight_consensus {
                    to_highlight.push(painted);
                }
            }
        }
    }

    to_highlight
}

/// Highlight consensus or mutation characters in the painted lines.
///
/// # Arguments
/// * `lines_painted` - Mutable reference to the painted character lines
/// * `max_line` - Maximum line length
/// * `alphabet` - Optional set of characters to consider for consensus
/// * `highlight_consensus` - If true, highlight consensus; if false, highlight mutations
/// * `style` - Style for highlighted chars. Where it has no color, the char keeps its color.
pub fn highlight_consensus_or_mutations(
    lines_painted: &mut [Vec<Char>],
    max_line: usize,
    alphabet: &Option<HashSet<char>>,
    highlight_consensus: bool,
    style: Style,
) {
    let letter_counts = count_chars(lines_painted, max_line, alphabet);
    let consensus = compute_consensus(&letter_counts, max_line);
    for painted in collect_chars_to_highlight(lines_painted, &consensus, highlight_consensus) {
        painted.style = overlay(painted.style, style);
    }
}
