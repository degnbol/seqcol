use phf::phf_map;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};

use crate::inout;

/// Sequence type detected from content.
/// Uses all-caps for biological acronyms (DNA, RNA, AA) for readability.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeqType {
    DNA,
    RNA,
    Nucl, // Could be either DNA or RNA (only A, C, G seen, or mixed T/U)
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

// Characters that are ONLY valid in amino acids (not nucleotides)
const AA_ONLY_CHARS: &[u8] = b"DEFHIKLMNPQRSVWY";

/// Detect sequence type from a line of sequence characters.
/// Returns None if line is empty or contains only gaps/whitespace/ambiguous bases.
pub fn detect_seq_type(line: &[u8]) -> Option<SeqType> {
    let mut has_t = false;
    let mut has_u = false;
    let mut has_any = false;

    for c in line.iter().filter(|c| c.is_ascii_alphabetic()) {
        let c = c.to_ascii_uppercase();
        has_any = true;
        if AA_ONLY_CHARS.contains(&c) {
            return Some(SeqType::AA);
        }
        if c == b'T' {
            has_t = true;
        }
        if c == b'U' {
            has_u = true;
        }
    }

    if !has_any {
        return None;
    }

    // No AA-only chars found, must be nucleotide
    match (has_t, has_u) {
        (true, false) => Some(SeqType::DNA),
        (false, true) => Some(SeqType::RNA),
        _ => Some(SeqType::Nucl), // Mixed T/U or only A,C,G - treat as generic nucleotide
    }
}

/// Detect sequence type by peeking at input files.
/// Reads until first sequence line (non-header) is found.
pub fn detect_seq_type_from_files(files: &[String]) -> Option<SeqType> {
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
            // Skip fasta/fastq headers
            if line.starts_with(b">") || line.starts_with(b"@") || line.starts_with(b"+") {
                continue;
            }
            if let Some(seq_type) = detect_seq_type(&line) {
                return Some(seq_type);
            }
        }
    }
    None
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

pub static ALPHABETS: phf::Map<&'static str, &'static str> = phf_map! {
    "dna" => "ACGT-",
    "rna" => "ACGU-",
    "nucl" => "ACGTU-",
    "aa" => "ARNDCQEGHILKMFPSTWYV-",
    "aax" => "ARNDCQEGHILKMFPSTWYVBZX-",
    "all" => "ACGTURNDQEHILKMFPSWYVBZX-",
    "dnanogap" => "ACGT",
    "rnanogap" => "ACGU",
    "nuclnogap" => "ACGTU",
    "aanogap" => "ARNDCQEGHILKMFPSTWYV",
    "aaxnogap" => "ARNDCQEGHILKMFPSTWYVBZX",
    "allnogap" => "ACGTURNDQEHILKMFPSWYVBZX",
};
