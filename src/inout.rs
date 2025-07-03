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

// Read lines along with a number of maximum line length.
pub fn read_lines(filenames: Vec<String>) -> Result<(Vec<String>, usize)> {
    let mut lines = Vec::new();
    let mut max_line = 0;
    for filename in filenames {
        match open(&filename) {
            Err(e) => eprintln!("{filename}: {e}"),
            Ok(file) => {
                for line_result in file.lines() {
                    let line = line_result?;
                    max_line = max_line.max(line.len());
                    lines.push(line);
                }
            }
        }
    }
    Ok((lines, max_line))
}
