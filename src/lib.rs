pub mod ansi_colors;
pub mod bio;
pub mod colorschemes;
pub mod consensus;
pub mod format;
pub mod inout;
pub mod matches;
pub mod pager;

use regex::bytes::Regex;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::ops::Range;
use yansi::{Painted, Style};

use crate::ansi_colors::{restrict, to_painted, write_line, Char, StyledText, Support};
use crate::bio::{nucl_log_odds, SeqType, MIN_ABS_LOG_ODDS};
use crate::format::{Format, NonSequenceLineFinder};
use crate::matches::intersect_matches;

/// What to color, and how.
pub struct Config {
    pub styles: Styles,
    /// Color only text matched by every regex. Empty colors all text.
    pub regexes: Vec<Regex>,
    /// Formats of the input. Lines that are not sequence in one of them get no matches.
    pub formats: Vec<Format>,
    /// Swap columns and rows of the colored lines.
    pub transpose: bool,
    /// `None` disables highlighting.
    pub highlight: Option<Highlight>,
    /// Styles the terminal can show. Input styles are restricted to it.
    pub support: Support,
}

/// Style of each character.
pub enum Styles {
    /// The same styles for all text.
    One(HashMap<char, Style>),
    /// Styles for each sequence type. Each match of `regexes` gets the styles of its type.
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
/// With [`Styles::ByType`], each match of `config.regexes` is classified by its letters,
/// and by the matches in the same column of its reader (see [`ColumnLogOdds`]).
/// So output can wait for the first lines of each reader.
/// Lines that are not sequence in one of `config.formats` get no matches.
/// CSI escape sequences are removed before matching. Text outside the matches keeps the styles of the SGR codes.
/// An SGR style continues on the next lines of its reader.
/// Invalid UTF-8 and line endings are written unchanged,
/// but a line without line ending gets `\n` if more output follows it.
/// Transposed rows get the line ending of the first transposed line.
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
    let support = config.support;
    let lines = readers
        .into_iter()
        .enumerate()
        .flat_map(|(i_reader, (name, reader))| {
            let non_sequence_line_finder = NonSequenceLineFinder::new(&config.formats);
            inout::read_lines(reader).scan((Style::new(), non_sequence_line_finder), move |state, line| {
                let (style_end, non_sequence_line_finder) = state;
                Some(
                    line.map(|(text, ending)| {
                        let mut text = StyledText::parse(text, *style_end);
                        *style_end = text.styles.last().unwrap().1;
                        for (_, style) in &mut text.styles {
                            *style = restrict(*style, support);
                        }
                        let excluded = non_sequence_line_finder.push(&text.text);
                        Line { i_reader, text, excluded, payload: ending }
                    })
                    .map_err(|e| io::Error::new(e.kind(), format!("{name}: {e}"))),
                )
            })
        })
        .filter_map(|line| line.map_err(|e| read_errors.push(e)).ok());
    match write_styled(lines, &mut io::BufWriter::with_capacity(1 << 16, output), config) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e),
        _ => Ok(read_errors),
    }
}

/// A line of input, and data that goes with it.
struct Line<T> {
    /// Index of the reader of the line
    i_reader: usize,
    text: StyledText,
    /// The line gets no matches
    excluded: bool,
    /// Data that goes with the line, unchanged
    payload: T,
}

