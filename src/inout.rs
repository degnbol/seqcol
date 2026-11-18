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

/// Lines of `readers`, like [`BufRead::lines`], but with invalid UTF-8 replaced by U+FFFD.
///
/// # Arguments
/// * `readers` - Input, read in order
///
/// # Returns
/// Each line without its trailing `\n`, `\r\n`, or `\r`, or the read error.
pub fn read_lines(readers: Vec<Box<dyn BufRead>>) -> impl Iterator<Item = io::Result<String>> {
    readers.into_iter().flat_map(|reader| {
        reader.split(b'\n').map(|line| {
            let mut line = line?;
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            Ok(String::from_utf8(line)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
        })
    })
}

/// Read all lines of `readers` into memory.
///
/// # Arguments
/// * `readers` - Input, read in order
///
/// # Returns
/// The lines (see [`read_lines`]) and the maximum line length in bytes, or the first read error.
pub fn read_lines_max(readers: Vec<Box<dyn BufRead>>) -> io::Result<(Vec<String>, usize)> {
    let mut lines = Vec::new();
    let mut max_line = 0;
    for line in read_lines(readers) {
        let line = line?;
        max_line = max_line.max(line.len());
        lines.push(line);
    }
    Ok((lines, max_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_lines_replaces_invalid_utf8() {
        let reader: Box<dyn BufRead> = Box::new(&b"A\xffC\r\nGT"[..]);
        let lines: Vec<String> = read_lines(vec![reader]).map(Result::unwrap).collect();
        assert_eq!(lines, ["A\u{FFFD}C", "GT"]);
    }
}
