pub mod ansi_colors;
pub mod bio;
pub mod case;
pub mod charmap;
pub mod colorschemes;
pub mod consensus;
pub mod format;
pub mod inout;
pub mod matches;
pub mod pager;

use regex::bytes::Regex;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::ops::Range;
use std::rc::Rc;
use yansi::{Painted, Style};

use crate::ansi_colors::{restrict, to_painted, write_line, Char, CharStyles, StyledText, Support};
use crate::bio::SeqType;
use crate::charmap::CharMap;
use crate::format::{Format, NonSequenceLineFinder};
use crate::matches::intersect_matches;

/// What to color, and how.
pub struct Config {
    /// Swap columns and rows of the colored lines.
    pub transpose: bool,
    /// `None` disables highlighting.
    pub highlight: Option<Highlight>,
    /// Styles the terminal can show. Input styles are restricted to it.
    pub support: Support,
}

/// A stream of input lines.
pub struct Input {
    /// Prefix of its read errors
    pub name: String,
    pub reader: Box<dyn BufRead>,
    /// A line is not sequence if it is not sequence in one of these formats
    pub formats: Vec<Format>,
    /// Styles of the characters in the matches of this input
    pub styles: Rc<Styles>,
    /// Color only text matched by every regex. Empty colors all text.
    pub regexes: Rc<Vec<Regex>>,
}

/// Style of each character.
pub enum Styles {
    /// The same styles for all text.
    One(CharStyles),
    /// Entries of sequence types and their styles. Each match of [`Input::regexes`] gets the styles
    /// of the entry with the largest log-likelihood, where the log-likelihood of an entry is the largest of its types.
    /// Ties give the first entry.
    ByType(Vec<(Vec<SeqType>, CharStyles)>),
}

/// Consensus or mismatch highlighting.
pub struct Highlight {
    /// Highlight the consensus if true, otherwise the mismatches.
    pub consensus: bool,
    /// Style to lay over highlighted chars.
    pub style: Style,
    /// Characters to count for the consensus. `None` counts all.
    pub alphabet: Option<CharMap<bool>>,
    /// Count lowercase and uppercase of a letter as the same character.
    pub ignore_case: bool,
}

/// Write the lines of `inputs` with ANSI styles.
///
/// Streams the input, unless `config` asks to transpose or highlight.
/// Each input is matched by its own regexes, and painted with its own styles.
/// In an input with [`Styles::ByType`], each match is classified by its letters,
/// and by the matches in the same column of the input (see [`ColumnLogLikelihoods`]).
/// So output can wait for the first lines of that input.
/// Lines that are not sequence in one of the formats of their input get no matches.
/// CSI escape sequences are removed before matching. Text keeps the styles of the SGR codes,
/// with the styles of the matches laid over them.
/// An SGR style continues on the next lines of its input.
/// Invalid UTF-8 and line endings are written unchanged,
/// but a line without line ending gets `\n` if more output follows it.
/// Transposed rows get the line ending of the first transposed line.
/// A read error skips the rest of its input.
/// A closed `output` ends the reading without error.
///
/// # Arguments
/// * `inputs` - Read in order
/// * `output` - Destination of the styled lines
/// * `config` - What to color, and how
///
/// # Returns
/// The read errors, each message prefixed by `"{name}: "`.
///
/// # Errors
/// The first write error other than a closed `output`.
pub fn run(inputs: Vec<Input>, output: &mut dyn Write, config: &Config) -> io::Result<Vec<io::Error>> {
    let mut read_errors = Vec::new();
    let support = config.support;
    let colorings: Vec<Coloring> = inputs.iter().map(|input| (input.styles.clone(), input.regexes.clone())).collect();
    let lines = inputs
        .into_iter()
        .enumerate()
        .flat_map(|(i_input, Input { name, reader, formats, .. })| {
            let non_sequence_line_finder = NonSequenceLineFinder::new(&formats);
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
                        Line { i_input, text, excluded, payload: ending }
                    })
                    .map_err(|e| io::Error::new(e.kind(), format!("{name}: {e}"))),
                )
            })
        })
        .filter_map(|line| line.map_err(|e| read_errors.push(e)).ok());
    match write_styled(lines, &mut io::BufWriter::with_capacity(1 << 16, output), &colorings, config) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e),
        _ => Ok(read_errors),
    }
}