/// Write `lines`, each with its line ending as payload, with ANSI styles. See [`run`].
/// Only the last line written can be without line ending.
fn write_styled(
    lines: impl Iterator<Item = Line<&'static [u8]>>,
    output: &mut dyn Write,
    config: &Config,
) -> io::Result<()> {
    let mut output = LineWriter { output, is_open: false };
    if !config.transpose && config.highlight.is_none() {
        // Streaming.
        paint_lines(lines, config, true, |line_painted, ending| output.write(&line_painted, ending))?;
    } else {
        // Not streaming.
        let mut lines_painted: Vec<Vec<Char>> = Vec::new();
        let mut endings: Vec<&[u8]> = Vec::new();
        paint_lines(lines, config, false, |line_painted, ending| {
            lines_painted.push(line_painted);
            endings.push(ending);
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
            for (painted_line, ending) in lines_painted.iter().zip(endings) {
                output.write(painted_line, ending)?;
            }
        } else {
            // Transpose only the lines with a match (`Char::Match`, even with an empty style).
            // Write the other lines unchanged before the transposed lines.
            let mut seq_lines: Vec<&Vec<Char>> = Vec::new();
            let mut seq_max_len = 0;
            let mut row_ending: &[u8] = b"";

            for (painted_line, ending) in lines_painted.iter().zip(endings) {
                let has_seq_char = painted_line.iter().any(|c| matches!(c, Char::Match(_)));
                if has_seq_char {
                    if seq_lines.is_empty() {
                        row_ending = ending;
                    }
                    seq_lines.push(painted_line);
                    seq_max_len = seq_max_len.max(painted_line.len());
                } else {
                    // Output non-sequence lines normally.
                    output.write(painted_line, ending)?;
                }
            }

            // Transpose the sequence lines. Each row gets the line ending of the first sequence line.
            let padding = Char::Text(Painted { value: ' ', style: Style::new() });
            for j in 0..seq_max_len {
                let row: Vec<Char> =
                    seq_lines.iter().map(|line| line.get(j).unwrap_or(&padding).clone()).collect();
                output.write(&row, row_ending)?;
            }
        }
    }

    output.output.flush()
}

/// Destination of lines, where only the last line can be without line ending.
struct LineWriter<'a> {
    output: &'a mut dyn Write,
    /// Whether the last written line has no line ending.
    is_open: bool,
}

impl LineWriter<'_> {
    /// Write a line with few escape codes (see [`write_line`]), then its line ending.
    /// If the previous line has no line ending, first write `\n`.
    ///
    /// # Arguments
    /// * `chars` - Chars of the line
    /// * `ending` - Line ending, or empty
    ///
    /// # Errors
    /// The first write error.
    fn write(&mut self, chars: &[Char], ending: &[u8]) -> io::Result<()> {
        if self.is_open {
            self.output.write_all(b"\n")?;
        }
        write_line(self.output, chars)?;
        self.output.write_all(ending)?;
        self.is_open = ending.is_empty();
        Ok(())
    }
}

/// Maximum number of lines to read before classifying the matches of a reader while streaming.
const MAX_READ_AHEAD_LINES: usize = 100;

/// A line, the byte ranges of it that all regexes match, and data that goes with the line.
struct LineMatches<T> {
    line: StyledText,
    /// Data that goes with the line, unchanged
    payload: T,
    /// Byte ranges of `line.text`
    matches: Vec<Range<usize>>,
    /// Log odds of nucleotides over amino acids for each match
    log_odds: Vec<f64>,
}

