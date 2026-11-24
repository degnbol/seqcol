use anyhow::{anyhow, ensure, Context, Result};
use phf::phf_map;
use std::collections::HashSet;
use std::fs;
use std::str::FromStr;
use std::sync::LazyLock;

use crate::charmap::CharMap;

/// Type of a biological sequence.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqType {
    AA,
    Dna,
    Rna,
    /// DNA with IUPAC ambiguity codes
    Dnax,
    /// RNA with IUPAC ambiguity codes
    Rnax,
}

impl SeqType {
    /// All sequence types, in the order of their index (`as usize`).
    pub const ALL: [SeqType; 5] = [SeqType::AA, SeqType::Dna, SeqType::Rna, SeqType::Dnax, SeqType::Rnax];

    /// Key of [`ALPHABETS`] whose letters a sequence of this type has.
    fn keyword(self) -> &'static str {
        match self {
            SeqType::AA => "aa",
            SeqType::Dna => "dna",
            SeqType::Rna => "rna",
            SeqType::Dnax => "dnax",
            SeqType::Rnax => "rnax",
        }
    }
}

/// Sequence types of an alphabet keyword.
///
/// # Arguments
/// * `keyword` - Key of [`ALPHABETS`]
///
/// # Returns
/// The types whose sequences the keyword alphabet covers, in the order of [`SeqType::ALL`].
/// A keyword with ambiguity codes also covers the types without them, e.g. "dnax" covers `Dna` and `Dnax`.
///
/// # Panics
/// If `keyword` is not a key of [`ALPHABETS`].
pub fn seq_types(keyword: &'static str) -> &'static [SeqType] {
    use SeqType::*;
    match keyword {
        "dna" => &[Dna],
        "rna" => &[Rna],
        "nucl" => &[Dna, Rna],
        "dnax" => &[Dna, Dnax],
        "rnax" => &[Rna, Rnax],
        "nuclx" => &[Dna, Rna, Dnax, Rnax],
        "aa" | "aax" => &[AA],
        "bio" => &SeqType::ALL,
        _ => panic!("{keyword}: not an alphabet keyword"),
    }
}

/// Keys of [`ALPHABETS`] other than "bio", from general to specific:
/// each keyword comes before the keywords whose alphabets are subsets of its alphabet.
pub const TABLE_ORDER: [&str; 8] = ["nuclx", "nucl", "dnax", "dna", "rnax", "rna", "aax", "aa"];

/// Alphabet keyword with ambiguity codes.
///
/// # Arguments
/// * `keyword` - Key of [`ALPHABETS`]
///
/// # Returns
/// "dnax" for "dna", "rnax" for "rna", "nuclx" for "nucl", "aax" for "aa", else `keyword`.
pub fn ambiguous(keyword: &'static str) -> &'static str {
    match keyword {
        "dna" => "dnax",
        "rna" => "rnax",
        "nucl" => "nuclx",
        "aa" => "aax",
        _ => keyword,
    }
}

/// Minimum log-likelihood ratio of the most likely class over each other class for a conclusive classification:
/// ln(0.99 / 0.01), a posterior probability of at least 0.99 against each other class, with equal priors.
pub const MIN_LOG_LIKELIHOOD_MARGIN: f64 = 4.59511985013459; // ln 99

/// Probability given to a letter that is not in an alphabet, so that one letter cannot rule out a type.
const MIN_LETTER_PROBABILITY: f64 = 1e-4;

/// ln P(c | type) of each byte c, for each type of [`SeqType::ALL`]. 0 for bytes that are not ASCII letters.
static LETTER_LOG_LIKELIHOODS: LazyLock<[[f64; 5]; 256]> = LazyLock::new(|| {
    let aa_composition: Vec<(u8, f64)> = include_str!("../data/aa_composition.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .skip(1)
        .map(|line| {
            let (letter, p) = line.split_once('\t').unwrap();
            (letter.as_bytes()[0], p.parse().unwrap())
        })
        .collect();
    let mut log_likelihoods = [[0.0; 5]; 256];
    for c in b'A'..=b'Z' {
        for seq_type in SeqType::ALL {
            let p = match seq_type {
                SeqType::AA => aa_composition.iter().find(|&&(letter, _)| letter == c).map(|&(_, p)| p),
                _ => {
                    let letters = ALPHABETS[seq_type.keyword()].bytes().filter(u8::is_ascii_alphabetic);
                    letters.clone().any(|letter| letter == c).then(|| 1.0 / letters.count() as f64)
                }
            };
            let log_likelihood = p.unwrap_or(MIN_LETTER_PROBABILITY).ln();
            log_likelihoods[c as usize][seq_type as usize] = log_likelihood;
            log_likelihoods[c.to_ascii_lowercase() as usize][seq_type as usize] = log_likelihood;
        }
    }
    log_likelihoods
});

/// Log-likelihood of text for each sequence type, with each letter independent.
///
/// P(c | type) is the UniProtKB/Swiss-Prot composition for amino acids,
/// and uniform over the letters of the type's alphabet for nucleotide types.
/// A letter outside these gets a small probability (`MIN_LETTER_PROBABILITY`).
///
/// # Arguments
/// * `text` - Sequence text. Case is ignored, and bytes that are not ASCII letters add 0.
///
/// # Returns
/// ln P(text | type) for each type of [`SeqType::ALL`].
pub fn log_likelihoods(text: &[u8]) -> [f64; 5] {
    let mut sums = [0.0; 5];
    for &c in text {
        for (sum, log_likelihood) in sums.iter_mut().zip(LETTER_LOG_LIKELIHOODS[c as usize]) {
            *sum += log_likelihood;
        }
    }
    sums
}

/// Index of the largest value. Ties give the lowest index.
fn argmax(values: impl IntoIterator<Item = f64>) -> usize {
    let mut i_max = 0;
    let mut max = f64::NEG_INFINITY;
    for (i, value) in values.into_iter().enumerate() {
        if value > max {
            (i_max, max) = (i, value);
        }
    }
    i_max
}

/// Whether the largest log-likelihood exceeds every other by at least [`MIN_LOG_LIKELIHOOD_MARGIN`].
///
/// # Arguments
/// * `log_likelihoods` - Log-likelihood of each class
pub fn is_conclusive(log_likelihoods: &[f64]) -> bool {
    let i_max = argmax(log_likelihoods.iter().copied());
    let max = log_likelihoods[i_max];
    log_likelihoods.iter().enumerate().all(|(i, &value)| i == i_max || max - value >= MIN_LOG_LIKELIHOOD_MARGIN)
}

/// Most likely class of a text, from its log-likelihoods and those of other texts that are likely of the same class.
///
/// # Arguments
/// * `text` - Log-likelihood of the text for each class
/// * `others` - Sum of the log-likelihoods of the other texts for each class, in the order of `text`
///
/// # Returns
/// The index of the largest `text` value if it is conclusive (see [`is_conclusive`]), else of the largest `text + others`.
/// Ties give the lowest index.
pub fn most_likely(text: &[f64], others: &[f64]) -> usize {
    if is_conclusive(text) {
        argmax(text.iter().copied())
    } else {
        argmax(text.iter().zip(others).map(|(a, b)| a + b))
    }
}

/// Read the characters of an alphabet file.
///
/// # Arguments
/// * `path` - File with the alphabet characters
///
/// # Returns
/// The set of the characters of the file, without whitespace.
///
/// # Errors
/// If reading the file fails or it has no characters other than whitespace.
pub fn read_alphabet_file(path: &str) -> Result<CharMap<bool>> {
    let text = fs::read_to_string(path).with_context(|| path.to_string())?;
    let alphabet: CharMap<bool> = text.chars().filter(|c| !c.is_whitespace()).collect();
    ensure!(!alphabet.by_char.is_empty(), "{path}: no characters other than whitespace.");
    Ok(alphabet)
}

/// Gap characters of alignments.
pub const GAPS: &str = "-.";

/// Characters of each alphabet keyword. Each alphabet includes [`GAPS`].
pub static ALPHABETS: phf::Map<&'static str, &'static str> = phf_map! {
    "dna" => "ACGTN-.",
    "rna" => "ACGUN-.",
    "nucl" => "ACGTUN-.",
    "dnax" => "ACGTNRYKMSWBDHV-.",
    "rnax" => "ACGUNRYKMSWBDHV-.",
    "nuclx" => "ACGTUNRYKMSWBDHV-.",
    "aa" => "ARNDCQEGHILKMFPSTWYV*-.",
    "aax" => "ARNDCQEGHILKMFPSTWYVBZX*-.",
    "bio" => "ABCDEFGHIKLMNPQRSTUVWXYZ*-.",
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

/// Alphabet of specs.
///
/// # Arguments
/// * `specs` - Keywords and edits
/// * `base` - Characters to include with the keyword characters
///
/// # Returns
/// `base` with the characters of all keywords, then the `add` characters of all `specs`,
/// then without the `remove` characters of all `specs`.
///
/// # Errors
/// If no characters are left.
pub fn alphabet(specs: &[AlphabetSpec], base: CharMap<bool>) -> Result<CharMap<bool>> {
    let mut alphabet = base;
    let keyword_chars = specs.iter().filter_map(|spec| spec.keyword).flat_map(|keyword| ALPHABETS[keyword].chars());
    let add_chars = specs.iter().flat_map(|spec| &spec.add).copied();
    alphabet.by_char.extend(keyword_chars.chain(add_chars).map(|c| (c, true)));
    alphabet.by_char.extend(specs.iter().flat_map(|spec| &spec.remove).map(|&c| (c, false)));
    ensure!(alphabet.other == Some(true) || alphabet.by_char.values().any(|&value| value), "empty alphabet");
    Ok(alphabet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn most_likely_type(text: &[u8]) -> SeqType {
        SeqType::ALL[most_likely(&log_likelihoods(text), &[0.0; 5])]
    }

    #[test]
    fn log_likelihoods_classify_letters() {
        for (text, seq_type) in [
            (&b"ACGT"[..], SeqType::Dna),
            (b"acgt", SeqType::Dna),
            (b"ACGU", SeqType::Rna),
            (b"GTYRAC", SeqType::Dnax),
            (b"MKLVWE", SeqType::AA),
        ] {
            assert_eq!(most_likely_type(text), seq_type, "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn log_likelihoods_without_letters() {
        assert_eq!(log_likelihoods(b""), [0.0; 5]);
        assert_eq!(log_likelihoods(b"--- 12\t"), [0.0; 5]);
    }

    #[test]
    fn most_likely_uses_others_only_below_the_margin() {
        assert_eq!(most_likely(&[0.0, -5.0], &[-100.0, 0.0]), 0);
        assert_eq!(most_likely(&[0.0, -1.0], &[-10.0, 0.0]), 1);
        assert_eq!(most_likely(&[0.0, 0.0], &[0.0, 0.0]), 0);
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
            ("all", "all: not an alphabet keyword (for a file, use -f)."),
            ("dna^", "dna^: no character after ^."),
            ("+", "+: no character after +."),
            ("", "empty alphabet"),
        ] {
            let error = value.parse::<AlphabetSpec>().unwrap_err();
            assert_eq!(error.to_string(), message, "{value:?}");
        }
    }

    /// The characters of `alphabet` among the `chars`.
    fn members(alphabet: &CharMap<bool>, chars: &str) -> HashSet<char> {
        chars.chars().filter(|&c| alphabet.contains(c)).collect()
    }

    #[test]
    fn alphabet_of_specs() {
        let printable: String = (' '..='~').collect();
        let union = alphabet(&[spec(Some("dna"), "", ""), spec(Some("aa"), "", "")], CharMap::default()).unwrap();
        assert_eq!(members(&union, &printable), set(ALPHABETS["dna"]).union(&set(ALPHABETS["aa"])).copied().collect());
        let edited = alphabet(&[spec(Some("dna"), "X", ""), spec(None, "", "X-.")], CharMap::default()).unwrap();
        assert_eq!(members(&edited, &printable), set("ACGTN"));
        let base = || "AB".chars().collect();
        assert_eq!(members(&alphabet(&[spec(None, "Z", "A")], base()).unwrap(), &printable), set("BZ"));
        let all_but_gap = alphabet(&[spec(None, "", "-")], CharMap { by_char: HashMap::new(), other: Some(true) }).unwrap();
        assert_eq!(members(&all_but_gap, "a-é"), set("aé"));
        assert!(alphabet(&[spec(None, "", "AB")], base()).is_err());
    }

    #[test]
    fn alphabets_have_gaps() {
        for (keyword, alphabet) in ALPHABETS.entries() {
            assert!(GAPS.chars().all(|gap| alphabet.contains(gap)), "{keyword}");
        }
    }

    #[test]
    fn bio_is_nuclx_and_aax() {
        assert_eq!(set(ALPHABETS["bio"]), set(ALPHABETS["nuclx"]).union(&set(ALPHABETS["aax"])).copied().collect());
    }
}