/// [`Input::styles`] and [`Input::regexes`] of an input.
pub type Coloring = (Rc<Styles>, Rc<Vec<Regex>>);

/// A line of input, and data that goes with it.
struct Line<T> {
    /// Index of the input of the line
    i_input: usize,
    text: StyledText,
    /// The line gets no matches
    excluded: bool,
    /// Data that goes with the line, unchanged
    payload: T,
}

/// Write `lines`, each with its line ending as payload, with ANSI styles. See [`run`].
/// Only the last line written can be without line ending.
/// `colorings` are the styles and regexes of each input, by [`Line::i_input`].
fn write_styled(
    lines: impl Iterator<Item = Line<&'static [u8]>>,
    output: &mut dyn Write,
    colorings: &[Coloring],
    config: &Config,
) -> io::Result<()> {
    let mut output = LineWriter { output, is_open: false };
    if !config.transpose && config.highlight.is_none() {
        // Streaming.
        paint_lines(lines, colorings, config.support, true, |line_painted, ending| output.write(&line_painted, ending))?;
    } else {
        // Not streaming.
        let mut lines_painted: Vec<Vec<Char>> = Vec::new();
        let mut endings: Vec<&[u8]> = Vec::new();
        paint_lines(lines, colorings, config.support, false, |line_painted, ending| {
            lines_painted.push(line_painted);
            endings.push(ending);
            Ok(())
        })?;
        if let Some(highlight) = &config.highlight {
            consensus::highlight_consensus_or_mismatches(
                &mut lines_painted,
                &highlight.alphabet,
                highlight.consensus,
                highlight.ignore_case,
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

/// Maximum number of lines to read before classifying the matches of an input while streaming.
const MAX_READ_AHEAD_LINES: usize = 100;

/// A line, the byte ranges of it that all regexes match, and data that goes with the line.
struct LineMatches<T> {
    line: StyledText,
    /// Data that goes with the line, unchanged
    payload: T,
    /// Byte ranges of `line.text`
    matches: Vec<Range<usize>>,
    /// Log-likelihood of each entry of [`Styles::ByType`] for each match
    log_likelihoods: Vec<Vec<f64>>,
}

/// Paint lines in order, each input with its own styles and regexes.
///
/// In an input with [`Styles::ByType`], a match gets the styles of [`ColumnLogLikelihoods::most_likely`],
/// with the other matches in its column on the lines of the input read so far.
///
/// # Arguments
/// * `lines` - Lines, grouped by input
/// * `colorings` - Styles and regexes of each input, by [`Line::i_input`].
///   The common matches of the regexes are colored in lines that are not excluded.
/// * `support` - Styles the terminal can show
/// * `streaming` - Paint the first lines of an input with [`Styles::ByType`] when [`ColumnLogLikelihoods::is_conclusive`],
///   or after `MAX_READ_AHEAD_LINES` lines, then paint each later line when it is read.
///   Else paint the lines of such an input after all of them are read.
/// * `emit` - Called with each painted line and its data. Its first error stops the painting.
///
/// # Errors
/// The first error of `emit`.
fn paint_lines<T>(
    lines: impl Iterator<Item = Line<T>>,
    colorings: &[Coloring],
    support: Support,
    streaming: bool,
    mut emit: impl FnMut(Vec<Char>, T) -> io::Result<()>,
) -> io::Result<()> {
    let mut lines = lines.peekable();
    while let Some(i_input) = lines.peek().map(|line| line.i_input) {
        let input_lines = std::iter::from_fn(|| lines.next_if(|line| line.i_input == i_input));
        let (styles, regexes) = &colorings[i_input];
        let line_matches = |line: &Line<T>| {
            if line.excluded { Vec::new() } else { intersect_matches(&line.text.text, regexes) }
        };
        let entries = match &**styles {
            Styles::One(styles) => {
                for line in input_lines {
                    let matches: Vec<_> = line_matches(&line).into_iter().map(|r| (r, styles)).collect();
                    emit(paint_line(&line.text, &matches, support), line.payload)?;
                }
                continue;
            }
            Styles::ByType(entries) => entries,
        };
        let paint = |line: &LineMatches<T>, column_log_likelihoods: &ColumnLogLikelihoods| {
            let i_entries = column_log_likelihoods.most_likely(&line.log_likelihoods);
            let matches: Vec<_> =
                line.matches.iter().cloned().zip(i_entries).map(|(range, i_entry)| (range, &entries[i_entry].1)).collect();
            paint_line(&line.line, &matches, support)
        };
        let mut input_lines = input_lines.map(|line| {
            let matches = line_matches(&line);
            let log_likelihoods = matches
                .iter()
                .map(|range| {
                    let by_type = bio::log_likelihoods(&line.text.text[range.clone()]);
                    let of_types = |seq_types: &[SeqType]| {
                        seq_types.iter().map(|&seq_type| by_type[seq_type as usize]).fold(f64::NEG_INFINITY, f64::max)
                    };
                    entries.iter().map(|(seq_types, _)| of_types(seq_types)).collect()
                })
                .collect();
            LineMatches { line: line.text, payload: line.payload, matches, log_likelihoods }
        });

        let mut column_log_likelihoods = ColumnLogLikelihoods::default();
        let mut lines_ahead = Vec::new();
        for line in input_lines.by_ref() {
            column_log_likelihoods.add(&line.log_likelihoods);
            lines_ahead.push(line);
            if streaming && (column_log_likelihoods.is_conclusive() || lines_ahead.len() >= MAX_READ_AHEAD_LINES) {
                break;
            }
        }
        for line in lines_ahead {
            emit(paint(&line, &column_log_likelihoods), line.payload)?;
        }
        for line in input_lines {
            column_log_likelihoods.add(&line.log_likelihoods);
            emit(paint(&line, &column_log_likelihoods), line.payload)?;
        }
    }
    Ok(())
}

/// Sum of the log-likelihoods of the matches in each column, for each class.
///
/// The k-th matches of the lines with n matches are column (n, k),
/// so the matches of a table column add up, and a row with a missing cell is not summed with other columns.
#[derive(Default)]
struct ColumnLogLikelihoods(HashMap<(usize, usize), Vec<f64>>);

impl ColumnLogLikelihoods {
    /// Add the matches of a line. Add each line once.
    ///
    /// # Arguments
    /// * `log_likelihoods` - Log-likelihood of each class, for each match of a line. Each match has the same classes.
    fn add(&mut self, log_likelihoods: &[Vec<f64>]) {
        for (k, match_log_likelihoods) in log_likelihoods.iter().enumerate() {
            let sums = self.0.entry((log_likelihoods.len(), k)).or_insert_with(|| vec![0.0; match_log_likelihoods.len()]);
            for (sum, log_likelihood) in sums.iter_mut().zip(match_log_likelihoods) {
                *sum += log_likelihood;
            }
        }
    }

    /// Most likely class of each match of a line that was added. See [`bio::most_likely`].
    ///
    /// # Arguments
    /// * `log_likelihoods` - Log-likelihood of each class, for each match of the line
    ///
    /// # Returns
    /// The index of the class of each match, from its log-likelihoods and the sums of the other matches in its column.
    fn most_likely(&self, log_likelihoods: &[Vec<f64>]) -> Vec<usize> {
        log_likelihoods
            .iter()
            .enumerate()
            .map(|(k, match_log_likelihoods)| {
                let column = &self.0[&(log_likelihoods.len(), k)];
                let others: Vec<f64> = column.iter().zip(match_log_likelihoods).map(|(sum, own)| sum - own).collect();
                bio::most_likely(match_log_likelihoods, &others)
            })
            .collect()
    }

    /// Whether a column was added and the sums of each column are conclusive (see [`bio::is_conclusive`]).
    fn is_conclusive(&self) -> bool {
        !self.0.is_empty() && self.0.values().all(|sums| bio::is_conclusive(sums))
    }
}

/// Paint the chars of a line.
/// Chars inside `matches` get the styles of the matches laid over their input styles.
/// Other chars keep their input styles.
///
/// # Arguments
/// * `line` - Text, decoded as UTF-8, with its input styles. Invalid sequences become [`Char::Invalid`].
/// * `matches` - Ascending, non-overlapping byte ranges of `line.text`, each with the style of each char
/// * `support` - Styles the terminal can show
///
/// # Returns
/// The chars of `line.text`.
fn paint_line(line: &StyledText, matches: &[(Range<usize>, &CharStyles)], support: Support) -> Vec<Char> {
    let mut line_painted = Vec::with_capacity(line.text.len());
    let mut i = 0;
    for (range, styles) in matches {
        line_painted.extend(line.chars(i..range.start));
        for (text, style) in line.runs(range.clone()) {
            line_painted.extend(to_painted(styles, text, style, support));
        }
        i = range.end;
    }
    line_painted.extend(line.chars(i..line.text.len()));
    line_painted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi_colors::ColorDepth;
    use crate::charmap::CharMap;
    use crate::inout::tests::FailingReader;
    use yansi::Color::{self, Blue, Red};

    /// Log-likelihoods of two classes whose difference (first minus second) is `log_odds`.
    fn two(log_odds: f64) -> Vec<f64> {
        vec![log_odds, 0.0]
    }

    #[test]
    fn column_log_likelihoods_give_an_inconclusive_match_the_class_of_its_column() {
        let mut column_log_likelihoods = ColumnLogLikelihoods::default();
        column_log_likelihoods.add(&[two(-20.0)]);
        column_log_likelihoods.add(&[two(1.0)]);
        assert_eq!(column_log_likelihoods.most_likely(&[two(1.0)]), [1]);
    }

    #[test]
    fn column_log_likelihoods_do_not_count_a_match_twice() {
        let mut column_log_likelihoods = ColumnLogLikelihoods::default();
        column_log_likelihoods.add(&[two(3.0)]);
        column_log_likelihoods.add(&[two(-2.0)]);
        assert_eq!(column_log_likelihoods.most_likely(&[two(-2.0)]), [0]);
    }

    #[test]
    fn column_log_likelihoods_separate_lines_with_different_numbers_of_matches() {
        let mut column_log_likelihoods = ColumnLogLikelihoods::default();
        column_log_likelihoods.add(&[two(-20.0), two(-20.0)]);
        column_log_likelihoods.add(&[two(1.0)]);
        assert_eq!(column_log_likelihoods.most_likely(&[two(1.0)]), [0]);
    }

    #[test]
    fn run_colors_each_match_by_the_type_of_its_input() {
        let red_or_blue_a = red_or_blue_a();
        let inputs = vec![
            input("dna", b"ACGTACGT\nA\n", &[], &red_or_blue_a),
            input("prot", b"A\nLLLLLLLL\n", &[], &red_or_blue_a),
            input("short", b"A\n", &[], &red_or_blue_a),
        ];
        let mut output = Vec::new();
        run(inputs, &mut output, &config(false)).unwrap();
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!([lines[1], lines[2], lines[4]], [RED_A, BLUE_A, RED_A], "{output:?}");
    }

    #[test]
    fn run_colors_each_input_with_its_styles() {
        let blue_a = Rc::new(Styles::One(bg_a(Blue)));
        let inputs = vec![
            input("one", b"ACGTACGT\nA\n", &[], &blue_a),
            input("by_type", b"ACGTACGT\nA\n", &[], &red_or_blue_a()),
        ];
        let mut output = Vec::new();
        run(inputs, &mut output, &config(false)).unwrap();
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!([lines[1], lines[3]], [BLUE_A, RED_A], "{output:?}");
    }

    /// A line of only A, with a red background.
    const RED_A: &str = "\x1b[0m\x1b[41mA\x1b[0m";
    /// A line of only A, with a blue background.
    const BLUE_A: &str = "\x1b[0m\x1b[44mA\x1b[0m";

    /// Input of `text` in `formats`, painted with `styles` in all text.
    fn input(name: &str, text: &'static [u8], formats: &[Format], styles: &Rc<Styles>) -> Input {
        Input { name: name.into(), reader: Box::new(text), formats: formats.to_vec(), styles: styles.clone(), regexes: Rc::default() }
    }

    fn regexes(patterns: &[&str]) -> Rc<Vec<Regex>> {
        Rc::new(patterns.iter().map(|re| Regex::new(re).unwrap()).collect())
    }

    const FASTA_FASTQ: &[Format] = &[Format::Fasta, Format::Fastq];

    /// Styles that give A the background `color`.
    fn bg_a(color: Color) -> CharStyles {
        let by_char = CharMap { by_char: HashMap::from([('A', color.background())]), other: None };
        CharStyles { by_char, contrast_except: None }
    }

    /// Styles that color A blue in amino acids and red in nucleotides.
    fn red_or_blue_a() -> Rc<Styles> {
        let nucleotides = vec![SeqType::Dna, SeqType::Rna, SeqType::Dnax, SeqType::Rnax];
        Rc::new(Styles::ByType(vec![(vec![SeqType::AA], bg_a(Blue)), (nucleotides, bg_a(Red))]))
    }

    /// Styles that color A red.
    fn red_a() -> Rc<Styles> {
        Rc::new(Styles::One(bg_a(Red)))
    }

    /// Config for a true color terminal.
    fn config(transpose: bool) -> Config {
        Config { transpose, highlight: None, support: Support::Colors(ColorDepth::True) }
    }

    /// Output of [`run`] for `text` in FASTA and FASTQ formats, with A red in the text that all `regexes` match.
    fn run_to_bytes(text: &'static [u8], regexes: &[&str], config: &Config) -> Vec<u8> {
        let mut output = Vec::new();
        let input = Input { regexes: self::regexes(regexes), ..input("-", text, FASTA_FASTQ, &red_a()) };
        run(vec![input], &mut output, config).unwrap();
        output
    }

    #[test]
    fn run_streams_styled_lines() {
        assert_eq!(run_to_bytes(b">h\n\nAC\n", &[], &config(false)), b">h\n\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }

    #[test]
    fn run_streams_invalid_utf8_unchanged() {
        assert_eq!(
            run_to_bytes(b">caf\xe9\nA\xffA\n", &[], &config(false)),
            b">caf\xe9\n\x1b[0m\x1b[41mA\x1b[0m\xff\x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_with_unstyled_padding() {
        assert_eq!(
            run_to_bytes(b"AA\nA\n\xffA\n", &[], &config(true)),
            b"\x1b[0m\x1b[41mAA\x1b[0m\xff\n\x1b[0m\x1b[41mA\x1b[0m \x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_invalid_utf8_unchanged() {
        assert_eq!(run_to_bytes(b"\xe2\x82A\n", &[], &config(true)), b"\xe2\x82\n\x1b[0m\x1b[41mA\x1b[0m\n");
    }

    #[test]
    fn run_matches_text_without_codes() {
        let run_to_bytes = |text| run_to_bytes(text, &["[ACGT]{4,}"], &config(false));
        assert_eq!(run_to_bytes(b"AC\x1b[KGT\n"), run_to_bytes(b"ACGT\n"));
    }

    #[test]
    fn run_keeps_input_styles_outside_matches() {
        assert_eq!(run_to_bytes(b"\x1b[1mACGT\x1b[0m\n", &["GT"], &config(false)), b"\x1b[0m\x1b[1mACGT\x1b[0m\n");
    }

    #[test]
    fn run_lays_match_styles_over_input_styles() {
        assert_eq!(run_to_bytes(b"\x1b[1mAC\x1b[0m\n", &[], &config(false)), b"\x1b[0m\x1b[1;41mA\x1b[0m\x1b[1mC\x1b[0m\n");
    }

    #[test]
    fn run_carries_style_across_lines() {
        assert_eq!(run_to_bytes(b"\x1b[1mx\ny\n", &["Z"], &config(false)), b"\x1b[0m\x1b[1mx\x1b[0m\n\x1b[0m\x1b[1my\x1b[0m\n");
    }

    #[test]
    fn run_restricts_input_styles() {
        let config = Config { support: Support::Plain, ..config(false) };
        assert_eq!(run_to_bytes(b"\x1b[1;31mx\n", &["Z"], &config), b"x\n");
    }

    #[test]
    fn run_transposes_text_without_codes() {
        assert_eq!(run_to_bytes(b"A\x1b[KC\n", &[], &config(true)), b"\x1b[0m\x1b[41mA\x1b[0m\nC\n");
    }

    fn bad_and_good() -> Vec<Input> {
        let bad = Input { reader: Box::new(io::BufReader::new(FailingReader)), ..input("bad", b"", &[], &red_a()) };
        vec![bad, input("good", b"GT\n", &[], &red_a())]
    }

    fn messages(errors: Vec<io::Error>) -> Vec<String> {
        errors.iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn run_returns_read_errors_and_continues() {
        for transpose in [false, true] {
            let config = config(transpose);
            let mut output = Vec::new();
            let read_errors = run(bad_and_good(), &mut output, &config).unwrap();
            let mut output_valid = Vec::new();
            run(vec![input("good", b"GT\n", &[], &red_a())], &mut output_valid, &config).unwrap();
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
            run_to_bytes(b">h\nAC\nCA\n", &[], &config(true)),
            b">h\n\x1b[0m\x1b[41mA\x1b[0mC\n\x1b[0mC\x1b[41mA\x1b[0m\n",
        );
    }

    #[test]
    fn run_transposes_fastq_reads_without_quality_lines() {
        assert_eq!(
            run_to_bytes(b"@r1\nAC\n+\nFF\n@r2\nAG\n+\nFF\n", &[], &config(true)),
            b"@r1\n+\nFF\n@r2\n+\nFF\n\x1b[0m\x1b[41mAA\x1b[0m\nCG\n",
        );
    }

    #[test]
    fn run_finds_fastq_quality_lines_in_each_input() {
        let inputs = vec![input("a", b"@r\nAC\n+\n", FASTA_FASTQ, &red_a()), input("b", b"AC\n", FASTA_FASTQ, &red_a())];
        let mut output = Vec::new();
        run(inputs, &mut output, &config(false)).unwrap();
        assert_eq!(output, b"@r\n\x1b[0m\x1b[41mA\x1b[0mC\n+\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }

    #[test]
    fn run_keeps_line_endings() {
        for transpose in [false, true] {
            assert_eq!(run_to_bytes(b">h\r\nx\ny\r\nz", &["Z"], &config(transpose)), b">h\r\nx\ny\r\nz");
        }
    }

    #[test]
    fn run_transposes_with_line_ending_of_first_row() {
        assert_eq!(run_to_bytes(b">h\nCC\r\nCC\n", &[], &config(true)), b">h\nCC\r\nCC\r\n");
        assert_eq!(run_to_bytes(b"CC", &[], &config(true)), b"C\nC");
    }

    #[test]
    fn run_ends_a_line_without_line_ending_before_more_output() {
        assert_eq!(run_to_bytes(b"CC\n>h", &[], &config(true)), b">h\nC\nC\n");
        let inputs = ["a", "b"].map(|name| Input { regexes: regexes(&["Z"]), ..input(name, b"x", &[], &red_a()) });
        let mut output = Vec::new();
        run(inputs.into(), &mut output, &config(false)).unwrap();
        assert_eq!(output, b"x\nx");
    }

    #[test]
    fn run_matches_each_input_with_its_regexes() {
        let inputs = vec![
            Input { regexes: regexes(&["C"]), ..input("c", b"AC\n", &[], &red_a()) },
            Input { regexes: regexes(&["A"]), ..input("a", b"AC\n", &[], &red_a()) },
        ];
        let mut output = Vec::new();
        run(inputs, &mut output, &config(false)).unwrap();
        assert_eq!(output, b"AC\n\x1b[0m\x1b[41mA\x1b[0mC\n");
    }
}
