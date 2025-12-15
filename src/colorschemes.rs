use anyhow::Result;
use include_dir::include_dir;
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use yansi::Color::{self, *};

use crate::bio::SeqType;
use crate::inout::open;
use crate::ansi_colors::{COLOR_NAMES,parse_hex};

/// Returns a list of (name, description) pairs for all builtin colorschemes.
/// Description is extracted from comment lines starting with "# " at the top of the file.
pub fn get_colorscheme_names() -> Vec<(String, String)> {
    let mut colorschemes = Vec::new();
    // Read at compile time, i.e. no performance penalty at run-time for file io.
    for file in include_dir!("data/colorschemes/").files() {
        let filename = file.path().file_name().unwrap();
        let name = filename.to_str().unwrap().strip_suffix(".tsv").unwrap();

        // Extract description from comment lines at the top
        let contents = file.contents_utf8().unwrap();
        let description = contents
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| line.trim_start_matches('#').trim())
            .collect::<Vec<_>>()
            .join(" ");

        colorschemes.push((name.to_string(), description));
    }
    colorschemes
}

// Load the builtin colorschemes with hex colors.
// Read at compile time, i.e. no performance penalty at run-time for file io.
pub fn load_colorschemes() -> HashMap<String, HashMap<char, Color>> {
    let mut colorschemes = HashMap::new();

    for file in include_dir!("data/colorschemes/").files() {
        let filename = file.path().file_name().unwrap();
        let name = filename.to_str().unwrap().strip_suffix(".tsv").unwrap();

        let mut colorscheme = HashMap::new();
        let contents = file.contents_utf8().unwrap();
        for line in contents.split('\n') {
            // Skip comment lines and empty lines
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            match line.split_once('\t') {
                None => {} // Ignore lines without tab.
                Some((c, hex)) => {
                    let c = c.chars().next().unwrap(); // should be a 1 character string
                    // start from index 1 since first char is '#'.
                    let col = parse_hex(&hex[1..]);
                    colorscheme.insert(c, col);
                }
            }
        }

        colorschemes.insert(name.to_string(), colorscheme);
    }
    colorschemes
}

pub fn parse_color(coltext: &str) -> Result<Color, &'static str> {
    let re_hex = Regex::new(r"^[^0-9A-Za-z]*#?([0-9a-fA-F]{6})$").unwrap();
    let re_rgb = Regex::new(r"([0-9]+)[\s,]+([0-9]+)[\s,]+([0-9]+)$").unwrap();
    let re_name = Regex::new(r"[A-Za-z ]+[0-9]*$").unwrap();

    let coltext = coltext.trim();

    if let Some(m) = re_hex.captures(coltext) {
        return Ok(parse_hex(&m[1]));
    }
    if let Some(m) = re_rgb.captures(coltext) {
        let r = m[1].parse::<u8>().unwrap();
        let g = m[2].parse::<u8>().unwrap();
        let b = m[3].parse::<u8>().unwrap();
        return Ok(Rgb(r, g, b));
    }
    if let Some(m) = re_name.find(coltext) {
        let col_name = m.as_str().to_lowercase().replace(' ', "");
        return match COLOR_NAMES.get(&col_name) {
            None => Err("Unknown color name."),
            Some(col) => Ok(*col),
        };
    }
    Err("Color description couldn't be parsed.")
}

pub fn read_colorscheme(path: &str) -> Result<HashMap<char, Color>> {
    match open(path) {
        Err(e) => panic!("{path}: {e}"),
        Ok(file) => {
            let mut colorscheme = HashMap::new();

            for line_result in file.lines() {
                let line = line_result?;
                // Skip comment lines and empty lines
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                let mut chars = line.chars();
                match chars.next() {
                    None => {} // Ignore empty lines.
                    Some(c) => {
                        let coltext = chars.as_str();
                        colorscheme.insert(c, parse_color(coltext).expect(coltext));
                    }
                }
            }
            Ok(colorscheme)
        }
    }
}

/// Resolve a colorscheme name, potentially adding a suffix based on detected sequence type.
/// Returns resolved name or an error message.
pub fn resolve_scheme_name(
    name: &str,
    seq_type: Option<SeqType>,
    schemes: &HashMap<String, HashMap<char, Color>>,
) -> Result<String, String> {
    // 1. Try exact name first (backward compatible)
    if schemes.contains_key(name) {
        return Ok(name.to_string());
    }

    // 2. If name already has a recognized suffix, don't auto-resolve
    if name.ends_with("_aa") || name.ends_with("_nucl") {
        return Err(format!("Colorscheme '{}' not found", name));
    }

    // 3. Try to auto-resolve with detected sequence type
    if let Some(st) = seq_type {
        let suffixed = format!("{}{}", name, st.suffix());
        if schemes.contains_key(&suffixed) {
            return Ok(suffixed);
        }

        // Check what suffixes ARE available for this base name
        let available: Vec<String> = ["_aa", "_nucl"]
            .iter()
            .map(|s| format!("{}{}", name, s))
            .filter(|full| schemes.contains_key(full))
            .collect();

        if !available.is_empty() {
            return Err(format!(
                "Colorscheme '{}' not found for detected {} sequences. Available: {}",
                suffixed,
                match st {
                    SeqType::AA => "amino acid",
                    _ => "nucleotide",
                },
                available.join(", ")
            ));
        }
    }

    // 4. Not a builtin scheme - will be treated as file path by caller
    Err(format!("Colorscheme '{}' not found as builtin", name))
}

