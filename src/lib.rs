pub mod ansi_colors;
pub mod bio;
pub mod colorschemes;
pub mod consensus;
pub mod inout;
pub mod matches;
pub mod pager;

use regex::bytes::Regex;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::ops::Range;
use yansi::Style;

use crate::ansi_colors::{decode, is_styled, to_painted, write_line, Char};
use crate::bio::{nucl_log_odds, SeqType, MIN_ABS_LOG_ODDS};
use crate::matches::intersect_matches;

/// What to color, and how.
pub struct Config {
    pub styles: Styles,
    /// Color only text matched by every regex. Empty colors all text.
    pub regexes: Vec<Regex>,
    /// Swap columns and rows of the colored lines.
    pub transpose: bool,
    /// `None` disables highlighting.
    pub highlight: Option<Highlight>,
}

/// Style of each character.
pub enum Styles {
    /// The same styles for all text.
    One(HashMap<char, Style>),
    /// Styles for each sequence type. Each run of text to color gets the styles of its type.
    ByType { nucl: HashMap<char, Style>, aa: HashMap<char, Style> },
}

/// Consensus or mutation highlighting.
pub struct Highlight {
    /// Highlight the consensus if true, otherwise the mutations.
    pub consensus: bool,
    /// Style to lay over highlighted chars.
    pub style: Style,
    /// Characters to count for the consensus. `None` counts all.
    pub alphabet: Option<HashSet<char>>,
}

/// Write the lines of `readers` with ANSI styles.
///
/// Streams the input, unless `config` asks to transpose or highlight.
/// With [`Styles::ByType`], each run of text to color is classified by its letters,
/// and by the runs at the same position on other lines of its reader.
/// So output can wait for the first lines of each reader.
/// Invalid UTF-8 is written unchanged.
/// A read error skips the rest of its reader.
/// A closed `output` ends the reading without error.
///
/// # Arguments
/// * `readers` - Input, read in order, each with a name for its errors
/// * `output` - Destination of the styled lines
/// * `config` - What to color, and how
///
/// # Returns
/// The read errors, each message prefixed by `"{name}: "`.
///
/// # Errors
/// The first write error other than a closed `output`.
pub fn run(
    readers: Vec<(String, Box<dyn BufRead>)>,
    output: &mut dyn Write,
    config: &Config,
) -> io::Result<Vec<io::Error>> {
    let mut read_errors = Vec::new();
    let lines = readers
        .into_iter()
        .enumerate()
        .flat_map(|(i_reader, (name, reader))| {
            inout::read_lines(reader).map(move |line| {
                line.map(|line| (i_reader, line))
                    .map_err(|e| io::Error::new(e.kind(), format!("{name}: {e}")))
            })
        })
        .filter_map(|line| line.map_err(|e| read_errors.push(e)).ok());
    match write_styled(lines, &mut io::BufWriter::with_capacity(1 << 16, output), config) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e),
        _ => Ok(read_errors),
    }
}

