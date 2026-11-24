use anyhow::{anyhow, Context, Result};
use include_dir::include_dir;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use serde::Deserialize;
use std::io::{self, Write};
use std::sync::LazyLock;
use yansi::Color::{self, *};
use yansi::Style;

use crate::bio::{self, SeqType};
use crate::charmap::CharMap;
use crate::inout::open;
use crate::ansi_colors::{COLOR_NAMES,parse_hex};

/// Colors of characters, for all text and for sequence types.
#[derive(Clone, Debug, Default)]
pub struct Scheme {
    /// Description of the colorscheme. Empty if it has none.
    pub description: String,
    /// Colors for all text.
    pub colors: CharMap<Color>,
    /// Colors for sequence types, by alphabet keyword of [`bio::TABLE_ORDER`].
    /// A table applies to the types of its keyword with ambiguity codes, e.g. "nucl" to `Dnax`.
    pub tables: HashMap<&'static str, HashMap<char, Color>>,
}

impl Scheme {
    /// Tables that color a sequence type.
    ///
    /// # Arguments
    /// * `seq_type` - Sequence type
    ///
    /// # Returns
    /// The colors of each table whose keyword with ambiguity codes covers `seq_type`,
    /// from general to specific (in the order of [`bio::TABLE_ORDER`]).
    pub fn tables_of(&self, seq_type: SeqType) -> impl Iterator<Item = &HashMap<char, Color>> {
        bio::TABLE_ORDER.into_iter().filter_map(move |keyword| {
            let table = self.tables.get(keyword)?;
            bio::seq_types(bio::ambiguous(keyword)).contains(&seq_type).then_some(table)
        })
    }

    /// Colors of characters in sequences of a type.
    ///
    /// # Arguments
    /// * `seq_type` - Sequence type
    ///
    /// # Returns
    /// The colors for all text, overlaid by the tables that color `seq_type` (see [`Scheme::tables_of`]),
    /// more specific tables taking precedence.
    pub fn colors(&self, seq_type: SeqType) -> CharMap<Color> {
        let mut colors = self.colors.clone();
        for table in self.tables_of(seq_type) {
            colors.by_char.extend(table);
        }
        colors
    }

    /// Characters of the scheme.
    ///
    /// # Returns
    /// The keys of `colors` and of `tables`, and the alphabets of the table keywords (see [`bio::ALPHABETS`]).
    /// All characters if `colors` has a color for all characters.
    pub fn alphabet(&self) -> CharMap<bool> {
        let keyword_chars = self.tables.keys().flat_map(|keyword| bio::ALPHABETS[keyword].chars());
        let keys = self.colors.by_char.keys().chain(self.tables.values().flat_map(HashMap::keys)).copied();
        CharMap { other: self.colors.other.map(|_| true), ..keyword_chars.chain(keys).collect() }
    }
}

/// Parse a colorscheme.
///
/// # Arguments
/// * `text` - TOML with an optional `description` string, an optional table `colors` (all text),
///   and optional tables named by the keywords of [`bio::TABLE_ORDER`] (sequence types, see [`Scheme::colors`]).
///   Each table maps characters to colors (see [`parse_color`]), e.g. `"#" = "red"`.
///
/// # Returns
/// The colors and the description.
///
/// # Errors
/// On invalid TOML, an unknown key or table, a key that is not one character, or a color that does not parse.
pub fn parse_colorscheme(text: &str) -> Result<Scheme> {
    let file: SchemeFile = toml::from_str(text)?;
    let tables = file
        .tables
        .iter()
        .map(|(name, table)| {
            let keyword = bio::TABLE_ORDER.into_iter().find(|keyword| keyword == name).ok_or_else(|| {
                anyhow!("[{name}]: not a table name. Tables: colors, {}", bio::TABLE_ORDER.join(", "))
            })?;
            Ok((keyword, parse_colors(table).with_context(|| format!("[{name}]"))?))
        })
        .collect::<Result<_>>()?;
    Ok(Scheme {
        description: file.description,
        colors: CharMap { by_char: parse_colors(&file.colors).context("[colors]")?, other: None },
        tables,
    })
}

