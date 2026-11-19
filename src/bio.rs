use anyhow::{anyhow, ensure, Context, Result};
use phf::phf_map;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::str::FromStr;
use std::sync::LazyLock;

/// Nucleotides (DNA or RNA) or amino acids.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeqType {
    Nucl,
    AA,
}

impl SeqType {
    /// All sequence types.
    pub const ALL: [SeqType; 2] = [SeqType::Nucl, SeqType::AA];

    /// Sequence type of an alphabet keyword.
    ///
    /// # Arguments
    /// * `keyword` - Key of `ALPHABETS`
    ///
    /// # Returns
    /// The type of the keyword, or `None` for "all" and for text that is not a keyword.
    pub fn from_alphabet(keyword: &str) -> Option<SeqType> {
        match keyword {
            "dna" | "rna" | "nucl" => Some(SeqType::Nucl),
            "aa" | "aax" => Some(SeqType::AA),
            _ => None,
        }
    }

    /// Sequence type of a text from its log odds and the log odds of other texts.
    ///
    /// # Arguments
    /// * `text` - ln(P(text | nucleotide) / P(text | amino acid))
    /// * `others` - Sum of the log odds of other texts that are likely of the same type
    ///
    /// # Returns
    /// The sign of `text` if |`text`| ≥ [`MIN_ABS_LOG_ODDS`], else the sign of `text + others`:
    /// `Nucl` if positive, else `AA`.
    pub fn from_log_odds(text: f64, others: f64) -> SeqType {
        let log_odds = if text.abs() >= MIN_ABS_LOG_ODDS { text } else { text + others };
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

/// Minimum |log odds| for a conclusive sequence type: ln(p / (1 − p)) for the posterior probability p = 0.99.
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

/// Read the characters of an alphabet file.
///
/// # Arguments
/// * `path` - File with the alphabet characters
///
/// # Returns
/// The characters of the file, without whitespace.
///
/// # Errors
/// If reading the file fails or it has no characters other than whitespace.
pub fn read_alphabet_file(path: &str) -> Result<HashSet<char>> {
    let text = fs::read_to_string(path).with_context(|| path.to_string())?;
    let alphabet: HashSet<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    ensure!(!alphabet.is_empty(), "{path}: no characters other than whitespace.");
    Ok(alphabet)
}

/// Gap characters of alignments.
pub const GAPS: &str = "-.";

/// Characters of each alphabet keyword. Each alphabet includes [`GAPS`].
pub static ALPHABETS:phf::Map<&'static str, &'static str> = phf_map! {
    "dna" => "ACGTN-.",
    "rna" => "ACGUN-.",
    "nucl" => "ACGTUN-.",
    "aa" => "ARNDCQEGHILKMFPSTWYV*-.",
    "aax" => "ARNDCQEGHILKMFPSTWYVBZX*-.",
    "all" => "ABCDEFGHIJKLMNOPQRSTUVWXYZ*-.",
};

/// An optional alphabet keyword, with characters to add and to remove.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlphabetSpec {
    /// Key of `ALPHABETS`.
    pub keyword: Option<&'static str>,
    /// Characters to add.
    pub add: HashSet<char>,
    /// Characters to remove. Removal has priority over addition.
    pub remove: HashSet<char>,
}

impl FromStr for AlphabetSpec {
    type Err = anyhow::Error;

