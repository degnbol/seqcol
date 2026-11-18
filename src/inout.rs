use anyhow::Result;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

// Understand "-" to mean stdin.
pub fn open(filename: &str) -> Result<Box<dyn BufRead>> {
    match filename {
        "-" => Ok(Box::new(BufReader::new(io::stdin()))),
        _ => Ok(Box::new(BufReader::new(File::open(filename)?))),
    }
}

/// Lines of `reader` as bytes, like [`BufRead::split`] on `\n`.
///
/// The lines end after the first read error.
///
/// # Arguments
/// * `reader` - Input
///
/// # Returns
/// Each line without its trailing `\n`, `\r\n`, or `\r`, or the read error.
pub fn read_lines(reader: impl BufRead) -> impl Iterator<Item = io::Result<Vec<u8>>> {
    let mut failed = false;
    reader.split(b'\n').map_while(move |line| {
        if failed {
            return None;
        }
        failed = line.is_err();
        Some(line.map(|mut line| {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            line
        }))
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn read_lines_keeps_invalid_utf8() {
        let reader: Box<dyn BufRead> = Box::new(&b"A\xffC\r\nGT"[..]);
        let lines: Vec<Vec<u8>> = read_lines(reader).map(Result::unwrap).collect();
        assert_eq!(lines, [&b"A\xffC"[..], b"GT"]);
    }

    pub(crate) struct FailingReader;

    impl io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failed"))
        }
    }

    #[test]
    fn read_lines_ends_after_error() {
        assert_eq!(read_lines(BufReader::new(FailingReader)).count(), 1);
    }
}