#[derive(Deserialize)]
struct SchemeFile {
    #[serde(default)]
    description: String,
    #[serde(default)]
    colors: BTreeMap<String, String>,
    /// Tables for sequence types, by name.
    #[serde(flatten)]
    tables: BTreeMap<String, BTreeMap<String, String>>,
}

/// Parse colors of characters.
///
/// # Arguments
/// * `table` - Colors (see [`parse_color`]) by one-character keys.
///
/// # Returns
/// Color of each character.
///
/// # Errors
/// On a key that is not one character, or a color that does not parse.
fn parse_colors(table: &BTreeMap<String, String>) -> Result<HashMap<char, Color>> {
    table
        .iter()
        .map(|(key, value)| {
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return Err(anyhow!("{key:?}: key is not one character"));
            };
            let color = parse_color(value).map_err(|e| anyhow!("{key:?} = {value:?}: {e}"))?;
            Ok((c, color))
        })
        .collect()
}

/// Builtin colorschemes: the `.toml` files of `data/colorschemes/`, by file name without extension.
///
/// # Panics
/// On a builtin that does not parse.
pub fn load_colorschemes() -> BTreeMap<String, Scheme> {
    include_dir!("data/colorschemes/")
        .files()
        .filter(|file| file.path().extension() == Some("toml".as_ref()))
        .map(|file| {
            let name = file.path().file_stem().unwrap().to_str().unwrap().to_string();
            let scheme = parse_colorscheme(file.contents_utf8().unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, scheme)
        })
        .collect()
}

/// Parse a color.
///
/// # Arguments
/// * `coltext` - A hex code (`#ff0000`), RGB values (`255 0 0` or `255,0,0`), or a color name (`red`, `light red`).
///
/// # Returns
/// The color.
///
/// # Errors
/// On an RGB value above 255, an unknown color name, or text in none of these forms.
pub fn parse_color(coltext: &str) -> Result<Color, &'static str> {
    static RE_HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#([0-9a-fA-F]{6})$").unwrap());
    static RE_RGB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([0-9]+)[\s,]+([0-9]+)[\s,]+([0-9]+)$").unwrap());
    static RE_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Za-z ]+[0-9]*)$").unwrap());

    let coltext = coltext.trim();

    if let Some(m) = RE_HEX.captures(coltext) {
        return Ok(parse_hex(&m[1]));
    }
    if let Some(m) = RE_RGB.captures(coltext) {
        let channel = |i: usize| m[i].parse::<u8>().map_err(|_| "RGB value above 255.");
        return Ok(Rgb(channel(1)?, channel(2)?, channel(3)?));
    }
    if let Some(m) = RE_NAME.captures(coltext) {
        let col_name = m[1].to_lowercase().replace(' ', "");
        return match COLOR_NAMES.get(&col_name) {
            None => Err("Unknown color name."),
            Some(col) => Ok(*col),
        };
    }
    Err("Color description couldn't be parsed.")
}

/// Parse a text style.
///
/// # Arguments
/// * `text` - Whitespace-separated words. "bold", "underline", and "italic" (any case) add that attribute.
///   The other words, joined by spaces, give the background color (see `parse_color`).
///   E.g. "bold", "red", "Bold 128,128,128".
///
/// # Returns
/// The style.
///
/// # Errors
/// On text without words, or with an invalid color.
pub fn parse_style(text: &str) -> Result<Style, &'static str> {
    let mut style = Style::new();
    let mut color_words = vec![];
    for word in text.split_whitespace() {
        match word.to_lowercase().as_str() {
            "bold" => style = style.bold(),
            "underline" => style = style.underline(),
            "italic" => style = style.italic(),
            _ => color_words.push(word),
        }
    }
    if !color_words.is_empty() {
        style = style.bg(parse_color(&color_words.join(" "))?);
    } else if style == Style::new() {
        return Err("Style is empty.");
    }
    Ok(style)
}

