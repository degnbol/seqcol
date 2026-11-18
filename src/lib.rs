pub mod ansi_colors;
pub mod bio;
pub mod colorschemes;
pub mod consensus;
pub mod inout;
pub mod matches;
pub mod pager;

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use yansi::Style;

use crate::ansi_colors::{to_painted, write_ansi, write_line, Char};
use crate::matches::intersect_matches;

/// What to color, and how.
pub struct Config {
    /// Style of each character.
    pub styles: HashMap<char, Style>,
    /// Color only text matched by every regex. Empty colors all text.
    pub regexes: Vec<Regex>,
    /// Swap columns and rows of the colored lines.
    pub transpose: bool,
    /// `None` disables highlighting.
    pub highlight: Option<Highlight>,
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
/// A read error skips the rest of its reader.
/// A closed `output` ends the reading without error.
///
/// # Arguments
/// * `readers` - Input, read in order, each with a name for its errors
/// * `output` - Destination of the styled lines
/// * `config` - What to color, and how
///
/// # Returns
/// The read errors, each message prefixed by `"{name}: "`, or the first write error other than a closed `output`.
pub fn run(
    readers: Vec<(String, Box<dyn BufRead>)>,
    output: &mut dyn Write,
    config: &Config,
) -> io::Result<Vec<io::Error>> {
    let mut read_errors = Vec::new();
    let lines = readers
        .into_iter()
        .flat_map(|(name, reader)| {
            inout::read_lines(reader).map(move |line| line.map_err(|e| io::Error::new(e.kind(), format!("{name}: {e}"))))
        })
        .filter_map(|line| line.map_err(|e| read_errors.push(e)).ok());
    match write_styled(lines, output, config) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e),
        _ => Ok(read_errors),
    }
}

/// Write `lines` with ANSI styles. See [`run`].
fn write_styled(lines: impl Iterator<Item = String>, output: &mut dyn Write, config: &Config) -> io::Result<()> {
    let newline = b"\n";
    let space = b" ";

    // Reset code to ensure clean state at line start (helps pagers like less
    // correctly render lines during scrolling/re-rendering).
    let reset = b"\x1B[0m";

    if !config.transpose && config.highlight.is_none() {
        // Streaming.
        for line in lines {
            output.write_all(reset)?;
            let mut i = 0;
            for range in intersect_matches(&line, &config.regexes) {
                output.write_all(&line.as_bytes()[i..range.start])?;
                write_ansi(output, &config.styles, &line[range.clone()])?;
                i = range.end;
            }
            output.write_all(&line.as_bytes()[i..])?;
            output.write_all(newline)?;
        }
    } else {
        // Not streaming.
        // First read input into memory.
        let lines: Vec<String> = lines.collect();
        let max_line = lines.iter().map(String::len).max().unwrap_or(0);

        // Gather styles according to each char in each line.
        let mut lines_painted: Vec<Vec<Char>> = Vec::with_capacity(lines.len());
        for line in lines {
            let mut line_painted: Vec<Char> = Vec::with_capacity(line.len());
            let mut i = 0;
            for range in intersect_matches(&line, &config.regexes) {
                line_painted.extend(line[i..range.start].chars().map(Char::Unstyled));
                line_painted.extend(to_painted(&config.styles, &line[range.clone()]));
                i = range.end;
            }
            line_painted.extend(line[i..].chars().map(Char::Unstyled));
            lines_painted.push(line_painted);
        }

        if let Some(highlight) = &config.highlight {
            consensus::highlight_consensus_or_mutations(
                &mut lines_painted,
                max_line,
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
            // Transpose: only transpose lines containing styled (sequence) characters.
            // Non-sequence lines are output normally before the transposed block.
            let mut seq_lines: Vec<&Vec<Char>> = Vec::new();
            let mut seq_max_len = 0;

            for painted_line in &lines_painted {
                let has_styled = painted_line.iter().any(|c| matches!(c, Char::Styled(_)));
                if has_styled {
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
                output.write_all(reset)?;
                for seq_line in &seq_lines {
                    match seq_line.get(j) {
                        None => output.write_all(space)?,
                        Some(ch) => ch.write(output)?,
                    };
                }
                output.write_all(newline)?;
            }
        }
    }

    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inout::tests::FailingReader;
    use yansi::Color::Red;

    fn run_to_string(input: &'static str, transpose: bool) -> String {
        let config = Config {
            styles: HashMap::from([('A', Red.background())]),
            regexes: vec![Regex::new("^[^>@+].*").unwrap()],
            transpose,
            highlight: None,
        };
        let mut output = Vec::new();
        run(vec![("-".into(), Box::new(input.as_bytes()))], &mut output, &config).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn run_streams_styled_lines() {
        assert_eq!(
            run_to_string(">h\nAC\n", false),
            "\x1b[0m>h\n\x1b[0m\x1b[41mA\x1b[0mC\x1b[0m\n",
        );
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
            let config = Config { styles: HashMap::new(), regexes: vec![], transpose, highlight: None };
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
            run_to_string(">h\nAC\nCA\n", true),
            "\x1b[0m>h\n\x1b[0m\x1b[41mA\x1b[0mC\n\x1b[0mC\x1b[41mA\x1b[0m\n",
        );
    }
}
