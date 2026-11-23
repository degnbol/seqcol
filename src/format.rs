use std::ffi::OsStr;
use std::path::Path;

/// Text format, which defines the lines that are not sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// All lines can be sequence.
    Text,
    /// Lines that start with `>` (header lines) are not sequence.
    Fasta,
    /// Lines that start with `@` or `+` (header and separator lines), and quality lines of records, are not sequence.
    Fastq,
}

impl Format {
    /// Format of a file from its extension, after a `.gz` extension is removed. Case-insensitive.
    ///
    /// # Arguments
    /// * `path` - File path
    ///
    /// # Returns
    /// `Fasta` for .fa .fa1 .fasta .fas .fsa .fna .faa .ffn .frn .mfa .mpfa .afa, `Fastq` for .fq .fastq, else `Text`.
    pub fn from_path(path: &str) -> Format {
        let path = path.to_ascii_lowercase();
        let uncompressed = path.strip_suffix(".gz").unwrap_or(&path);
        match Path::new(uncompressed).extension().and_then(OsStr::to_str) {
            Some("fa" | "fa1" | "fasta" | "fas" | "fsa" | "fna" | "faa" | "ffn" | "frn" | "mfa" | "mpfa" | "afa") => {
                Format::Fasta
            }
            Some("fq" | "fastq") => Format::Fastq,
            _ => Format::Text,
        }
    }
}

/// Finds the lines of a stream that are not sequence in one of a set of formats.
pub struct NonSequenceLineFinder {
    fasta: bool,
    /// `None` without FASTQ
    fastq: Option<QualityLineFinder>,
}

impl NonSequenceLineFinder {
    /// Finder for a new stream.
    ///
    /// # Arguments
    /// * `formats` - A line is not sequence if it is not sequence in one of these formats
    pub fn new(formats: &[Format]) -> Self {
        Self {
            fasta: formats.contains(&Format::Fasta),
            fastq: formats.contains(&Format::Fastq).then(QualityLineFinder::default),
        }
    }

    /// Classify the next line of the stream.
    ///
    /// # Arguments
    /// * `line` - Text of the line, without line ending
    ///
    /// # Returns
    /// Whether `line` is not sequence in one of the formats.
    pub fn push(&mut self, line: &[u8]) -> bool {
        let is_quality = self.fastq.as_mut().is_some_and(|finder| finder.push(line));
        is_quality
            || match line.first() {
                Some(b'>') => self.fasta,
                Some(b'@' | b'+') => self.fastq.is_some(),
                _ => false,
            }
    }
}

/// Finds the quality lines of FASTQ records in a stream of lines.
#[derive(Default)]
struct QualityLineFinder {
    state: State,
    /// Title of the last header line, without `@`
    title: Vec<u8>,
}

/// Position in a FASTQ record after the last line.
#[derive(Default, Clone, Copy)]
enum State {
    /// Not in a record
    #[default]
    None,
    Header,
    /// Sequence of this length in bytes
    Sequence(usize),
    /// Separator after a sequence of this length in bytes
    Separator(usize),
}

