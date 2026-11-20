/// Finds the quality lines of FASTQ records in a stream of lines.
#[derive(Default)]
pub struct QualityLineFinder {
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
    pub fn push(&mut self, line: &[u8]) -> bool {
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
}