/// Get a colorscheme by name, color, or file path.
///
/// # Arguments
/// * `name` - Name of a `builtins` scheme, else a color (see [`parse_color`]) for all characters,
///   else path of a colorscheme file (see [`parse_colorscheme`])
/// * `builtins` - Colorschemes by name
///
/// # Returns
/// The scheme. A color gives a scheme with that color for all characters, and no tables.
///
/// # Errors
/// If the file does not open, read, or parse. The message starts with `name`.
pub fn read_scheme(name: &str, builtins: &BTreeMap<String, Scheme>) -> Result<Scheme> {
    if let Some(scheme) = builtins.get(name) {
        return Ok(scheme.clone());
    }
    if let Ok(color) = parse_color(name) {
        let colors = CharMap { by_char: HashMap::new(), other: Some(color) };
        return Ok(Scheme { colors, ..Default::default() });
    }
    let file = open(name).map_err(|e| anyhow!("{name}: not a builtin colorscheme or a color, and {e}"))?;
    let text = io::read_to_string(file).with_context(|| name.to_string())?;
    parse_colorscheme(&text).with_context(|| name.to_string())
}

/// Get colorschemes by name, color, or file path. See [`read_scheme`].
///
/// # Arguments
/// * `names` - Names of `builtins`, colors, or paths of colorscheme files. Empty names are skipped, so `[""]` gives no scheme.
/// * `builtins` - Colorschemes by name
///
/// # Returns
/// The schemes in the order of `names`.
///
/// # Errors
/// The first error from reading or parsing a file.
pub fn read_schemes(names: &[String], builtins: &BTreeMap<String, Scheme>) -> Result<Vec<Scheme>> {
    names.iter().filter(|name| !name.is_empty()).map(|name| read_scheme(name, builtins)).collect()
}

/// Merge the colors of schemes for one sequence type.
///
/// # Arguments
/// * `schemes` - Later schemes take precedence
/// * `seq_type` - Sequence type of the colors to merge
///
/// # Returns
/// Color of each character. A scheme with `other` replaces the colors of the schemes before it (see [`CharMap::overlay`]).
pub fn merge_colors<'a>(schemes: impl IntoIterator<Item = &'a Scheme>, seq_type: SeqType) -> CharMap<Color> {
    schemes.into_iter().fold(CharMap::default(), |colors, scheme| colors.overlay(&scheme.colors(seq_type)))
}