/// Write `lines`, each with the index of its reader, with ANSI styles. See [`run`].
fn write_styled(
    lines: impl Iterator<Item = (usize, Vec<u8>)>,
    output: &mut dyn Write,
    config: &Config,
) -> io::Result<()> {
    let newline = b"\n";

    if !config.transpose && config.highlight.is_none() {
        // Streaming.
        paint_lines(lines, config, true, |line_painted| {
            write_line(output, &line_painted)?;
            output.write_all(newline)
        })?;
    } else {
        // Not streaming.
        let mut lines_painted: Vec<Vec<Char>> = Vec::new();
        paint_lines(lines, config, false, |line_painted| {
            lines_painted.push(line_painted);
            Ok(())
        })?;
        if let Some(highlight) = &config.highlight {
            consensus::highlight_consensus_or_mutations(
                &mut lines_painted,
                &highlight.alphabet,
                highlight.consensus,
                highlight.style,
            );
        }

        if !config.transpose {
            for painted_line in &lines_painted {
                write_line(output, painted_line)?;
                output.write_all(newline)?;
            }
        } else {
            // Transpose: only transpose lines with a sequence character (`Char::Styled`, even with an empty style).
            // Non-sequence lines are output normally before the transposed block.
            let mut seq_lines: Vec<&Vec<Char>> = Vec::new();
            let mut seq_max_len = 0;

            for painted_line in &lines_painted {
                let has_seq_char = painted_line.iter().any(|c| matches!(c, Char::Styled(_)));
                if has_seq_char {
                    seq_lines.push(painted_line);
                    seq_max_len = seq_max_len.max(painted_line.len());
                } else {
                    // Output non-sequence lines normally.
                    write_line(output, painted_line)?;
                    output.write_all(newline)?;
                }
            }

            // Transpose the sequence lines.
            for j in 0..seq_max_len {
                // Reset code to ensure clean state at line start (helps pagers like less
                // correctly render lines during scrolling/re-rendering).
                if is_styled(seq_lines.iter().filter_map(|line| line.get(j))) {
                    output.write_all(b"\x1B[0m")?;
                }
                for seq_line in &seq_lines {
                    match seq_line.get(j) {
                        None => output.write_all(b" ")?,
                        Some(ch) => ch.write(output)?,
                    };
                }
                output.write_all(newline)?;
            }
        }
    }

    output.flush()
}

/// Maximum number of lines to read before classifying the runs of a reader while streaming.
const MAX_READ_AHEAD_LINES: usize = 100;

/// A line and its runs of text to color.
struct LineRuns {
    line: Vec<u8>,
    /// Byte ranges of `line`
    runs: Vec<Range<usize>>,
    /// Log odds of nucleotides over amino acids for each run
    runs_log_odds: Vec<f64>,
}

/// Paint lines in order with the styles of `config`.
///
/// With [`Styles::ByType`], a run gets the type of [`SeqType::from_log_odds`],
/// with the runs at its position on the lines of its reader read so far as the other runs.
///
/// # Arguments
/// * `lines` - Each line with the index of its reader
/// * `config` - Styles, and regexes whose common matches are the runs to color
/// * `streaming` - Paint the first lines of a reader when the sum of the log odds at each run position decides a type,
///   or after `MAX_READ_AHEAD_LINES` lines, then paint each later line when it is read.
///   Else paint the lines of a reader after all of them are read.
/// * `emit` - Called with each painted line. Its first error stops the painting.
///
/// # Errors
/// The first error of `emit`.
fn paint_lines(
    lines: impl Iterator<Item = (usize, Vec<u8>)>,
    config: &Config,
    streaming: bool,
    mut emit: impl FnMut(Vec<Char>) -> io::Result<()>,
) -> io::Result<()> {
    let (nucl, aa) = match &config.styles {
        Styles::One(styles) => {
            for (_, line) in lines {
                let runs: Vec<_> = intersect_matches(&line, &config.regexes).into_iter().map(|r| (r, styles)).collect();
                emit(paint_line(&line, &runs))?;
            }
            return Ok(());
        }
        Styles::ByType { nucl, aa } => (nucl, aa),
    };
    let paint = |line: LineRuns, evidence: &Evidence| {
        let seq_types = evidence.seq_types(&line.runs_log_odds);
        let runs: Vec<_> = line
            .runs
            .into_iter()
            .zip(seq_types)
            .map(|(run, seq_type)| (run, match seq_type { SeqType::Nucl => nucl, SeqType::AA => aa }))
            .collect();
        paint_line(&line.line, &runs)
    };

    let mut lines = lines.peekable();
    while let Some(&(i_reader, _)) = lines.peek() {
        let mut reader_lines = std::iter::from_fn(|| lines.next_if(|(i, _)| *i == i_reader)).map(|(_, line)| {
            let runs = intersect_matches(&line, &config.regexes);
            let runs_log_odds = runs.iter().map(|run| nucl_log_odds(&line[run.clone()])).collect();
            LineRuns { line, runs, runs_log_odds }
        });

        let mut evidence = Evidence::default();
        let mut lines_ahead = Vec::new();
        for line in reader_lines.by_ref() {
            evidence.add(&line.runs_log_odds);
            lines_ahead.push(line);
            if streaming && (evidence.is_decided() || lines_ahead.len() >= MAX_READ_AHEAD_LINES) {
                break;
            }
        }
        for line in lines_ahead {
            emit(paint(line, &evidence))?;
        }
        for line in reader_lines {
            evidence.add(&line.runs_log_odds);
            emit(paint(line, &evidence))?;
        }
    }
    Ok(())
}

