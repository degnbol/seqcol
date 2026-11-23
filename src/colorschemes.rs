use anyhow::{anyhow, Context, Result};
use include_dir::include_dir;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use serde::Deserialize;
use std::io::{self, Write};
use std::sync::LazyLock;
use yansi::Color::{self, *};
use yansi::Style;

use crate::bio::SeqType;
use crate::inout::open;
use crate::ansi_colors::{COLOR_NAMES,parse_hex};

/// Colors of characters for each sequence type.
#[derive(Clone, Debug, Default)]
pub struct Scheme {
    pub nucl: HashMap<char, Color>,
    pub aa: HashMap<char, Color>,
    /// Description of the colorscheme. Empty if it has none.
    pub description: String,
}

impl Scheme {
    /// Colors of characters in sequences of type `seq_type`.
    pub fn colors(&self, seq_type: SeqType) -> &HashMap<char, Color> {
        match seq_type {
            SeqType::Nucl => &self.nucl,
            SeqType::AA => &self.aa,
        }
    }
}

/// Parse a colorscheme.
///
/// # Arguments
/// * `text` - TOML with an optional `description` string, and the optional tables `colors` (both sequence types),
///   `nucl` and `aa` (override `colors` for that type). Each table maps characters to colors (see [`parse_color`]),
///   e.g. `"#" = "red"`.
///
/// # Returns
/// The colors and the description.
///
/// # Errors
/// On invalid TOML, an unknown key or table, a key that is not one character, or a color that does not parse.
pub fn parse_colorscheme(text: &str) -> Result<Scheme> {
    let file: SchemeFile = toml::from_str(text)?;
    let colors = parse_colors(&file.colors)?;
    let mut nucl = colors.clone();
    nucl.extend(parse_colors(&file.nucl)?);
    let mut aa = colors;
    aa.extend(parse_colors(&file.aa)?);
    Ok(Scheme { nucl, aa, description: file.description })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemeFile {
    #[serde(default)]
    description: String,
    #[serde(default)]
    colors: BTreeMap<String, String>,
    #[serde(default)]
    nucl: BTreeMap<String, String>,
    #[serde(default)]
    aa: BTreeMap<String, String>,
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
    static RE_HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#?([0-9a-fA-F]{6})$").unwrap());
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

/// Read a colorscheme file. See [`parse_colorscheme`].
///
/// # Errors
/// If the file does not open, read, or parse. The message starts with `path`.
pub fn read_colorscheme(path: &str) -> Result<Scheme> {
    let text = open(path).and_then(|file| Ok(io::read_to_string(file)?)).with_context(|| path.to_string())?;
    parse_colorscheme(&text).with_context(|| path.to_string())
}

/// Get colorschemes by name or file path.
///
/// # Arguments
/// * `names` - Names of `builtins`, else paths of colorscheme files. Empty names are skipped, so `[""]` gives no scheme.
/// * `builtins` - Colorschemes by name
///
/// # Returns
/// The schemes in the order of `names`.
///
/// # Errors
/// The first error from reading or parsing a file.
pub fn read_schemes(names: &[String], builtins: &BTreeMap<String, Scheme>) -> Result<Vec<Scheme>> {
    names
        .iter()
        .filter(|name| !name.is_empty())
        .map(|name| builtins.get(name).cloned().map_or_else(|| read_colorscheme(name), Ok))
        .collect()
}

/// Merge the colors of schemes for one sequence type.
///
/// # Arguments
/// * `schemes` - Later schemes take precedence
/// * `seq_type` - Sequence type of the colors to merge
///
/// # Returns
/// Color of each character.
pub fn merge_colors(schemes: &[Scheme], seq_type: SeqType) -> HashMap<char, Color> {
    schemes.iter().flat_map(|scheme| scheme.colors(seq_type)).map(|(&c, &color)| (c, color)).collect()
}

/// Write colorscheme list with aligned descriptions and word wrapping.
///
/// # Arguments
/// * `output` - Destination of the list
/// * `schemes` - Colorschemes by name. Each name is written with the sequence types it has colors for,
///   e.g. "taylor (nucl, aa)". A scheme with the same colors for all sequence types is written with "text",
///   e.g. "bits_rgb (text)", before the other schemes. An empty description writes the name only.
///
/// # Errors
/// The first write error.
pub fn write_colorscheme_list(output: &mut impl Write, schemes: &BTreeMap<String, Scheme>) -> io::Result<()> {
    let mut sorted_schemes: Vec<_> = schemes.iter().collect();
    sorted_schemes.sort_by_key(|(_, scheme)| scheme.nucl != scheme.aa);
    let labels: Vec<(String, &str)> = sorted_schemes
        .into_iter()
        .map(|(name, scheme)| {
            let seq_types = if scheme.nucl == scheme.aa {
                "text".to_string()
            } else {
                SeqType::ALL
                    .into_iter()
                    .filter(|&seq_type| !scheme.colors(seq_type).is_empty())
                    .map(|seq_type| seq_type.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            (format!("{name} ({seq_types})"), scheme.description.as_str())
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
        assert!(list.contains("clustal (aa)"), "{list}");
        assert!(list.starts_with("bits_rgb (text)") && list.contains("\ncharacter_class (text)"), "{list}");
    }

    #[test]
    fn character_class_colors_by_category() {
        let schemes = load_colorschemes();
        let colors = &schemes["character_class"].nucl;
        for (c, hex) in [('A', "df7f78"), ('a', "6cb26f"), ('0', "61a3e6"), ('!', "c1983a"), ('+', "bf85cd")] {
            assert_eq!(colors.get(&c), Some(&parse_hex(hex)), "{c:?}");
        }
        assert!(!colors.contains_key(&'\u{0301}') && !colors.contains_key(&'\u{00AD}'));
    }

    #[test]
    fn bits_rgb_colors_by_code_point_bits() {
        let schemes = load_colorschemes();
        let colors = &schemes["bits_rgb"].nucl;
        assert_eq!(colors.get(&'\u{01FF}'), Some(&Rgb(255, 255, 255)));
        assert_eq!(colors.get(&'\u{0200}'), Some(&Rgb(0, 0, 0)));
    }

    #[test]
    fn text_schemes_cover_the_same_characters() {
        let schemes = load_colorschemes();
        let chars = |name: &str| schemes[name].nucl.keys().copied().collect::<std::collections::HashSet<char>>();
        assert_eq!(chars("character_class").len(), 1132);
        assert_eq!(chars("character_class"), chars("bits_rgb"));
    }

    #[test]
    fn parse_colorscheme_reads_type_tables() {
        let text = "description = \"Both.\"\n[colors]\n\"-\" = \"gray\"\n\"#\" = \"red\"\n[nucl]\nA = \"#ff0000\"\n[aa]\nA = \"0 0 255\"\n";
        let scheme = parse_colorscheme(text).unwrap();
        let gray = Rgb(127, 127, 127);
        assert_eq!(scheme.nucl, HashMap::from([('-', gray), ('#', Red), ('A', Rgb(255, 0, 0))]));
        assert_eq!(scheme.aa, HashMap::from([('-', gray), ('#', Red), ('A', Rgb(0, 0, 255))]));
        assert_eq!(scheme.description, "Both.");
    }

    #[test]
    fn parse_colorscheme_lets_type_tables_override_colors() {
        let scheme = parse_colorscheme("[colors]\nA = \"red\"\n[aa]\nA = \"blue\"\n").unwrap();
        assert_eq!(scheme.nucl, HashMap::from([('A', Red)]));
        assert_eq!(scheme.aa, HashMap::from([('A', Blue)]));
    }

    #[test]
    fn parse_colorscheme_returns_color_error() {
        let err = parse_colorscheme("[colors]\nA = \"nocolor\"\n").unwrap_err().to_string();
        assert!(err.contains("\"A\"") && err.contains("nocolor"), "{err}");
    }

    #[test]
    fn parse_colorscheme_returns_key_and_table_errors() {
        let err = parse_colorscheme("[colors]\nAB = \"red\"\n").unwrap_err().to_string();
        assert!(err.contains("\"AB\""), "{err}");
        for text in ["[dna]\nA = \"red\"\n", "name = \"x\"\n", "A\t#ff0000\n"] {
            assert!(parse_colorscheme(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn builtins_have_colors_for_their_types() {
        let both = ["bits_rgb", "character_class", "chemistry", "shapely", "taylor", "zappo"];
        for (name, scheme) in load_colorschemes() {
            let n_types = SeqType::ALL.into_iter().filter(|&seq_type| !scheme.colors(seq_type).is_empty()).count();
            assert_eq!(n_types, if both.contains(&name.as_str()) { 2 } else { 1 }, "{name}");
            assert!(!scheme.description.is_empty(), "{name}");
        }
    }
}

