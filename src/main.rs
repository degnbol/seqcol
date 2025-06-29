use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

use phf::{Map, phf_map};

// For detecting if terminal can show true colors, etc.
use anstyle_query;
// For abstracting away writing ANSI codes.
use yansi::{Color, Color::*, Paint, Style};

// For detecting light or dark terminal
use terminal_colorsaurus::{ColorScheme, QueryOptions, color_scheme};

use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};

static SHAPELY_AA: Map<char, Color> = phf_map! {
     'D' => Rgb(230,10,10),
     'E' => Rgb(230,10,10),
     'C' => Rgb(230,230,0),
     'M' => Rgb(230,230,0),
     'K' => Rgb(20,90,255),
     'R' => Rgb(20,90,255),
     'S' => Rgb(250,150,0),
     'T' => Rgb(250,150,0),
     'F' => Rgb(50,50,170),
     'Y' => Rgb(50,50,170),
     'N' => Rgb(0,220,220),
     'Q' => Rgb(0,220,220),
     'G' => Rgb(235,235,235),
     'L' => Rgb(15,130,15),
     'I' => Rgb(15,130,15),
     'V' => Rgb(15,130,15),
     'A' => Rgb(200,200,200),
     'W' => Rgb(180,90,180),
     'H' => Rgb(130,130,210),
     'P' => Rgb(220,150,130),
};
static SHAPELY_NUCL: Map<char, Color> = phf_map! {
     'A' => Rgb(160,160,255),
     'C' => Rgb(255,140,75),
     'G' => Rgb(255,112,112),
     'T' => Rgb(160,255,160),
     'U' => Rgb(184,184,184),
};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    // Input file(s)
    #[arg(value_name = "FILE", default_value = "-")]
    files: Vec<String>,

    #[arg(short('n'), long("nucl"))]
    nucl: bool,

    #[arg(short('s'), long("scheme"))]
    colorscheme: String,

    #[arg(short('c'), long("custom"))]
    colorscheme_file: String,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    let colors = if args.nucl {
        &SHAPELY_NUCL
    } else {
        &SHAPELY_AA
    };

    let mut ansi_colors = HashMap::new();
    if anstyle_query::truecolor() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, *col);
        }
    } else if anstyle_query::term_supports_ansi_color() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, Fixed(ansi256(*col)));
        }
    } else if anstyle_query::term_supports_color() {
        // TODO
    } else {
        // TODO
    }

    let mut styles = HashMap::new();
    // Make text legible by using dark text with light bg, and light text with dark bg.
    // We can either explicitly set the text fg to black and white, or use inversion to use the
    // terminal colours. Here we do the latter.
    if is_light_color_scheme() {
        for (c, col) in ansi_colors.into_iter() {
            if is_light(col) {
                styles.insert(c, col.background());
            } else {
                styles.insert(c, col.foreground().invert());
            }
        }
    } else {
        for (c, col) in ansi_colors.into_iter() {
            if is_light(col) {
                styles.insert(c, col.foreground().invert());
            } else {
                styles.insert(c, col.background());
            }
        }
    }

    for filename in args.files {
        match open(&filename) {
            Err(e) => eprintln!("{filename}: {e}"),
            Ok(file) => {
                for line_result in file.lines() {
                    let line = line_result?;
                    // In case of fasta, skip coloring lines starting with '>'
                    if line.starts_with('>') {
                        println!("{}", line);
                    } else {
                        for c in line.chars() {
                            match styles.get(&c) {
                                Some(style) => print!("{}", c.paint(*style)),
                                None => print!("{}", c),
                            }
                        }
                        println!();
                    }
                }
            }
        }
    }
    Ok(())
}

fn open(filename: &str) -> Result<Box<dyn BufRead>> {
    match filename {
        "-" => Ok(Box::new(BufReader::new(io::stdin()))),
        _ => Ok(Box::new(BufReader::new(File::open(filename)?))),
    }
}

fn is_light_color_scheme() -> bool {
    match color_scheme(QueryOptions::default()) {
        Ok(scheme) => match scheme {
            ColorScheme::Dark => false,
            ColorScheme::Light => true,
        },
        Err(_) => false,
    }
}

fn is_light(col: Color) -> bool {
    // Return whether a colour is light or dark.
    match col {
        Black => false,
        Red => false,
        // Varies by program:
        // https://stackoverflow.com/questions/4842424/list-of-ansi-color-escape-sequences
        // We go with the most common, otherwise would have to test which program someone uses.
        Green => false,
        Yellow => false,
        Blue => false,
        Magenta => false,
        Cyan => false,
        White => true,
        BrightBlack => false,
        BrightRed => true,
        BrightGreen => true,
        BrightYellow => true,
        BrightBlue => true,
        BrightMagenta => true,
        BrightCyan => true,
        BrightWhite => true,
        Fixed(idx) => {
            let (r, g, b) = rgb_from_ansi256(idx);
            is_light(Rgb(r, g, b))
        }
        // Note there are more complicated formula for the lightness perception of a colour.
        Rgb(r, g, b) => (r as u16 + g as u16 + b as u16) / 3 > 128,
        // Not sure how useful/meaningful, but here for completeness.
        Primary => is_light_color_scheme(),
    }
}

fn ansi256(col: Color) -> u8 {
    match col {
        Black => 0,
        Red => 124,
        Green => 2,
        Yellow => 184,
        Blue => 12,
        Magenta => 90,
        Cyan => 43,
        White => 255,
        BrightBlack => 238,
        BrightRed => 9,
        BrightGreen => 40,
        BrightYellow => 11,
        BrightBlue => 33,
        BrightMagenta => 13,
        BrightCyan => 14,
        BrightWhite => 15,
        Fixed(idx) => idx,
        Rgb(r, g, b) => ansi256_from_rgb(&[r, g, b]),
        Primary => {
            if is_light_color_scheme() {
                0
            } else {
                15
            }
        }
    }
}