/// Sum of the log odds of the runs at each position of a line.
///
/// A position is the number of runs on a line and the index of a run on it,
/// so the runs of a column of a table add up, and a row with a missing cell does not take evidence from other columns.
#[derive(Default)]
struct Evidence(HashMap<(usize, usize), f64>);

impl Evidence {
    /// Add the runs of a line. Add each line once.
    ///
    /// # Arguments
    /// * `runs_log_odds` - Log odds of nucleotides over amino acids for each run of a line
    fn add(&mut self, runs_log_odds: &[f64]) {
        for (i, &log_odds) in runs_log_odds.iter().enumerate() {
            *self.0.entry((runs_log_odds.len(), i)).or_default() += log_odds;
        }
    }

    /// Sequence type of each run of a line that was added. See [`SeqType::from_log_odds`].
    ///
    /// # Arguments
    /// * `runs_log_odds` - Log odds of each run of the line
    ///
    /// # Returns
    /// The type of each run, with the other runs at its position as evidence.
    fn seq_types(&self, runs_log_odds: &[f64]) -> Vec<SeqType> {
        runs_log_odds
            .iter()
            .enumerate()
            .map(|(i, &run)| SeqType::from_log_odds(run, self.0[&(runs_log_odds.len(), i)] - run))
            .collect()
    }

    /// Whether a position was added and the sum of each position decides a sequence type.
    fn is_decided(&self) -> bool {
        !self.0.is_empty() && self.0.values().all(|log_odds| log_odds.abs() >= MIN_ABS_LOG_ODDS)
    }
}

