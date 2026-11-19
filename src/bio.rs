use phf::phf_map;
use regex::bytes::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};

use crate::inout;
use crate::matches::intersect_matches;

/// Sequence type detected from content.
/// Uses all-caps for biological acronyms (DNA, RNA, AA) for readability.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeqType {
    DNA,
    RNA,
    /// DNA or RNA: the text has neither T nor U, or both.
    Nucl,
    AA,
}

impl SeqType {
    /// Get the colorscheme suffix for this sequence type.
    pub fn suffix(&self) -> &'static str {
        match self {
            SeqType::DNA | SeqType::RNA | SeqType::Nucl => "_nucl",
            SeqType::AA => "_aa",
        }
    }

    /// Derive SeqType from an alphabet keyword (dna, rna, nucl, aa, aax, all).
    /// Returns None for "all" or unknown keywords (requires auto-detection).
    pub fn from_alphabet(alphabet: &str) -> Option<SeqType> {
        match alphabet.to_lowercase().replace(" ", "").as_str() {
            "dna" | "dnanogap" => Some(SeqType::DNA),
            "rna" | "rnanogap" => Some(SeqType::RNA),
            "nucl" | "nuclnogap" => Some(SeqType::Nucl),
            "aa" | "aanogap" | "aax" | "aaxnogap" => Some(SeqType::AA),
            _ => None, // "all" or unknown - requires detection
        }
    }
}

/// Fraction of letters that must be ACGTUN for text to be nucleotides.
/// The rest allows IUPAC ambiguity codes such as R and Y.
const MIN_NUCLEOTIDE_FRACTION: f64 = 0.9;

/// Detect sequence type from the letters in `text`, ignoring case.
///
/// # Arguments
/// * `text` - Sequence text. Characters that are not ASCII letters are ignored.
///
/// # Returns
/// Nucleotides if at least `MIN_NUCLEOTIDE_FRACTION` of the letters are ACGTUN, else AA.
/// Nucleotides are DNA with T and no U, RNA with U and no T, else Nucl.
/// `None` if `text` has no letter.
pub fn detect_seq_type(text: &[u8]) -> Option<SeqType> {
    let letters = text.iter().filter(|c| c.is_ascii_alphabetic()).map(u8::to_ascii_uppercase);
    let (mut n_letters, mut n_nucleotides) = (0, 0);
    let (mut has_t, mut has_u) = (false, false);
    for c in letters {
        n_letters += 1;
        if b"ACGTUN".contains(&c) {
            n_nucleotides += 1;
        }
        has_t |= c == b'T';
        has_u |= c == b'U';
    }

    if n_letters == 0 {
        return None;
    }
    if (n_nucleotides as f64) < MIN_NUCLEOTIDE_FRACTION * n_letters as f64 {
        return Some(SeqType::AA);
    }
    match (has_t, has_u) {
        (true, false) => Some(SeqType::DNA),
        (false, true) => Some(SeqType::RNA),
        _ => Some(SeqType::Nucl),
    }
}

/// Minimum number of letters to read for sequence type detection.
const N_DETECTION_LETTERS: usize = 1000;

/// Detect sequence type by peeking at input files.
///
/// # Arguments
/// * `files` - Paths to read. Stdin ("-") and files that do not open are skipped.
/// * `regexes` - Only text that all regexes match is used. If empty, all text is used.
///
/// # Returns
/// Type of the text in the lines that hold the first `N_DETECTION_LETTERS` matched letters,
/// or of all matched text if it has fewer letters. `None` if the matched text has no letter.
pub fn detect_seq_type_from_files(files: &[String], regexes: &[Regex]) -> Option<SeqType> {
    let mut sample = Vec::new();
    let mut n_letters = 0;
    for filename in files {
        if filename == "-" {
            // For stdin, we can't peek without consuming
            // Skip auto-detection for stdin-only input
            continue;
        }
        let reader = match inout::open(filename) {
            Ok(r) => r,
            Err(_) => continue,
        };

        for line in inout::read_lines(reader).map_while(Result::ok) {
            for range in intersect_matches(&line, regexes) {
                let matched = &line[range];
                n_letters += matched.iter().filter(|c| c.is_ascii_alphabetic()).count();
                sample.extend_from_slice(matched);
            }
            if n_letters >= N_DETECTION_LETTERS {
                return detect_seq_type(&sample);
            }
        }
    }
    detect_seq_type(&sample)
}

/// Read an alphabet from a file, or look it up by keyword.
///
/// # Arguments
/// * `arg` - Path to a file with the alphabet characters, or a key of `ALPHABETS`.
///   Keywords ignore case and spaces.
///
/// # Returns
/// Characters of the alphabet, or the error from reading a file that opens.
///
/// # Panics
/// If `arg` is neither a file that opens nor a keyword.
pub fn read_alphabet(arg: &str) -> io::Result<HashSet<char>> {
    match File::open(arg) {
        Ok(mut file) => {
            let mut alphabet = String::new();
            file.read_to_string(&mut alphabet)?;
            Ok(alphabet.chars().collect())
        }
        Err(_) => match ALPHABETS.get(&arg.to_lowercase().replace(' ', "")) {
            Some(alphabet) => Ok(alphabet.chars().collect()),
            None => panic!("Alphabet keyword not understood."),
        },
    }
}

/// Characters of each alphabet keyword. Only the `nogap` keywords exclude the gap `-`.
pub static ALPHABETS:phf::Map<&'static str, &'static str> = phf_map! {
    "dna" => "ACGTN-",
    "rna" => "ACGUN-",
    "nucl" => "ACGTUN-",
    "aa" => "ARNDCQEGHILKMFPSTWYV*-",
    "aax" => "ARNDCQEGHILKMFPSTWYVBZX*-",
    "all" => "ABCDEFGHIJKLMNOPQRSTUVWXYZ*-",
    "dnanogap" => "ACGTN",
    "rnanogap" => "ACGUN",
    "nuclnogap" => "ACGTUN",
    "aanogap" => "ARNDCQEGHILKMFPSTWYV*",
    "aaxnogap" => "ARNDCQEGHILKMFPSTWYVBZX*",
    "allnogap" => "ABCDEFGHIJKLMNOPQRSTUVWXYZ*",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_seq_type_nucleotides_with_n_and_iupac() {
        assert_eq!(detect_seq_type(b"ACGTNNACGT"), Some(SeqType::DNA));
        assert_eq!(detect_seq_type(b"ACGUACGUACGUACGUACGR"), Some(SeqType::RNA));
        assert_eq!(detect_seq_type(b"ACGACG-acg"), Some(SeqType::Nucl));
    }

    #[test]
    fn detect_seq_type_amino_acids() {
        assert_eq!(detect_seq_type(b"MTSNRARVTYNPPPTTTGTR"), Some(SeqType::AA));
    }

    #[test]
    fn detect_seq_type_nucleotide_fraction_boundary() {
        assert_eq!(detect_seq_type(b"ACGTACGTAR"), Some(SeqType::DNA));
        assert_eq!(detect_seq_type(b"ACGTACGTRY"), Some(SeqType::AA));
    }

    #[test]
    fn detect_seq_type_without_letters() {
        assert_eq!(detect_seq_type(b""), None);
        assert_eq!(detect_seq_type(b"--- 12\t"), None);
    }
}