impl QualityLineFinder {
    /// Classify the next line of the stream.
    ///
    /// # Arguments
    /// * `line` - Text of the line, without line ending
    ///
    /// # Returns
    /// Whether `line` is the quality line of a record: it follows a header line (`@title`, where `title` does not
    /// start with `@`, so the hunk header `@@ … @@` of a unified diff is not a header line), a sequence line,
    /// and a separator line (`+`, or `+` and the title of the header line), and has the length of the sequence line.
    fn push(&mut self, line: &[u8]) -> bool {
        let (state, is_quality) = match self.state {
            State::Separator(n) if line.len() == n => (State::None, true),
            State::Sequence(n) if line.strip_prefix(b"+").is_some_and(|t| t.is_empty() || t == self.title) => {
                (State::Separator(n), false)
            }
            State::Header => (State::Sequence(line.len()), false),
            _ => match line.strip_prefix(b"@") {
                Some(title) if !title.starts_with(b"@") => {
                    self.title.clear();
                    self.title.extend_from_slice(title);
                    (State::Header, false)
                }
                _ => (State::None, false),
            },
        };
        self.state = state;
        is_quality
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether each line is a quality line.
    fn quality_lines(lines: &[&str]) -> Vec<bool> {
        let mut finder = QualityLineFinder::default();
        lines.iter().map(|line| finder.push(line.as_bytes())).collect()
    }

    #[test]
    fn records_have_quality_on_line_4() {
        assert_eq!(
            quality_lines(&["@r1", "ACGT", "+", "FFFF", "@r2", "AC", "+", "FF"]),
            [false, false, false, true, false, false, false, true],
        );
    }

    #[test]
    fn quality_can_start_with_header_or_separator_char() {
        assert_eq!(
            quality_lines(&["@r1", "AC", "+", "@F", "@r2", "AC", "+", "+F"]),
            [false, false, false, true, false, false, false, true],
        );
    }

    #[test]
    fn separator_title_must_be_header_title() {
        assert_eq!(quality_lines(&["@r1 x", "AC", "+r1 x", "FF"]), [false, false, false, true]);
        assert_eq!(quality_lines(&["@r1", "AC", "+r2", "FF"]), [false; 4]);
    }

    #[test]
    fn quality_must_have_sequence_length() {
        assert_eq!(quality_lines(&["@r", "ACG", "+", "FF", "FFF"]), [false; 5]);
    }

    #[test]
    fn multi_line_record_has_no_quality() {
        assert_eq!(quality_lines(&["@r", "AC", "GT", "+", "FFFF"]), [false; 5]);
    }

    #[test]
    fn empty_record_has_quality() {
        assert_eq!(quality_lines(&["@r", "", "+", ""]), [false, false, false, true]);
    }

    #[test]
    fn unified_diff_has_no_quality() {
        assert_eq!(quality_lines(&["@@ -1,3 +1,4 @@", " a", "+b", " c"]), [false; 4]);
        assert_eq!(quality_lines(&["@@ -1,2 +1,3 @@", " a", "+", " c"]), [false; 4]);
    }

    /// Whether each line is not sequence in `formats`.
    fn non_sequence_lines(formats: &[Format], lines: &[&str]) -> Vec<bool> {
        let mut finder = NonSequenceLineFinder::new(formats);
        lines.iter().map(|line| finder.push(line.as_bytes())).collect()
    }

    #[test]
    fn text_is_all_sequence() {
        assert_eq!(non_sequence_lines(&[Format::Text], &[">a", "@r", "AC", "+", "FF"]), [false; 5]);
    }

    #[test]
    fn fasta_header_is_not_sequence() {
        assert_eq!(
            non_sequence_lines(&[Format::Fasta], &[">a", "AC", "@r", "AC", "+", "FF"]),
            [true, false, false, false, false, false],
        );
    }

    #[test]
    fn fastq_header_separator_and_quality_are_not_sequence() {
        assert_eq!(
            non_sequence_lines(&[Format::Fastq], &[">a", "AC", "@r", "AC", "+", "FF"]),
            [false, false, true, false, true, true],
        );
    }

    #[test]
    fn format_from_path_is_from_extension() {
        for (path, format) in [
            ("x.fa", Format::Fasta),
            ("x.fa1", Format::Fasta),
            ("dir/x.FASTA.gz", Format::Fasta),
            ("x.fq", Format::Fastq),
            ("x.fastq.gz", Format::Fastq),
            ("x.txt", Format::Text),
            ("x.gz", Format::Text),
            ("-", Format::Text),
            ("/dev/fd/63", Format::Text),
            ("x", Format::Text),
        ] {
            assert_eq!(Format::from_path(path), format, "{path}");
        }
    }

    #[test]
    fn formats_combine() {
        assert_eq!(
            non_sequence_lines(&[Format::Fasta, Format::Fastq], &[">a", "AC", "@r", "AC", "+", "FF"]),
            [true, false, true, false, true, true],
        );
    }
}
