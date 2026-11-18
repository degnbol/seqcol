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

// Iterator of lines from readers, in order.
pub fn read_lines(readers: Vec<Box<dyn BufRead>>) -> impl Iterator<Item=String> {
    readers.into_iter().flat_map(|reader| reader.lines().map(Result::unwrap))
}


// Read lines along with a number of maximum line length.
pub fn read_lines_max(readers: Vec<Box<dyn BufRead>>) -> (Vec<String>, usize) {
    let mut lines = Vec::new();
    let mut max_line = 0;
    for line in read_lines(readers) {
        max_line = max_line.max(line.len());
        lines.push(line);
    }
    (lines, max_line)
}