/// Paint lines in order with the styles of `config`.
///
/// With [`Styles::ByType`], a match gets the type of [`SeqType::from_log_odds`],
/// with the other matches in its column on the lines of its reader read so far.
///
/// # Arguments
/// * `lines` - Lines, grouped by reader
/// * `config` - Styles, and regexes whose common matches are colored in lines that are not excluded
/// * `streaming` - Paint the first lines of a reader when [`ColumnLogOdds::is_conclusive`],
///   or after `MAX_READ_AHEAD_LINES` lines, then paint each later line when it is read.
///   Else paint the lines of a reader after all of them are read.
/// * `emit` - Called with each painted line and its data. Its first error stops the painting.
///
/// # Errors
/// The first error of `emit`.
fn paint_lines<T>(
    lines: impl Iterator<Item = Line<T>>,
    config: &Config,
    streaming: bool,
    mut emit: impl FnMut(Vec<Char>, T) -> io::Result<()>,
) -> io::Result<()> {
    let line_matches = |line: &Line<T>| {
        if line.excluded { Vec::new() } else { intersect_matches(&line.text.text, &config.regexes) }
    };
    let (nucl, aa) = match &config.styles {
        Styles::One(styles) => {
            for line in lines {
                let matches: Vec<_> = line_matches(&line).into_iter().map(|r| (r, styles)).collect();
                emit(paint_line(&line.text, &matches), line.payload)?;
            }
            return Ok(());
        }
        Styles::ByType { nucl, aa } => (nucl, aa),
    };
    let paint = |line: &LineMatches<T>, column_log_odds: &ColumnLogOdds| {
        let seq_types = column_log_odds.seq_types(&line.log_odds);
        let matches: Vec<_> = line
            .matches
            .iter()
            .cloned()
            .zip(seq_types)
            .map(|(range, seq_type)| (range, match seq_type { SeqType::Nucl => nucl, SeqType::AA => aa }))
            .collect();
        paint_line(&line.line, &matches)
    };

    let mut lines = lines.peekable();
    while let Some(i_reader) = lines.peek().map(|line| line.i_reader) {
        let mut reader_lines = std::iter::from_fn(|| lines.next_if(|line| line.i_reader == i_reader)).map(|line| {
            let matches = line_matches(&line);
            let log_odds = matches.iter().map(|range| nucl_log_odds(&line.text.text[range.clone()])).collect();
            LineMatches { line: line.text, payload: line.payload, matches, log_odds }
        });

        let mut column_log_odds = ColumnLogOdds::default();
        let mut lines_ahead = Vec::new();
        for line in reader_lines.by_ref() {
            column_log_odds.add(&line.log_odds);
            lines_ahead.push(line);
            if streaming && (column_log_odds.is_conclusive() || lines_ahead.len() >= MAX_READ_AHEAD_LINES) {
                break;
            }
        }
        for line in lines_ahead {
            emit(paint(&line, &column_log_odds), line.payload)?;
        }
        for line in reader_lines {
            column_log_odds.add(&line.log_odds);
            emit(paint(&line, &column_log_odds), line.payload)?;
        }
    }
    Ok(())
}

/// Sum of the log odds of the matches in each column.
///
/// The k-th matches of the lines with n matches are column (n, k),
/// so the matches of a table column add up, and a row with a missing cell is not summed with other columns.
#[derive(Default)]
struct ColumnLogOdds(HashMap<(usize, usize), f64>);

impl ColumnLogOdds {
    /// Add the matches of a line. Add each line once.
    ///
    /// # Arguments
    /// * `log_odds` - Log odds of nucleotides over amino acids for each match of a line
    fn add(&mut self, log_odds: &[f64]) {
        for (k, &match_log_odds) in log_odds.iter().enumerate() {
            *self.0.entry((log_odds.len(), k)).or_default() += match_log_odds;
        }
    }

    /// Sequence type of each match of a line that was added. See [`SeqType::from_log_odds`].
    ///
    /// # Arguments
    /// * `log_odds` - Log odds of each match of the line
    ///
    /// # Returns
    /// The type of each match, from its log odds and the sum of the other matches in its column.
    fn seq_types(&self, log_odds: &[f64]) -> Vec<SeqType> {
        log_odds
            .iter()
            .enumerate()
            .map(|(k, &match_log_odds)| {
                SeqType::from_log_odds(match_log_odds, self.0[&(log_odds.len(), k)] - match_log_odds)
            })
            .collect()
    }

    /// Whether a column was added and the sum of each column has |sum| ≥ [`MIN_ABS_LOG_ODDS`].
    fn is_conclusive(&self) -> bool {
        !self.0.is_empty() && self.0.values().all(|log_odds| log_odds.abs() >= MIN_ABS_LOG_ODDS)
    }
}

