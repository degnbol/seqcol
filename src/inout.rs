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