/// Paint the chars of a line. Chars inside the runs are styled, other chars are unstyled.
///
/// # Arguments
/// * `line` - Bytes, decoded as UTF-8. Invalid sequences become [`Char::Invalid`].
/// * `runs` - Ascending, non-overlapping byte ranges of `line`, each with the style of each char
///
/// # Returns
/// The chars of `line`.
fn paint_line(line: &[u8], runs: &[(Range<usize>, &HashMap<char, Style>)]) -> Vec<Char> {
    let mut line_painted = Vec::with_capacity(line.len());
    let mut i = 0;
    for (run, styles) in runs {
        line_painted.extend(decode(&line[i..run.start], Char::Unstyled));
        line_painted.extend(to_painted(styles, &line[run.clone()]));
        i = run.end;
    }
    line_painted.extend(decode(&line[i..], Char::Unstyled));
    line_painted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inout::tests::FailingReader;
    use yansi::Color::{Blue, Red};

    #[test]
    fn evidence_gives_undecided_run_the_type_of_other_runs() {
        let mut evidence = Evidence::default();
        evidence.add(&[-20.0]);
        evidence.add(&[1.0]);
        assert_eq!(evidence.seq_types(&[1.0]), [SeqType::AA]);
    }

    #[test]
    fn evidence_does_not_count_a_run_twice() {
        let mut evidence = Evidence::default();
        evidence.add(&[3.0]);
        evidence.add(&[-2.0]);
        assert_eq!(evidence.seq_types(&[-2.0]), [SeqType::Nucl]);
    }

    #[test]
    fn evidence_is_by_run_position() {
        let mut evidence = Evidence::default();
        evidence.add(&[-20.0, -20.0]);
        evidence.add(&[1.0]);
        assert_eq!(evidence.seq_types(&[1.0]), [SeqType::Nucl]);
    }

    #[test]
    fn run_colors_each_run_by_the_type_of_its_reader() {
        let config = Config {
            styles: Styles::ByType {
                nucl: HashMap::from([('A', Red.background())]),
                aa: HashMap::from([('A', Blue.background())]),
            },
            regexes: vec![],
            transpose: false,
            highlight: None,
        };
        let readers: Vec<(String, Box<dyn BufRead>)> = vec![
            ("dna".into(), Box::new(&b"ACGTACGT\nA\n"[..])),
            ("prot".into(), Box::new(&b"A\nLLLLLLLL\n"[..])),
            ("short".into(), Box::new(&b"A\n"[..])),
        ];
        let mut output = Vec::new();
        run(readers, &mut output, &config).unwrap();
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        let (red_a, blue_a) = ("\x1b[0m\x1b[41mA\x1b[0m", "\x1b[0m\x1b[44mA\x1b[0m");
        assert_eq!([lines[1], lines[2], lines[4]], [red_a, blue_a, red_a], "{output:?}");
    }

    fn run_to_bytes(input: &'static [u8], transpose: bool) -> Vec<u8> {
        let config = Config {
            styles: Styles::One(HashMap::from([('A', Red.background())])),
            regexes: vec![Regex::new("(?-u)^[^>@+].*").unwrap()],
            transpose,
            highlight: None,
        };
        let mut output = Vec::new();
        run(vec![("-".into(), Box::new(input))], &mut output, &config).unwrap();
        output
    }

    #[test]
    fn run_streams_styled_lines() {
        assert_eq!(run_to_bytes(b">h\n\nAC\n", false), b">h\n\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }

    #[test]
    fn run_streams_invalid_utf8_unchanged() {
        assert_eq!(
            run_to_bytes(b">caf\xe9\nA\xffA\n", false),
            b">caf\xe9\n\x1b[0m\x1b[41mA\x1b[0m\xff\x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_invalid_utf8_unchanged() {
        assert_eq!(run_to_bytes(b"\xe2\x82A\n", true), b"\xe2\x82\n\x1b[0m\x1b[41mA\x1b[0m\n");
    }

    fn bad_and_good() -> Vec<(String, Box<dyn BufRead>)> {
        vec![("bad".into(), Box::new(io::BufReader::new(FailingReader))), ("good".into(), Box::new(&b"GT\n"[..]))]
    }

    fn messages(errors: Vec<io::Error>) -> Vec<String> {
        errors.iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn run_returns_read_errors_and_continues() {
        for transpose in [false, true] {
            let config = Config { styles: Styles::One(HashMap::new()), regexes: vec![], transpose, highlight: None };
            let mut output = Vec::new();
            let read_errors = run(bad_and_good(), &mut output, &config).unwrap();
            let mut output_valid = Vec::new();
            run(vec![("good".into(), Box::new(&b"GT\n"[..]))], &mut output_valid, &config).unwrap();
            assert_eq!(messages(read_errors), ["bad: read failed"]);
            assert_eq!(output, output_valid);
            assert_eq!(messages(run(bad_and_good(), &mut ClosedWriter, &config).unwrap()), ["bad: read failed"]);
        }
    }

    struct ClosedWriter;

    impl Write for ClosedWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn run_transposes_styled_lines() {
        assert_eq!(
            run_to_bytes(b">h\nAC\nCA\n", true),
            b">h\n\x1b[0m\x1b[41mA\x1b[0mC\n\x1b[0mC\x1b[41mA\x1b[0m\n",
        );
    }
}
