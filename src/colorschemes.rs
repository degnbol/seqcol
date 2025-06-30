use anyhow::Result;
use include_dir::include_dir;
use phf::phf_map;
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use yansi::Color::{self, *};

use crate::inout::open;

static COLOR_NAMES: phf::Map<&'static str, Color> = phf_map! {
    "black"         => Black,
    "red"           => Red,
    "green"         => Green,
    "yellow"        => Yellow,
    "blue"          => Blue,
    "magenta"       => Magenta,
    "cyan"          => Cyan,
    "white"         => White,
    "brightblack"   => BrightBlack,
    "brightred"     => BrightRed,
    "brightgreen"   => BrightGreen,
    "brightyellow"  => BrightYellow,
    "brightblue"    => BrightBlue,
    "brightmagenta" => BrightMagenta,
    "brightcyan"    => BrightCyan,
    "brightwhite"   => BrightWhite,
    "primary"       => Primary,
};

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

pub fn read_colorscheme(path: &str) -> Result<HashMap<char, Color>> {
    match open(path) {
        Err(e) => panic!("{path}: {e}"),
        Ok(file) => {
            let mut colorscheme = HashMap::new();

            let re_hex = Regex::new(r"^[^0-9A-Za-z]*#?([0-9a-fA-F]{6})$").unwrap();
            let re_rgb = Regex::new(r"([0-9]+)[\s,]+([0-9]+)[\s,]+([0-9]+)$").unwrap();
            let re_name = Regex::new(r"[A-Za-z ]+[0-9]*$").unwrap();

            for line_result in file.lines() {
                let line = line_result?;
                let mut chars = line.chars();
                match chars.next() {
                    None => {} // Ignore empty lines.
                    Some(c) => {
                        let coltext = chars.as_str().trim();
                        match re_hex.captures(coltext) {
                            Some(m) => {
                                let col = parse_hex(m[1].into());
                                colorscheme.insert(c, col);
                            }
                            None => match re_rgb.captures(coltext) {
                                Some(m) => {
                                    let r = m[1].parse::<u8>().unwrap();
                                    let g = m[2].parse::<u8>().unwrap();
                                    let b = m[3].parse::<u8>().unwrap();
                                    let col = Rgb(r, g, b);
                                    colorscheme.insert(c, col);
                                }
                                None => match re_name.find(coltext) {
                                    Some(m) => {
                                        let col_name = m.as_str().to_lowercase().replace(' ', "");
                                        let col = COLOR_NAMES.get(&col_name).expect(coltext);
                                        colorscheme.insert(c, *col);
                                    }
                                    None => {
                                        panic!("Color description not understood: {coltext}")
                                    }
                                },
                            },
                        }
                    }
                }
            }
            Ok(colorscheme)
        }
    }
}

// Parse 6 char long hex string.
fn parse_hex(hex: &str) -> Color {
    let r = u8::from_str_radix(&hex[0..2], 16).expect(hex);
    let g = u8::from_str_radix(&hex[2..4], 16).expect(hex);
    let b = u8::from_str_radix(&hex[4..6], 16).expect(hex);
    Rgb(r, g, b)
}
