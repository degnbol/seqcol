use phf::phf_map;
use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::sync::LazyLock;

/// Nucleotides (DNA or RNA) or amino acids.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeqType {
    Nucl,
    AA,
}

impl SeqType {
    /// Sequence type of an alphabet keyword.
    ///
    /// # Arguments
    /// * `alphabet` - Keyword of `ALPHABETS`, ignoring case and spaces
    ///
    /// # Returns
    /// The type of the keyword, or `None` for "all" and for text that is not a keyword.
    pub fn from_alphabet(alphabet: &str) -> Option<SeqType> {
        match alphabet.to_lowercase().replace(' ', "").as_str() {
            "dna" | "dnanogap" | "rna" | "rnanogap" | "nucl" | "nuclnogap" => Some(SeqType::Nucl),
            "aa" | "aanogap" | "aax" | "aaxnogap" => Some(SeqType::AA),
            _ => None,
        }
    }

    /// Sequence type of a run from its log odds and the log odds of other runs.
    ///
    /// # Arguments
    /// * `run` - ln(P(run | nucleotide) / P(run | amino acid))
    /// * `others` - Sum of the log odds of other runs that are likely of the same type
    ///
    /// # Returns
    /// The sign of `run` if |`run`| ≥ [`MIN_ABS_LOG_ODDS`], else the sign of `run + others`:
    /// `Nucl` if positive, else `AA`.
    pub fn from_log_odds(run: f64, others: f64) -> SeqType {
        let log_odds = if run.abs() >= MIN_ABS_LOG_ODDS { run } else { run + others };
        if log_odds > 0.0 { SeqType::Nucl } else { SeqType::AA }
    }
}

impl fmt::Display for SeqType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            SeqType::Nucl => "nucl",
            SeqType::AA => "aa",
        })
    }
}

/// Log odds that decide a sequence type: ln(p / (1 − p)) for the posterior probability p = 0.99.
pub const MIN_ABS_LOG_ODDS: f64 = 4.59511985013459; // ln 99

/// Probability given to a letter that is not in an alphabet, so that no letter has infinite log odds.
const MIN_LETTER_PROBABILITY: f64 = 1e-4;

/// ln(P_nucl(c) / P_aa(c)) of each byte c. 0 for bytes that are not letters.
static LETTER_LOG_ODDS: LazyLock<[f64; 256]> = LazyLock::new(|| {
    let mut log_odds = [0.0; 256];
    for line in include_str!("../data/letter_frequencies.tsv").lines().filter(|l| !l.starts_with('#')).skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        let c = fields[0].as_bytes()[0];
        let [p_nucl, p_aa] = [fields[1], fields[2]].map(|p| p.parse::<f64>().unwrap().max(MIN_LETTER_PROBABILITY));
        log_odds[c as usize] = (p_nucl / p_aa).ln();
    }
    for c in b'A'..=b'Z' {
        log_odds[c.to_ascii_lowercase() as usize] = log_odds[c as usize];
    }
    log_odds
});

/// Log odds that text is nucleotides rather than amino acids, with each letter independent.
///
/// # Arguments
/// * `text` - Sequence text. Case is ignored, and characters that are not letters add 0.
///
/// # Returns
/// ln(P(text | nucleotide) / P(text | amino acid)): positive for nucleotides, negative for amino acids.
pub fn nucl_log_odds(text: &[u8]) -> f64 {
    text.iter().map(|&c| LETTER_LOG_ODDS[c as usize]).sum()
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
    fn nucl_log_odds_nucleotides() {
        for text in [&b"ACGTNNACGT"[..], b"ACGUACGU", b"NNNNNNNNNN", b"GAYCARATHGAYGG", b"acgtacgt"] {
            assert!(nucl_log_odds(text) > 0.0, "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn nucl_log_odds_amino_acids() {
        for text in [&b"MTSNRARVTYNPPPTTTGTR"[..], b"GATAMKLV"] {
            assert!(nucl_log_odds(text) < 0.0, "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn nucl_log_odds_without_letters() {
        assert_eq!(nucl_log_odds(b""), 0.0);
        assert_eq!(nucl_log_odds(b"--- 12\t"), 0.0);
    }

    #[test]
    fn from_log_odds_decided_run_ignores_others() {
        assert_eq!(SeqType::from_log_odds(5.0, -100.0), SeqType::Nucl);
        assert_eq!(SeqType::from_log_odds(-5.0, 100.0), SeqType::AA);
    }

    #[test]
    fn from_log_odds_undecided_run_uses_others() {
        assert_eq!(SeqType::from_log_odds(1.0, -10.0), SeqType::AA);
        assert_eq!(SeqType::from_log_odds(-1.0, 10.0), SeqType::Nucl);
        assert_eq!(SeqType::from_log_odds(0.0, 0.0), SeqType::AA);
    }
}