    /// Parse `[keyword]` followed by any number of `+chars` (add) and `^chars` (remove).
    ///
    /// Ignores whitespace. Ignores the case of the keyword, but not of the edit characters.
    /// The first character after a marker is always literal, so `++` adds `+` and `^^` removes `^`.
    ///
    /// # Arguments
    /// * `value` - Text such as `dna^-` or `aa+X`
    ///
    /// # Returns
    /// The keyword and the characters to add and to remove.
    ///
    /// # Errors
    /// If `value` has only whitespace, the text before the first marker is not a keyword, or a marker ends the value.
    fn from_str(value: &str) -> Result<Self> {
        let text: String = value.chars().filter(|c| !c.is_whitespace()).collect();
        ensure!(!text.is_empty(), "empty alphabet");
        let (name, edits) = text.split_at(text.find(['+', '^']).unwrap_or(text.len()));
        let keyword = match name {
            "" => None,
            _ => Some(
                *ALPHABETS
                    .get_key(&name.to_lowercase())
                    .ok_or_else(|| anyhow!("{name}: not an alphabet keyword (for a file, use -f)."))?,
            ),
        };
        let mut spec = AlphabetSpec { keyword, ..Default::default() };
        let mut removing = false;
        let mut chars = edits.chars();
        while let Some(c) = chars.next() {
            let c = match c {
                '+' | '^' => {
                    removing = c == '^';
                    chars.next().ok_or_else(|| anyhow!("{value}: no character after {c}."))?
                }
                _ => c,
            };
            if removing { spec.remove.insert(c) } else { spec.add.insert(c) };
        }
        Ok(spec)
    }
}

impl AlphabetSpec {
    /// Characters of the alphabet.
    ///
    /// # Arguments
    /// * `base` - Characters to include with the characters of the keyword
    ///
    /// # Returns
    /// The union of `base`, the keyword characters, and `add`, minus `remove`.
    /// If `base` and `keyword` are both `None`, the keyword is "all".
    ///
    /// # Errors
    /// If no characters are left.
    pub fn chars(&self, base: Option<HashSet<char>>) -> Result<HashSet<char>> {
        let keyword = match (&base, self.keyword) {
            (None, None) => Some("all"),
            (_, keyword) => keyword,
        };
        let mut alphabet = base.unwrap_or_default();
        alphabet.extend(keyword.map_or("", |keyword| ALPHABETS[keyword]).chars());
        alphabet.extend(&self.add);
        alphabet.retain(|c| !self.remove.contains(c));
        ensure!(!alphabet.is_empty(), "empty alphabet");
        Ok(alphabet)
    }
}

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
    fn from_log_odds_conclusive_text_ignores_others() {
        assert_eq!(SeqType::from_log_odds(5.0, -100.0), SeqType::Nucl);
        assert_eq!(SeqType::from_log_odds(-5.0, 100.0), SeqType::AA);
    }

    fn set(chars: &str) -> HashSet<char> {
        chars.chars().collect()
    }

    fn spec(keyword: Option<&'static str>, add: &str, remove: &str) -> AlphabetSpec {
        AlphabetSpec { keyword, add: set(add), remove: set(remove) }
    }

    #[test]
    fn alphabet_spec_from_str() {
        for (value, expected) in [
            ("AA^-J", spec(Some("aa"), "", "-J")),
            ("dna+X^-", spec(Some("dna"), "X", "-")),
            ("^-", spec(None, "", "-")),
            ("+x", spec(None, "x", "")),
            ("DNA++^X", spec(Some("dna"), "+", "X")),
            ("+^X", spec(None, "^X", "")),
            ("dna ^ -", spec(Some("dna"), "", "-")),
        ] {
            assert_eq!(value.parse::<AlphabetSpec>().unwrap(), expected, "{value:?}");
        }
    }

    #[test]
    fn alphabet_spec_from_str_errors() {
        for (value, message) in [
            ("nope^-", "nope: not an alphabet keyword (for a file, use -f)."),
            ("dna^", "dna^: no character after ^."),
            ("+", "+: no character after +."),
            ("", "empty alphabet"),
        ] {
            let error = value.parse::<AlphabetSpec>().unwrap_err();
            assert_eq!(error.to_string(), message, "{value:?}");
        }
    }

    #[test]
    fn alphabet_spec_chars() {
        assert_eq!(spec(Some("dna"), "X", "-.").chars(None).unwrap(), set("ACGTNX"));
        assert_eq!(spec(Some("dna"), "", "").chars(Some(set("Z"))).unwrap(), set("ACGTN-.Z"));
        assert_eq!(spec(None, "", "A").chars(Some(set("AB"))).unwrap(), set("B"));
        assert_eq!(spec(None, "", "-.").chars(None).unwrap(), set("ABCDEFGHIJKLMNOPQRSTUVWXYZ*"));
        assert_eq!(spec(None, "X", "X").chars(Some(set("A"))).unwrap(), set("A"));
        assert!(spec(None, "", "AB").chars(Some(set("AB"))).is_err());
    }

    #[test]
    fn alphabets_have_gaps() {
        for (keyword, alphabet) in ALPHABETS.entries() {
            assert!(GAPS.chars().all(|gap| alphabet.contains(gap)), "{keyword}");
        }
    }

    #[test]
    fn from_log_odds_inconclusive_text_uses_others() {
        assert_eq!(SeqType::from_log_odds(1.0, -10.0), SeqType::AA);
        assert_eq!(SeqType::from_log_odds(-1.0, 10.0), SeqType::Nucl);
        assert_eq!(SeqType::from_log_odds(0.0, 0.0), SeqType::AA);
    }
}