/// Paint the chars of a line.
/// Chars inside `matches` get the styles of the matches, other chars keep their input styles.
///
/// # Arguments
/// * `line` - Text, decoded as UTF-8, with its input styles. Invalid sequences become [`Char::Invalid`].
/// * `matches` - Ascending, non-overlapping byte ranges of `line.text`, each with the style of each char
///
/// # Returns
/// The chars of `line.text`.
fn paint_line(line: &StyledText, matches: &[(Range<usize>, &HashMap<char, Style>)]) -> Vec<Char> {
    let mut line_painted = Vec::with_capacity(line.text.len());
    let mut i = 0;
    for (range, styles) in matches {
        line_painted.extend(line.chars(i..range.start));
        line_painted.extend(to_painted(styles, &line.text[range.clone()]));
        i = range.end;
    }
    line_painted.extend(line.chars(i..line.text.len()));
    line_painted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi_colors::ColorDepth;
    use crate::inout::tests::FailingReader;
    use yansi::Color::{Blue, Red};

    #[test]
    fn column_log_odds_give_an_inconclusive_match_the_type_of_its_column() {
        let mut column_log_odds = ColumnLogOdds::default();
        column_log_odds.add(&[-20.0]);
        column_log_odds.add(&[1.0]);
        assert_eq!(column_log_odds.seq_types(&[1.0]), [SeqType::AA]);
    }

    #[test]
    fn column_log_odds_do_not_count_a_match_twice() {
        let mut column_log_odds = ColumnLogOdds::default();
        column_log_odds.add(&[3.0]);
        column_log_odds.add(&[-2.0]);
        assert_eq!(column_log_odds.seq_types(&[-2.0]), [SeqType::Nucl]);
    }

    #[test]
    fn column_log_odds_separate_lines_with_different_numbers_of_matches() {
        let mut column_log_odds = ColumnLogOdds::default();
        column_log_odds.add(&[-20.0, -20.0]);
        column_log_odds.add(&[1.0]);
        assert_eq!(column_log_odds.seq_types(&[1.0]), [SeqType::Nucl]);
    }

    #[test]
    fn run_colors_each_match_by_the_type_of_its_reader() {
        let config = Config {
            styles: Styles::ByType {
                nucl: HashMap::from([('A', Red.background())]),
                aa: HashMap::from([('A', Blue.background())]),
            },
            regexes: vec![],
            formats: vec![],
            transpose: false,
            highlight: None,
            support: Support::Colors(ColorDepth::True),
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

    /// Config for FASTA and FASTQ input and a true color terminal. It colors A red in text that all `regexes` match.
    fn red_a(regexes: &[&str], transpose: bool) -> Config {
        Config {
            styles: Styles::One(HashMap::from([('A', Red.background())])),
            regexes: regexes.iter().map(|re| Regex::new(re).unwrap()).collect(),
            formats: vec![Format::Fasta, Format::Fastq],
            transpose,
            highlight: None,
            support: Support::Colors(ColorDepth::True),
        }
    }

    fn run_to_bytes(input: &'static [u8], config: &Config) -> Vec<u8> {
        let mut output = Vec::new();
        run(vec![("-".into(), Box::new(input))], &mut output, config).unwrap();
        output
    }

    #[test]
    fn run_streams_styled_lines() {
        assert_eq!(run_to_bytes(b">h\n\nAC\n", &red_a(&[], false)), b">h\n\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }

    #[test]
    fn run_streams_invalid_utf8_unchanged() {
        assert_eq!(
            run_to_bytes(b">caf\xe9\nA\xffA\n", &red_a(&[], false)),
            b">caf\xe9\n\x1b[0m\x1b[41mA\x1b[0m\xff\x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_with_unstyled_padding() {
        assert_eq!(
            run_to_bytes(b"AA\nA\n\xffA\n", &red_a(&[], true)),
            b"\x1b[0m\x1b[41mAA\x1b[0m\xff\n\x1b[0m\x1b[41mA\x1b[0m \x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_invalid_utf8_unchanged() {
        assert_eq!(run_to_bytes(b"\xe2\x82A\n", &red_a(&[], true)), b"\xe2\x82\n\x1b[0m\x1b[41mA\x1b[0m\n");
    }

    #[test]
    fn run_matches_text_without_codes() {
        let config = red_a(&["[ACGT]{4,}"], false);
        assert_eq!(run_to_bytes(b"\x1b[1mAC\x1b[0mGT\n", &config), run_to_bytes(b"ACGT\n", &config));
    }

    #[test]
    fn run_keeps_input_styles_outside_matches() {
        assert_eq!(run_to_bytes(b"\x1b[1mACGT\n", &red_a(&["GT"], false)), b"\x1b[0m\x1b[1mAC\x1b[0mGT\n");
    }

    #[test]
    fn run_carries_style_across_lines() {
        assert_eq!(run_to_bytes(b"\x1b[1mx\ny\n", &red_a(&["Z"], false)), b"\x1b[0m\x1b[1mx\x1b[0m\n\x1b[0m\x1b[1my\x1b[0m\n");
    }

    #[test]
    fn run_restricts_input_styles() {
        let config = Config { support: Support::Plain, ..red_a(&["Z"], false) };
        assert_eq!(run_to_bytes(b"\x1b[1;31mx\n", &config), b"x\n");
    }

    #[test]
    fn run_transposes_text_without_codes() {
        assert_eq!(run_to_bytes(b"\x1b[1mAC\x1b[0m\n", &red_a(&[], true)), b"\x1b[0m\x1b[41mA\x1b[0m\nC\n");
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
            let config = Config {
                styles: Styles::One(HashMap::new()),
                regexes: vec![],
                formats: vec![],
                transpose,
                highlight: None,
                support: Support::Colors(ColorDepth::True),
            };
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
            run_to_bytes(b">h\nAC\nCA\n", &red_a(&[], true)),
            b">h\n\x1b[0m\x1b[41mA\x1b[0mC\n\x1b[0mC\x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_fastq_reads_without_quality_lines() {
        assert_eq!(
            run_to_bytes(b"@r1\nAC\n+\nFF\n@r2\nAG\n+\nFF\n", &red_a(&[], true)),
            b"@r1\n+\nFF\n@r2\n+\nFF\n\x1b[0m\x1b[41mAA\x1b[0m\nCG\n",
        );
    }

    #[test]
    fn run_finds_fastq_quality_lines_in_each_reader() {
        let readers: Vec<(String, Box<dyn BufRead>)> =
            vec![("a".into(), Box::new(&b"@r\nAC\n+\n"[..])), ("b".into(), Box::new(&b"AC\n"[..]))];
        let mut output = Vec::new();
        run(readers, &mut output, &red_a(&[], false)).unwrap();
        assert_eq!(output, b"@r\n\x1b[0m\x1b[41mA\x1b[0mC\n+\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }

    #[test]
    fn run_keeps_line_endings() {
        for transpose in [false, true] {
            assert_eq!(run_to_bytes(b">h\r\nx\ny\r\nz", &red_a(&["Z"], transpose)), b">h\r\nx\ny\r\nz");
        }
    }

    #[test]
    fn run_transposes_with_line_ending_of_first_row() {
        assert_eq!(run_to_bytes(b">h\nCC\r\nCC\n", &red_a(&[], true)), b">h\nCC\r\nCC\r\n");
        assert_eq!(run_to_bytes(b"CC", &red_a(&[], true)), b"C\nC");
    }

    #[test]
    fn run_ends_a_line_without_line_ending_before_more_output() {
        assert_eq!(run_to_bytes(b"CC\n>h", &red_a(&[], true)), b">h\nC\nC\n");
        let readers: Vec<(String, Box<dyn BufRead>)> =
            vec![("a".into(), Box::new(&b"x"[..])), ("b".into(), Box::new(&b"y"[..]))];
        let mut output = Vec::new();
        run(readers, &mut output, &red_a(&["Z"], false)).unwrap();
        assert_eq!(output, b"x\ny");
    }
}
