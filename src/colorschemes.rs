use anyhow::Result;
use include_dir::include_dir;
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use yansi::Color::{self, *};

use crate::inout::open;
use crate::ansi_colors::{COLOR_NAMES,parse_hex};

pub fn get_colorscheme_names() -> Vec<String> {
    let mut colorschemes = Vec::new();
    // Read at compile time, i.e. no performance penalty at run-time for file io.
    for file in include_dir!("data/colorschemes/").files() {
        let filename = file.path().file_name().unwrap();
        let name = filename.to_str().unwrap().strip_suffix(".tsv").unwrap();
        colorschemes.push(name.to_string());
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
            match line.split_once('\t') {
                None => {} // Ignore empty lines.
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

    match re_hex.captures(coltext) {
        Some(m) => {
            return Ok(parse_hex(m[1].into()));
        }
        None => {}
    }
    match re_rgb.captures(coltext) {
        Some(m) => {
            let r = m[1].parse::<u8>().unwrap();
            let g = m[2].parse::<u8>().unwrap();
            let b = m[3].parse::<u8>().unwrap();
            return Ok(Rgb(r, g, b));
        }
        None => {}
    }
    match re_name.find(coltext) {
        Some(m) => {
            let col_name = m.as_str().to_lowercase().replace(' ', "");
            return match COLOR_NAMES.get(&col_name) {
                None => Err("Uknown color name."),
                Some(col) => Ok(*col)
            }
        }
        None => {}
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

