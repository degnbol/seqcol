use phf::phf_map;

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
const AA_ONLY_CHARS: &str = "DEFHIKLMNPQRSVWY";

/// Detect sequence type from a line of sequence characters.
/// Returns None if line is empty or contains only gaps/whitespace/ambiguous bases.
pub fn detect_seq_type(line: &str) -> Option<SeqType> {
    let mut has_t = false;
    let mut has_u = false;
    let mut has_any = false;

    for c in line.chars().filter(|c| c.is_ascii_alphabetic()) {
        let c = c.to_ascii_uppercase();
        has_any = true;
        if AA_ONLY_CHARS.contains(c) {
            return Some(SeqType::AA);
        }
        if c == 'T' {
            has_t = true;
        }
        if c == 'U' {
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
