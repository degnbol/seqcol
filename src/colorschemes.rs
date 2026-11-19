use anyhow::{anyhow, Context, Result};
use include_dir::include_dir;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
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
    /// Text of the `# ` comment lines before the first color, joined by spaces.
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
/// * `text` - Lines of a character followed by its color (see [`parse_color`]), e.g. `A\t#ff0000` or `A,255 0 0`.
///   The line `#type: nucl` or `#type: aa` starts a section of colors for that sequence type.
///   Colors before the first section are for both types.
///   Other lines that start with `#` are comments, and empty lines are skipped.
///
/// # Returns
/// The colors, and the description: the text of the `# ` comment lines before the first color, joined by spaces.
/// An error for a color that does not parse, or a `#type:` line with another type.
pub fn parse_colorscheme(text: &str) -> Result<Scheme> {
    let mut scheme = Scheme::default();
    let mut description_lines = vec![];
    let mut section = None;
    for line in text.lines() {
        match line {
            "#type: nucl" => section = Some(SeqType::Nucl),
            "#type: aa" => section = Some(SeqType::AA),
            _ if line.starts_with("#type:") => return Err(anyhow!("{line}: type is not \"nucl\" or \"aa\"")),
            "" => {}
            _ if line.starts_with('#') => {
                if scheme.nucl.is_empty() && scheme.aa.is_empty() {
                    description_lines.extend(line.strip_prefix("# ").map(str::trim));
                }
            }
            _ => {
                let mut chars = line.chars();
                let c = chars.next().unwrap();
                let color = parse_color(chars.as_str()).map_err(|e| anyhow!("{line}: {e}"))?;
                if section != Some(SeqType::AA) {
                    scheme.nucl.insert(c, color);
                }
                if section != Some(SeqType::Nucl) {
                    scheme.aa.insert(c, color);
                }
            }
        }
    }
    scheme.description = description_lines.join(" ");
    Ok(scheme)
}

/// Builtin colorschemes, by file name without extension.
///
/// # Panics
/// On a builtin that does not parse.
pub fn load_colorschemes() -> BTreeMap<String, Scheme> {
    include_dir!("data/colorschemes/")
        .files()
        .map(|file| {
            let name = file.path().file_stem().unwrap().to_str().unwrap().to_string();
            let scheme = parse_colorscheme(file.contents_utf8().unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, scheme)
        })
        .collect()
}

pub fn parse_color(coltext: &str) -> Result<Color, &'static str> {
    let re_hex = Regex::new(r"^[^0-9A-Za-z]*#?([0-9a-fA-F]{6})$").unwrap();
    let re_rgb = Regex::new(r"^[^0-9A-Za-z]*([0-9]+)[\s,]+([0-9]+)[\s,]+([0-9]+)$").unwrap();
    let re_name = Regex::new(r"^[^0-9A-Za-z]*([A-Za-z ]+[0-9]*)$").unwrap();

    let coltext = coltext.trim();

    if let Some(m) = re_hex.captures(coltext) {
        return Ok(parse_hex(&m[1]));
    }
    if let Some(m) = re_rgb.captures(coltext) {
        let channel = |i: usize| m[i].parse::<u8>().map_err(|_| "RGB value above 255.");
        return Ok(Rgb(channel(1)?, channel(2)?, channel(3)?));
    }
    if let Some(m) = re_name.captures(coltext) {
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
/// The style, or an error for text without words or with an invalid color.
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
/// The schemes in the order of `names`, or the first error from reading or parsing a file.
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
///   e.g. "taylor (nucl, aa)". An empty description writes the name only.
///
/// # Returns
/// The first write error, if any.
pub fn write_colorscheme_list(output: &mut impl Write, schemes: &BTreeMap<String, Scheme>) -> io::Result<()> {
    let labels: Vec<(String, &str)> = schemes
        .iter()
        .map(|(name, scheme)| {
            let seq_types: Vec<String> = SeqType::ALL
                .into_iter()
                .filter(|&seq_type| !scheme.colors(seq_type).is_empty())
                .map(|seq_type| seq_type.to_string())
                .collect();
            (format!("{name} ({})", seq_types.join(", ")), scheme.description.as_str())
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
        for invalid in ["nocolor", "", " ", "300 0 0", "blod 255 0 0", "bold,red", "1 2 3 4"] {
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
    }

    #[test]
    fn parse_colorscheme_reads_type_sections() {
        let scheme = parse_colorscheme("# Both.\n# More.\n-\tgray\n#type: nucl\nA\t#ff0000\n#type: aa\nA\t0 0 255\n").unwrap();
        let gray = Rgb(127, 127, 127);
        assert_eq!(scheme.nucl, HashMap::from([('-', gray), ('A', Rgb(255, 0, 0))]));
        assert_eq!(scheme.aa, HashMap::from([('-', gray), ('A', Rgb(0, 0, 255))]));
        assert_eq!(scheme.description, "Both. More.");
    }

    #[test]
    fn parse_colorscheme_returns_color_error() {
        assert!(parse_colorscheme("A\tnocolor\n").is_err());
    }

    #[test]
    fn parse_colorscheme_returns_unknown_type_error() {
        for text in ["#type: dna\n", "#type:aa\n", "#type: AA\n"] {
            assert!(parse_colorscheme(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn builtins_have_colors_for_their_types() {
        let both = ["chemistry", "letter", "shapely", "taylor", "zappo"];
        for (name, scheme) in load_colorschemes() {
            let n_types = SeqType::ALL.into_iter().filter(|&seq_type| !scheme.colors(seq_type).is_empty()).count();
            assert_eq!(n_types, if both.contains(&name.as_str()) { 2 } else { 1 }, "{name}");
            assert!(!scheme.description.is_empty(), "{name}");
        }
    }
}

