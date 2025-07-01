use anyhow::Result;
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use std::process::exit;

// For detecting if terminal can show true colors, etc.
use anstyle_query;
// For abstracting away writing ANSI codes.
use yansi::{Color, Color::*, Paint, Style};

use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};

mod colorschemes;
mod inout;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        help = "Text containing sequences. Default is reading stdin."
    )]
    files: Vec<String>,

    // #[arg(
    //     short('b'),
    //     long("bw"),
    //     help = "Print sequence letters with black and white foreground, rather than using the terminals primary colors."
    // )]
    // blackwhite: bool,
    //
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

    #[arg(short('T'), long("transpose"), help = "Transpose the input.")]
    transpose: bool,

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
        help = "Colorscheme file(s). Each line should contain a character, then a delimiter, e.g. tab or comma, then a color name, hex, or \
        integer triplet, e.g. delimiting integers with spaces or commas. Can \
        be used in combination with -s/--scheme to modify an exisiting colorscheme."
    )]
    colorscheme_files: Option<Vec<String>>,

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

    match args.colorscheme_files {
        None => {}
        Some(paths) => {
            for path in paths {
                let _colors = colorschemes::read_colorscheme(&path)?;
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

    // TEMP: dimmed gap builtin.
    styles.insert('-', Rgb(128, 128, 128).foreground());

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

    let re_min_len = match args.min_seq_length {
        None => None,
        Some(min_seq_length) => {
            // Build regex of min length of matches taken from the colorscheme alphabet.
            let mut alphabet = Vec::new();
            for c in styles.keys() {
                // characters with special meaning inside regex [...]
                if "^[]-".contains(*c) {
                    alphabet.push('\\');
                }
                alphabet.push(*c);
            }
            let alphabet: String = alphabet.iter().collect();
            Some(Regex::new(format!("[{alphabet}]{{{min_seq_length},}}").as_str()).unwrap())
        }
    };

    let re = match args.regex {
        None => None,
        Some(regex) => Some(Regex::new(regex.as_str()).expect("Uknown regex.")),
    };

    if !args.transpose {
        for filename in args.files {
            match inout::open(&filename) {
                Err(e) => eprintln!("{filename}: {e}"),
                Ok(file) => {
                    for line_result in file.lines() {
                        let line = line_result?;
                        // In case of fasta, skip coloring lines starting with '>'
                        if !args.no_fasta_check && line.starts_with('>') {
                            println!("{}", line);
                        } else {
                            match &re_min_len {
                                None => {
                                    ansiprint(&styles, &line);
                                    println!();
                                }
                                Some(_re_min_len) => {
                                    let mut i = 0;
                                    for m in _re_min_len.find_iter(&line) {
                                        print!("{}", &line[i..m.start()]);
                                        ansiprint(&styles, m.as_str());
                                        i = m.end();
                                    }
                                    println!("{}", &line[i..]);
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        let mut lines = Vec::new();
        let mut max_line = 0;
        for filename in args.files {
            match inout::open(&filename) {
                Err(e) => eprintln!("{filename}: {e}"),
                Ok(file) => {
                    for line_result in file.lines() {
                        let line = line_result?;
                        max_line = max_line.max(line.len());
                        lines.push(line);
                    }
                }
            }
        }
        for j in 0..max_line {
            for line in &lines {
                match line.chars().nth(j) {
                    None => print!(" "),
                    Some(c) => _ansiprint(&styles, c),
                }
            }
            println!();
        }
    }
    Ok(())
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

fn ansiprint(styles: &HashMap<char, Style>, text: &str) {
    for c in text.chars() {
        _ansiprint(styles, c);
    }
}
fn _ansiprint(styles: &HashMap<char, Style>, c: char) {
    match styles.get(&c) {
        Some(style) => print!("{}", c.paint(*style)),
        None => print!("{}", c),
    }
}