/// Write colorscheme list with aligned descriptions and word wrapping.
///
/// # Arguments
/// * `output` - Destination of the list
/// * `schemes` - Colorschemes by name. Each name is written with its table keywords in the order of
///   [`bio::TABLE_ORDER`], e.g. "taylor (nucl, aa)". A scheme without tables is written with "text",
///   e.g. "bits_rgb (text)", before the other schemes. An empty description writes the name only.
///
/// # Errors
/// The first write error.
pub fn write_colorscheme_list(output: &mut impl Write, schemes: &BTreeMap<String, Scheme>) -> io::Result<()> {
    let mut sorted_schemes: Vec<_> = schemes.iter().collect();
    sorted_schemes.sort_by_key(|(_, scheme)| !scheme.tables.is_empty());
    let labels: Vec<(String, &str)> = sorted_schemes
        .into_iter()
        .map(|(name, scheme)| {
            let keywords = if scheme.tables.is_empty() {
                "text".to_string()
            } else {
                bio::TABLE_ORDER.into_iter().filter(|keyword| scheme.tables.contains_key(keyword)).collect::<Vec<_>>().join(", ")
            };
            (format!("{name} ({keywords})"), scheme.description.as_str())
        })
        .collect();

    // Find the longest name for alignment
    let max_name_len = labels.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let indent = " ".repeat(max_name_len + 2);

    let options = textwrap::Options::with_termwidth()
        .initial_indent("")
        .subsequent_indent(&indent);

    for (name, description) in labels {
        if description.is_empty() {
            writeln!(output, "{}", name)?;
        } else {
            let line = format!("{:width$}  {}", name, description, width = max_name_len);
            writeln!(output, "{}", textwrap::fill(&line, &options))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writer whose reader has closed.
    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn parse_style_reads_attributes_and_color() {
        assert_eq!(parse_style("bold"), Ok(Style::new().bold()));
        assert_eq!(parse_style("255 0 0"), Ok(Style::new().bg(Rgb(255, 0, 0))));
        assert_eq!(parse_style("Bold red"), Ok(Style::new().bold().bg(Red)));
        assert_eq!(parse_style("bright red underline"), Ok(Style::new().underline().bg(BrightRed)));
        for invalid in ["nocolor", "", " ", "300 0 0", "blod 255 0 0", "bold,red", "1 2 3 4", "-red", "!#ff0000"] {
            assert!(parse_style(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn write_colorscheme_list_returns_write_error() {
        let err = write_colorscheme_list(&mut ClosedPipe, &load_colorschemes()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn write_colorscheme_list_shows_types() {
        let mut output = Vec::new();
        write_colorscheme_list(&mut output, &load_colorschemes()).unwrap();
        let list = String::from_utf8(output).unwrap();
        assert!(list.contains("taylor (nucl, aa)"), "{list}");
        assert!(list.contains("clustal (nucl, aa)") && list.contains("igv (nucl)"), "{list}");
        assert!(list.starts_with("bits_rgb (text)") && list.contains("\ncharacter_class (text)"), "{list}");
    }

    #[test]
    fn character_class_colors_by_category() {
        let schemes = load_colorschemes();
        let colors = &schemes["character_class"].colors.by_char;
        for (c, hex) in [('A', "df7f78"), ('a', "6cb26f"), ('0', "61a3e6"), ('!', "c1983a"), ('+', "bf85cd")] {
            assert_eq!(colors.get(&c), Some(&parse_hex(hex)), "{c:?}");
        }
        assert!(!colors.contains_key(&'\u{0301}') && !colors.contains_key(&'\u{00AD}'));
    }

    #[test]
    fn igv_and_clustal_color_nucleotides() {
        let schemes = load_colorschemes();
        assert_eq!(schemes["igv"].colors(SeqType::Dna).get('A'), Some(&parse_hex("00c800")));
        assert_eq!(schemes["igv"].colors(SeqType::Rnax).get('N'), Some(&parse_hex("505050")));
        assert_eq!(schemes["clustal"].colors(SeqType::Dna).get('T'), Some(&parse_hex("19cc19")));
        assert_eq!(schemes["clustal"].colors(SeqType::AA).get('A'), Some(&parse_hex("80a0f0")));
    }

    #[test]
    fn bits_rgb_colors_by_code_point_bits() {
        let schemes = load_colorschemes();
        let colors = &schemes["bits_rgb"].colors.by_char;
        assert_eq!(colors.get(&'\u{01FF}'), Some(&Rgb(255, 255, 255)));
        assert_eq!(colors.get(&'\u{0200}'), Some(&Rgb(0, 0, 0)));
    }

    #[test]
    fn text_schemes_cover_the_same_characters() {
        let schemes = load_colorschemes();
        let chars = |name: &str| schemes[name].colors.by_char.keys().copied().collect::<std::collections::HashSet<char>>();
        assert_eq!(chars("character_class").len(), 1132);
        assert_eq!(chars("character_class"), chars("bits_rgb"));
    }

    #[test]
    fn parse_colorscheme_reads_type_tables() {
        let text = "description = \"Both.\"\n[colors]\n\"-\" = \"gray\"\n\"#\" = \"red\"\n[nucl]\nA = \"#ff0000\"\n[aa]\nA = \"0 0 255\"\n";
        let scheme = parse_colorscheme(text).unwrap();
        let gray = Rgb(127, 127, 127);
        assert_eq!(scheme.colors(SeqType::Dna).by_char, HashMap::from([('-', gray), ('#', Red), ('A', Rgb(255, 0, 0))]));
        assert_eq!(scheme.colors(SeqType::AA).by_char, HashMap::from([('-', gray), ('#', Red), ('A', Rgb(0, 0, 255))]));
        assert_eq!(scheme.description, "Both.");
    }

    #[test]
    fn parse_colorscheme_lets_type_tables_override_colors() {
        let scheme = parse_colorscheme("[colors]\nA = \"red\"\n[aa]\nA = \"blue\"\n").unwrap();
        assert_eq!(scheme.colors(SeqType::Dna).by_char, HashMap::from([('A', Red)]));
        assert_eq!(scheme.colors(SeqType::AA).by_char, HashMap::from([('A', Blue)]));
    }

    #[test]
    fn specific_tables_override_general_tables() {
        let scheme = parse_colorscheme("[nucl]\nA = \"red\"\n[dna]\nT = \"blue\"\n").unwrap();
        assert_eq!(scheme.colors(SeqType::Dna).by_char, HashMap::from([('A', Red), ('T', Blue)]));
        assert_eq!(scheme.colors(SeqType::Rna).by_char, HashMap::from([('A', Red)]));
        let scheme = parse_colorscheme("[aa]\nA = \"red\"\n[aax]\nA = \"blue\"\nX = \"blue\"\n").unwrap();
        assert_eq!(scheme.colors(SeqType::AA).by_char, HashMap::from([('A', Red), ('X', Blue)]));
    }

    #[test]
    fn parse_colorscheme_returns_color_error() {
        let err = format!("{:#}", parse_colorscheme("[colors]\nA = \"nocolor\"\n").unwrap_err());
        assert!(err.contains("\"A\"") && err.contains("nocolor"), "{err}");
    }

    #[test]
    fn parse_colorscheme_returns_key_and_table_errors() {
        let err = format!("{:#}", parse_colorscheme("[colors]\nAB = \"red\"\n").unwrap_err());
        assert!(err.contains("\"AB\""), "{err}");
        for text in ["[all]\nA = \"red\"\n", "name = \"x\"\n", "A\t#ff0000\n"] {
            assert!(parse_colorscheme(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn parse_color_needs_hash_for_hex_and_no_leading_characters() {
        assert_eq!(parse_color("#ff0000"), Ok(Rgb(255, 0, 0)));
        for invalid in ["./black", "facade", "ff0000"] {
            assert!(parse_color(invalid).is_err(), "{invalid:?}");
        }
    }

    fn uniform(color: Color) -> CharMap<Color> {
        CharMap { by_char: HashMap::new(), other: Some(color) }
    }

    #[test]
    fn read_schemes_reads_a_color_before_a_file() {
        let builtins = load_colorschemes();
        let [scheme] = read_schemes(&["black".into()], &builtins).unwrap().try_into().unwrap();
        assert_eq!(scheme.colors, uniform(Black));
        assert!(scheme.tables.is_empty());
        assert_eq!(scheme.alphabet(), CharMap { by_char: HashMap::new(), other: Some(true) });

        let dir = std::env::temp_dir().join(format!("seqcol-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("black");
        std::fs::write(&path, "[colors]\nA = \"red\"\n").unwrap();
        let [scheme] = read_schemes(&[path.to_str().unwrap().into()], &builtins).unwrap().try_into().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(scheme.colors, CharMap { by_char: HashMap::from([('A', Red)]), other: None });
    }

    #[test]
    fn alphabet_has_keys_and_keyword_alphabets() {
        let scheme = parse_colorscheme("[colors]\n\"#\" = \"red\"\n[dna]\nX = \"red\"\n").unwrap();
        assert_eq!(scheme.alphabet(), "#XACGTN-.".chars().collect());
    }

    #[test]
    fn read_scheme_error_names_all_readings() {
        let err = read_scheme("noscheme", &load_colorschemes()).unwrap_err().to_string();
        assert!(err.starts_with("noscheme: not a builtin colorscheme or a color, and No such file"), "{err}");
    }

    #[test]
    fn merge_colors_lets_a_uniform_scheme_replace_earlier_colors() {
        let builtins = load_colorschemes();
        let black = read_scheme("black", &builtins).unwrap();
        let taylor = &builtins["taylor"];
        assert_eq!(merge_colors([taylor, &black], SeqType::Dna), uniform(Black));
        let merged = merge_colors([&black, taylor], SeqType::Dna);
        assert_eq!(merged, CharMap { other: Some(Black), ..taylor.colors(SeqType::Dna) });
    }

    #[test]
    fn builtins_have_colors_for_their_types() {
        let text_schemes = ["bits_rgb", "character_class", "gaps"];
        for (name, scheme) in load_colorschemes() {
            assert!(!scheme.description.is_empty(), "{name}");
            assert_eq!(scheme.tables.is_empty(), text_schemes.contains(&name.as_str()), "{name}");
            assert!(scheme.tables.values().all(|table| !table.is_empty()), "{name}");
        }
    }
}

