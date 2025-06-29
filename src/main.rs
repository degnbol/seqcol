use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process::exit;

// For detecting if terminal can show true colors, etc.
use anstyle_query;
// For abstracting away writing ANSI codes.
use yansi::{Color, Color::*, Paint};

use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};

mod colorschemes;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        help = "Text containing sequences."
    )]
    files: Vec<String>,

    // #[arg(
    //     short('b'),
    //     long("bw"),
    //     help = "Print sequence letters with black and white foreground, rather than using the terminals primary colors."
    // )]
    // blackwhite: bool,
    #[arg(
        short('i'),
        long("invisible"),
        help = "Hide letter codes by printing text foreground color the same as background color."
    )]
    invisible: bool,

    #[arg(
        short('F'),
        long("no-fasta"),
        help = "By default lines starting with '>' are ignored."
    )]
    no_fasta_check: bool,

    #[arg(short('m'), long("min"), help = "Minimum sequence length to color.")]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long("regex"),
        help = "Color sequences matching the given regex."
    )]
    regex: Option<String>,

    #[arg(
        short('s'),
        long("scheme"),
        help = "Name of predefined colorscheme. Flag can be specified multiple times where \
        definitions in subsequent color schemes take precedence over previous. \
        Use -l/--list-schemes to get list of available colorschemes. \
        If neiter -s/--scheme or -c/--custom is provided then default is \"shapely_aa\"."
    )]
    colorscheme: Option<Vec<String>>,

    #[arg(
        short('c'),
        long("custom"),
        help = "Colorscheme file. Each line should contain a character, then a delimiter, e.g. tab or comma, then a color name, hex, or \
        integer triplet, e.g. delimiting integers with spaces or commas. Can \
        be used in combination with -s/--scheme to modify an exisiting colorscheme."
    )]
    colorscheme_file: Option<String>,

    #[arg(
        short('l'),
        long("list-schemes"),
        help = "List available colorschemes."
    )]
    list_colorschemes: bool,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    if args.list_colorschemes {
        let names = colorschemes::get_colorscheme_names();
        println!("{}", names.join("\n"));
        exit(0)
    }

    let schemes = colorschemes::load_colorschemes();

    let mut colors: HashMap<char, Color>;
    match args.colorscheme {
        None => {
            colors = schemes
                .get("shapely_aa")
                .expect("Unkown colorscheme")
                .clone()
        }
        Some(scheme_names) => {
            colors = HashMap::new();
            for scheme_name in scheme_names {
                let _colors = schemes.get(&scheme_name).expect("Unkown colorscheme");
                colors.extend(_colors);
            }
        }
    }

    let mut ansi_colors = HashMap::new();
    if anstyle_query::truecolor() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, col);
        }
    } else if anstyle_query::term_supports_ansi_color() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, Fixed(ansi256(col)));
        }
    } else if anstyle_query::term_supports_color() {
        unimplemented!()
    } else {
        unimplemented!()
    }

    let mut styles = HashMap::new();
    if args.invisible {
        for (c, col) in ansi_colors.into_iter() {
            styles.insert(c, col.background().fg(col));
        }
    } else {
        // Make text legible by using dark text with light bg, and light text with dark bg.
        // We can either explicitly set the text fg to black and white, or use inversion to use the
        // terminal colours. Here we wanted to do the latter but it breaks the pager.
        for (c, col) in ansi_colors.into_iter() {
            if is_light(col) {
                styles.insert(c, col.background().fg(Black));
            } else {
                styles.insert(c, col.background().fg(White));
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
                    if !args.no_fasta_check && line.starts_with('>') {
                        println!("{}", line);
                    } else {
                        for c in line.chars() {
                            match styles.get(&c) {
                                Some(style) => print!("{}", c.paint(*style)),
                                None => print!("{}", c),
                            }
                        }
                        println!();
                        // println!("{}", "".resetting());
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
        // Simple relative luminance calculation for roughly and efficiently approximating the
        // perceived lightness of a colour.
        Rgb(r, g, b) => r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722 > 128.,
        // Not sure how useful/meaningful, but here for completeness.
        Primary => false,
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
        Primary => 15, // not known but not used
    }
}
